//! HTTP surface for town hall meetings (`/api/townhall/...`). The rules live in
//! `crate::townhall`; live state, turns and message ordering in
//! `crate::townhall_runtime`; the reach into a workspace in
//! `crate::townhall_bridge`. See `docs/townhall.md`.
//!
//! Clients poll `GET .../messages?since=` for new messages and read each
//! agent's presence from `GET .../meetings/{id}`. A realtime stream is a
//! planned replacement for polling, not part of this first slice.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::entities::{
    meeting, meeting_converse_rule, meeting_message, meeting_participant, townhall_agent,
};
use crate::error::AppError;
use crate::state::AppState;
use crate::townhall::{
    agent_listed_for, check_admission, may_assign_permanent, may_dismiss, may_reenroll,
    AdmissionError, DialogType, TurnMode,
};
use crate::townhall_bridge::WorkspaceClient;
use crate::townhall_runtime::{
    dispatch_for_message, effective_turn_mode, post_message, speaker_of, start_turn, Presence,
    TurnReason, RUNTIME,
};

const PERMISSION: &str = "meetings.use";

pub(super) fn gate(caller: &AuthUser) -> Result<(), AppError> {
    caller.require(PERMISSION)
}

// --- sessions and agents -------------------------------------------------

#[derive(Serialize)]
pub struct SessionItem {
    pub id: String,
    pub title: String,
    pub tool: String,
    pub status: String,
}

/// GET /api/townhall/sessions: the caller's structured aoe sessions, which are
/// the ones that can become agents.
pub async fn my_sessions(
    State(state): State<AppState>,
    caller: AuthUser,
) -> Result<Json<Vec<SessionItem>>, AppError> {
    gate(&caller)?;
    let client = WorkspaceClient::for_user(&state, caller.user.id).await?;
    let sessions = client.list_sessions().await?;
    Ok(Json(
        sessions
            .into_iter()
            .filter(|s| s.acp_capable && s.view == "structured")
            .map(|s| SessionItem {
                id: s.id,
                title: s.title,
                tool: s.tool,
                status: s.status,
            })
            .collect(),
    ))
}

#[derive(Serialize)]
pub struct AgentItem {
    pub id: i32,
    pub owner_user_id: i32,
    pub session_id: String,
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
}

fn agent_item(a: townhall_agent::Model) -> AgentItem {
    AgentItem {
        id: a.id,
        owner_user_id: a.owner_user_id,
        session_id: a.session_id,
        name: a.name,
        description: a.description,
        tools: serde_json::from_str(&a.tools).unwrap_or_default(),
    }
}

/// GET /api/townhall/agents: the agents the caller has registered.
pub async fn my_agents(
    State(state): State<AppState>,
    caller: AuthUser,
) -> Result<Json<Vec<AgentItem>>, AppError> {
    gate(&caller)?;
    let agents = townhall_agent::Entity::find()
        .filter(townhall_agent::Column::OwnerUserId.eq(caller.user.id))
        .order_by_asc(townhall_agent::Column::Id)
        .all(&state.db)
        .await?;
    Ok(Json(agents.into_iter().map(agent_item).collect()))
}

#[derive(Deserialize)]
pub struct RegisterAgent {
    /// An existing structured session to register. Left out, CityHall starts a
    /// new one for the agent.
    pub session_id: Option<String>,
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
}

pub(super) fn admission_error(e: AdmissionError) -> AppError {
    match e {
        AdmissionError::EmptyDescription => {
            AppError::BadRequest("an agent needs a description of its specialty")
        }
        AdmissionError::NoTools => AppError::BadRequest("an agent needs at least one tool"),
    }
}

/// POST /api/townhall/agents: register one of the caller's sessions as an agent.
pub async fn register_agent(
    State(state): State<AppState>,
    caller: AuthUser,
    Json(req): Json<RegisterAgent>,
) -> Result<Json<AgentItem>, AppError> {
    gate(&caller)?;
    let name = req.name.trim().to_string();
    if name.is_empty() || name.contains(char::is_whitespace) {
        // The name is what people @mention, so it cannot contain spaces.
        return Err(AppError::BadRequest(
            "an agent name is required and cannot contain spaces",
        ));
    }
    let tools: Vec<String> = req
        .tools
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let tools_json =
        serde_json::to_string(&tools).map_err(|_| AppError::Internal("failed to encode tools"))?;
    check_admission(&req.description, &tools_json).map_err(admission_error)?;

    let taken = townhall_agent::Entity::find()
        .filter(townhall_agent::Column::OwnerUserId.eq(caller.user.id))
        .filter(townhall_agent::Column::Name.eq(name.clone()))
        .one(&state.db)
        .await?;
    if taken.is_some() {
        return Err(AppError::Conflict(
            "you already have an agent with that name",
        ));
    }

    let client = WorkspaceClient::for_user(&state, caller.user.id).await?;
    let session_id = match req.session_id {
        Some(session_id) => {
            // An existing session must be structured and the caller's own.
            let sessions = client.list_sessions().await?;
            if !sessions
                .iter()
                .any(|s| s.id == session_id && s.acp_capable && s.view == "structured")
            {
                return Err(AppError::BadRequest(
                    "that is not a structured session in your workspace",
                ));
            }
            let duplicate = townhall_agent::Entity::find()
                .filter(townhall_agent::Column::OwnerUserId.eq(caller.user.id))
                .filter(townhall_agent::Column::SessionId.eq(session_id.clone()))
                .one(&state.db)
                .await?;
            if duplicate.is_some() {
                return Err(AppError::Conflict("that session is already registered"));
            }
            session_id
        }
        None => client.create_session(&name).await?,
    };

    let now = Utc::now();
    let agent = townhall_agent::ActiveModel {
        owner_user_id: Set(caller.user.id),
        session_id: Set(session_id),
        name: Set(name),
        description: Set(req.description.trim().to_string()),
        tools: Set(tools_json),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;
    Ok(Json(agent_item(agent)))
}

// --- meetings ------------------------------------------------------------

#[derive(Serialize)]
pub struct MeetingItem {
    pub id: i32,
    pub title: String,
    pub moderator_user_id: i32,
    pub status: String,
    pub allow_member_agents: bool,
    pub agent_mode_ceiling: String,
    pub max_consecutive_agent_turns: i32,
    pub max_consecutive_permanent_turns: i32,
    /// The most autonomous agent turns may cost in an hour, in US cents.
    pub autonomous_hourly_cents: i32,
}

fn meeting_item(m: &meeting::Model) -> MeetingItem {
    MeetingItem {
        id: m.id,
        title: m.title.clone(),
        moderator_user_id: m.moderator_user_id,
        status: m.status.clone(),
        allow_member_agents: m.allow_member_agents,
        agent_mode_ceiling: m.agent_mode_ceiling.clone(),
        max_consecutive_agent_turns: m.max_consecutive_agent_turns,
        max_consecutive_permanent_turns: m.max_consecutive_permanent_turns,
        autonomous_hourly_cents: m.autonomous_hourly_cents,
    }
}

#[derive(Deserialize)]
pub struct CreateMeeting {
    pub title: String,
    pub allow_member_agents: Option<bool>,
    pub agent_mode_ceiling: Option<String>,
    pub max_consecutive_agent_turns: Option<i32>,
}

pub(super) fn parse_mode(s: &str) -> Result<TurnMode, AppError> {
    TurnMode::parse(s).ok_or(AppError::BadRequest(
        "turn mode must be moderator_called, mention_only or every_message",
    ))
}

fn check_cap(n: i32) -> Result<i32, AppError> {
    if (1..=50).contains(&n) {
        Ok(n)
    } else {
        Err(AppError::BadRequest(
            "max_consecutive_agent_turns must be between 1 and 50",
        ))
    }
}

/// POST /api/townhall/meetings: the caller becomes its moderator.
pub async fn create_meeting(
    State(state): State<AppState>,
    caller: AuthUser,
    Json(req): Json<CreateMeeting>,
) -> Result<Json<MeetingItem>, AppError> {
    gate(&caller)?;
    let title = req.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::BadRequest("a meeting needs a title"));
    }
    let ceiling = match &req.agent_mode_ceiling {
        Some(s) => parse_mode(s)?,
        None => TurnMode::MentionOnly,
    };
    let cap = check_cap(req.max_consecutive_agent_turns.unwrap_or(3))?;
    let created = meeting::ActiveModel {
        title: Set(title),
        moderator_user_id: Set(caller.user.id),
        status: Set("open".to_string()),
        allow_member_agents: Set(req.allow_member_agents.unwrap_or(false)),
        agent_mode_ceiling: Set(ceiling.as_str().to_string()),
        max_consecutive_agent_turns: Set(cap),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;
    admit_user(&state.db, created.id, caller.user.id).await?;
    Ok(Json(meeting_item(&created)))
}

/// Put a person in the room: a new row, or, for someone who was dismissed, their
/// reopened one (the pair (meeting, user) is unique). Used for the moderator
/// when a meeting opens and when the moderator approves a waiting-room request.
pub(super) async fn admit_user(
    db: &DatabaseConnection,
    meeting_id: i32,
    user_id: i32,
) -> Result<meeting_participant::Model, AppError> {
    let earlier = meeting_participant::Entity::find()
        .filter(meeting_participant::Column::MeetingId.eq(meeting_id))
        .filter(meeting_participant::Column::UserId.eq(user_id))
        .one(db)
        .await?;
    match earlier {
        Some(row) if row.left_at.is_none() => Ok(row),
        Some(row) => {
            let mut active: meeting_participant::ActiveModel = row.into();
            active.left_at = Set(None);
            active.left_by_user_id = Set(None);
            active.joined_at = Set(Utc::now());
            Ok(active.update(db).await?)
        }
        None => Ok(meeting_participant::ActiveModel {
            meeting_id: Set(meeting_id),
            user_id: Set(Some(user_id)),
            last_turn_seq: Set(0),
            joined_at: Set(Utc::now()),
            ..Default::default()
        }
        .insert(db)
        .await?),
    }
}

pub(super) async fn find_user_participant(
    db: &DatabaseConnection,
    meeting_id: i32,
    user_id: i32,
) -> Result<Option<meeting_participant::Model>, AppError> {
    Ok(meeting_participant::Entity::find()
        .filter(meeting_participant::Column::MeetingId.eq(meeting_id))
        .filter(meeting_participant::Column::UserId.eq(user_id))
        .filter(meeting_participant::Column::LeftAt.is_null())
        .one(db)
        .await?)
}

pub(super) async fn load_meeting(
    db: &DatabaseConnection,
    id: i32,
) -> Result<meeting::Model, AppError> {
    meeting::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("meeting not found"))
}

pub(super) fn require_moderator(
    caller: &AuthUser,
    meeting: &meeting::Model,
) -> Result<(), AppError> {
    if meeting.moderator_user_id == caller.user.id {
        Ok(())
    } else {
        Err(AppError::Forbidden("only the moderator can do that"))
    }
}

/// The moderator, or an assistant moderator: who may make and adjust permanent
/// agent assignments.
pub(super) async fn require_assigner(
    db: &DatabaseConnection,
    caller: &AuthUser,
    meeting: &meeting::Model,
) -> Result<(), AppError> {
    let is_assistant = find_user_participant(db, meeting.id, caller.user.id)
        .await?
        .is_some_and(|p| p.is_assistant);
    if may_assign_permanent(meeting.moderator_user_id == caller.user.id, is_assistant) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "only the moderator or an assistant moderator can do that",
        ))
    }
}

/// The caller's participant row; the moderator always has one.
async fn require_member(
    db: &DatabaseConnection,
    caller: &AuthUser,
    meeting_id: i32,
) -> Result<meeting_participant::Model, AppError> {
    find_user_participant(db, meeting_id, caller.user.id)
        .await?
        .ok_or(AppError::Forbidden("join the meeting first"))
}

/// GET /api/townhall/meetings
pub async fn list_meetings(
    State(state): State<AppState>,
    caller: AuthUser,
) -> Result<Json<Vec<MeetingItem>>, AppError> {
    gate(&caller)?;
    let meetings = meeting::Entity::find()
        .order_by_desc(meeting::Column::Id)
        .all(&state.db)
        .await?;
    Ok(Json(meetings.iter().map(meeting_item).collect()))
}

#[derive(Serialize)]
pub struct ParticipantItem {
    pub id: i32,
    pub kind: &'static str,
    pub name: String,
    pub user_id: Option<i32>,
    pub agent_id: Option<i32>,
    pub owner_user_id: Option<i32>,
    /// The owner's choice, as stored.
    pub turn_mode: Option<String>,
    /// What actually applies after the moderator's ceiling.
    pub effective_turn_mode: Option<String>,
    pub presence: Option<Presence>,
    /// An assistant moderator.
    pub assistant: bool,
    /// For an agent: whether other people see it in the room list.
    pub visible_to_others: bool,
    /// A permanent resident agent, and what it is working toward.
    pub permanent: bool,
    pub objective: Option<String>,
    pub wake_every_minutes: Option<i32>,
    /// For a permanent agent: the permanent agents it may address.
    pub may_address: Vec<ParticipantRef>,
}

#[derive(Serialize)]
pub struct ParticipantRef {
    pub id: i32,
    pub name: String,
}

#[derive(Serialize)]
pub struct MeetingDetail {
    #[serde(flatten)]
    pub meeting: MeetingItem,
    pub participants: Vec<ParticipantItem>,
}

/// GET /api/townhall/meetings/{id}: settings, participants and live presence.
pub async fn get_meeting(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<i32>,
) -> Result<Json<MeetingDetail>, AppError> {
    gate(&caller)?;
    let meeting = load_meeting(&state.db, id).await?;
    require_member(&state.db, &caller, id).await?;
    let rows = meeting_participant::Entity::find()
        .filter(meeting_participant::Column::MeetingId.eq(id))
        .filter(meeting_participant::Column::LeftAt.is_null())
        .order_by_asc(meeting_participant::Column::Id)
        .all(&state.db)
        .await?;
    let mut participants = Vec::with_capacity(rows.len());
    for row in rows {
        let (name, is_agent) = speaker_of(&state.db, row.id).await?;
        let mut item = ParticipantItem {
            id: row.id,
            kind: if is_agent { "agent" } else { "person" },
            name,
            user_id: row.user_id,
            agent_id: row.agent_id,
            owner_user_id: None,
            turn_mode: row.turn_mode.clone(),
            effective_turn_mode: None,
            presence: None,
            assistant: row.is_assistant,
            visible_to_others: row.visible_to_others,
            permanent: row.permanent,
            objective: row.objective.clone(),
            wake_every_minutes: row.wake_every_minutes,
            may_address: Vec::new(),
        };
        if row.permanent {
            let rules = meeting_converse_rule::Entity::find()
                .filter(meeting_converse_rule::Column::FromParticipantId.eq(row.id))
                .all(&state.db)
                .await?;
            for rule in rules {
                let (name, _) = speaker_of(&state.db, rule.to_participant_id).await?;
                item.may_address.push(ParticipantRef {
                    id: rule.to_participant_id,
                    name,
                });
            }
        }
        if let Some(agent_id) = row.agent_id {
            if let Some(agent) = townhall_agent::Entity::find_by_id(agent_id)
                .one(&state.db)
                .await?
            {
                item.owner_user_id = Some(agent.owner_user_id);
                item.effective_turn_mode = Some(
                    effective_turn_mode(&row, &agent, &meeting)
                        .as_str()
                        .to_string(),
                );
                item.presence = RUNTIME.presence_of(row.id);
            }
        }
        if item.kind == "agent"
            && !agent_listed_for(
                item.owner_user_id == Some(caller.user.id),
                meeting.moderator_user_id == caller.user.id,
                row.permanent,
                row.visible_to_others,
            )
        {
            continue;
        }
        participants.push(item);
    }
    Ok(Json(MeetingDetail {
        meeting: meeting_item(&meeting),
        participants,
    }))
}

#[derive(Deserialize)]
pub struct UpdateMeeting {
    pub allow_member_agents: Option<bool>,
    pub agent_mode_ceiling: Option<String>,
    pub max_consecutive_agent_turns: Option<i32>,
    pub max_consecutive_permanent_turns: Option<i32>,
    pub autonomous_hourly_cents: Option<i32>,
    pub close: Option<bool>,
}

/// PATCH /api/townhall/meetings/{id}: moderator settings. Effective turn modes
/// follow the ceiling immediately; the owners' stored choices are untouched.
pub async fn update_meeting(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<i32>,
    Json(req): Json<UpdateMeeting>,
) -> Result<Json<MeetingItem>, AppError> {
    gate(&caller)?;
    let meeting = load_meeting(&state.db, id).await?;
    require_moderator(&caller, &meeting)?;
    let mut active: meeting::ActiveModel = meeting.into();
    if let Some(v) = req.allow_member_agents {
        active.allow_member_agents = Set(v);
    }
    if let Some(s) = &req.agent_mode_ceiling {
        active.agent_mode_ceiling = Set(parse_mode(s)?.as_str().to_string());
    }
    if let Some(n) = req.max_consecutive_agent_turns {
        active.max_consecutive_agent_turns = Set(check_cap(n)?);
    }
    if let Some(n) = req.max_consecutive_permanent_turns {
        active.max_consecutive_permanent_turns = Set(check_cap(n)?);
    }
    if let Some(cents) = req.autonomous_hourly_cents {
        if !(0..=10_000).contains(&cents) {
            return Err(AppError::BadRequest(
                "the hourly ceiling must be between $0 and $100",
            ));
        }
        active.autonomous_hourly_cents = Set(cents);
    }
    if req.close == Some(true) {
        active.status = Set("closed".to_string());
        active.closed_at = Set(Some(Utc::now()));
    }
    let updated = active.update(&state.db).await?;
    Ok(Json(meeting_item(&updated)))
}

#[derive(Deserialize)]
pub struct EnrollAgent {
    pub agent_id: i32,
    pub turn_mode: Option<String>,
}

/// POST /api/townhall/meetings/{id}/agents: an owner enrolls their own agent.
pub async fn enroll_agent(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<i32>,
    Json(req): Json<EnrollAgent>,
) -> Result<Json<ParticipantSummary>, AppError> {
    gate(&caller)?;
    let meeting = load_meeting(&state.db, id).await?;
    if meeting.status != "open" {
        return Err(AppError::Conflict("meeting is closed"));
    }
    require_member(&state.db, &caller, id).await?;
    let agent = townhall_agent::Entity::find_by_id(req.agent_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound("agent not found"))?;
    // Only an agent's owner can enroll it.
    if agent.owner_user_id != caller.user.id {
        return Err(AppError::Forbidden("only an agent's owner can enroll it"));
    }
    // The moderator's own agents may always join; anyone else's need the setting.
    if agent.owner_user_id != meeting.moderator_user_id && !meeting.allow_member_agents {
        return Err(AppError::Forbidden(
            "the moderator has not allowed other people's agents",
        ));
    }
    check_admission(&agent.description, &agent.tools).map_err(admission_error)?;
    let mode = match &req.turn_mode {
        Some(s) => parse_mode(s)?,
        None => TurnMode::ModeratorCalled,
    };
    // Any earlier row for this agent, in the room or dismissed.
    let earlier = meeting_participant::Entity::find()
        .filter(meeting_participant::Column::MeetingId.eq(id))
        .filter(meeting_participant::Column::AgentId.eq(agent.id))
        .one(&state.db)
        .await?;
    if let Some(row) = &earlier {
        if row.left_at.is_none() {
            return Err(AppError::Conflict("that agent is already in the meeting"));
        }
        if !may_reenroll(
            row.left_by_user_id,
            meeting.moderator_user_id,
            caller.user.id,
        ) {
            return Err(AppError::Forbidden("the moderator dismissed that agent"));
        }
    }
    let row = put_agent_in_room(&state.db, id, agent.id, mode, earlier).await?;
    Ok(Json(ParticipantSummary {
        id: row.id,
        turn_mode: mode.as_str().to_string(),
    }))
}

/// Put an agent in the room: a new row, or, for an agent that was dismissed, its
/// reopened one (the pair (meeting, agent) is unique). Who may do this, and
/// whether the agent is admissible, is decided by the caller. A newly placed
/// agent starts from the end of the conversation, not the beginning.
pub(super) async fn put_agent_in_room(
    db: &DatabaseConnection,
    meeting_id: i32,
    agent_id: i32,
    mode: TurnMode,
    earlier: Option<meeting_participant::Model>,
) -> Result<meeting_participant::Model, AppError> {
    let last = meeting_message::Entity::find()
        .filter(meeting_message::Column::MeetingId.eq(meeting_id))
        .order_by_desc(meeting_message::Column::Seq)
        .one(db)
        .await?
        .map_or(0, |m| m.seq);
    Ok(match earlier {
        Some(row) => {
            let mut active: meeting_participant::ActiveModel = row.into();
            active.left_at = Set(None);
            active.left_by_user_id = Set(None);
            active.turn_mode = Set(Some(mode.as_str().to_string()));
            active.last_turn_seq = Set(last);
            active.joined_at = Set(Utc::now());
            active.update(db).await?
        }
        None => {
            meeting_participant::ActiveModel {
                meeting_id: Set(meeting_id),
                agent_id: Set(Some(agent_id)),
                turn_mode: Set(Some(mode.as_str().to_string())),
                last_turn_seq: Set(last),
                joined_at: Set(Utc::now()),
                ..Default::default()
            }
            .insert(db)
            .await?
        }
    })
}

#[derive(Serialize)]
pub struct ParticipantSummary {
    pub id: i32,
    pub turn_mode: String,
}

#[derive(Deserialize)]
pub struct SetTurnMode {
    pub turn_mode: String,
}

/// PATCH /api/townhall/meetings/{id}/participants/{pid}: the agent's owner
/// changes its turn mode, in realtime.
pub async fn set_turn_mode(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((id, pid)): Path<(i32, i32)>,
    Json(req): Json<SetTurnMode>,
) -> Result<Json<ParticipantSummary>, AppError> {
    gate(&caller)?;
    let mode = parse_mode(&req.turn_mode)?;
    let row = meeting_participant::Entity::find_by_id(pid)
        .one(&state.db)
        .await?
        .filter(|p| p.meeting_id == id)
        .ok_or(AppError::NotFound("participant not found"))?;
    let agent_id = row
        .agent_id
        .ok_or(AppError::BadRequest("only agents have a turn mode"))?;
    let agent = townhall_agent::Entity::find_by_id(agent_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound("agent not found"))?;
    if agent.owner_user_id != caller.user.id {
        return Err(AppError::Forbidden(
            "only an agent's owner can change its turn mode",
        ));
    }
    let mut active: meeting_participant::ActiveModel = row.into();
    active.turn_mode = Set(Some(mode.as_str().to_string()));
    let saved = active.update(&state.db).await?;
    Ok(Json(ParticipantSummary {
        id: saved.id,
        turn_mode: mode.as_str().to_string(),
    }))
}

/// DELETE /api/townhall/meetings/{id}/participants/{pid}: the moderator
/// dismisses anyone; an agent's owner dismisses their own agent. The moderator
/// cannot be dismissed (close the meeting instead).
pub async fn dismiss_participant(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((id, pid)): Path<(i32, i32)>,
) -> Result<StatusCode, AppError> {
    gate(&caller)?;
    let meeting = load_meeting(&state.db, id).await?;
    let row = meeting_participant::Entity::find_by_id(pid)
        .one(&state.db)
        .await?
        .filter(|p| p.meeting_id == id && p.left_at.is_none())
        .ok_or(AppError::NotFound("participant not found"))?;
    if row.user_id == Some(meeting.moderator_user_id) {
        return Err(AppError::BadRequest(
            "the moderator cannot be dismissed; close the meeting instead",
        ));
    }
    let agent_owner = match row.agent_id {
        Some(agent_id) => townhall_agent::Entity::find_by_id(agent_id)
            .one(&state.db)
            .await?
            .map(|a| a.owner_user_id),
        None => None,
    };
    let is_moderator = meeting.moderator_user_id == caller.user.id;
    if !may_dismiss(is_moderator, agent_owner == Some(caller.user.id)) {
        return Err(AppError::Forbidden(
            "only the moderator, or an agent's owner, can dismiss",
        ));
    }
    let agent_id = row.agent_id;
    let turn_mode = row.turn_mode.clone();
    let mut active: meeting_participant::ActiveModel = row.into();
    active.left_at = Set(Some(Utc::now()));
    active.left_by_user_id = Set(Some(caller.user.id));
    // Whatever made it a permanent resident, or a person an assistant, ends with
    // their place in the room.
    active.is_assistant = Set(false);
    active.permanent = Set(false);
    active.objective = Set(None);
    active.wake_every_minutes = Set(None);
    active.update(&state.db).await?;
    meeting_converse_rule::Entity::delete_many()
        .filter(
            meeting_converse_rule::Column::FromParticipantId
                .eq(pid)
                .or(meeting_converse_rule::Column::ToParticipantId.eq(pid)),
        )
        .exec(&state.db)
        .await?;
    RUNTIME.forget(pid);

    // An agent the moderator dismisses goes to the waiting room, where only the
    // moderator can let it back. An owner who dismisses their own agent just
    // takes it out.
    if let (true, Some(agent_id), Some(owner)) = (is_moderator, agent_id, agent_owner) {
        if owner != meeting.moderator_user_id {
            super::townhall_requests::send_to_waiting_room(
                &state.db, id, owner, agent_id, turn_mode,
            )
            .await?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SetAssistant {
    pub assistant: bool,
}

/// PUT /api/townhall/meetings/{id}/participants/{pid}/assistant: the moderator
/// names a person in the room an assistant moderator, or takes that back.
pub async fn set_assistant(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((id, pid)): Path<(i32, i32)>,
    Json(req): Json<SetAssistant>,
) -> Result<StatusCode, AppError> {
    gate(&caller)?;
    let meeting = load_meeting(&state.db, id).await?;
    require_moderator(&caller, &meeting)?;
    let row = meeting_participant::Entity::find_by_id(pid)
        .one(&state.db)
        .await?
        .filter(|p| p.meeting_id == id && p.left_at.is_none() && p.user_id.is_some())
        .ok_or(AppError::NotFound("person not found in this meeting"))?;
    if row.user_id == Some(meeting.moderator_user_id) {
        return Err(AppError::BadRequest("the moderator is already in charge"));
    }
    let mut active: meeting_participant::ActiveModel = row.into();
    active.is_assistant = Set(req.assistant);
    active.update(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SetVisibility {
    pub visible: bool,
}

/// PUT /api/townhall/meetings/{id}/participants/{pid}/visibility: an agent's
/// owner chooses whether other people in the room see it in the room list. What
/// the agent says stays in the conversation either way.
pub async fn set_visibility(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((id, pid)): Path<(i32, i32)>,
    Json(req): Json<SetVisibility>,
) -> Result<StatusCode, AppError> {
    gate(&caller)?;
    let row = meeting_participant::Entity::find_by_id(pid)
        .one(&state.db)
        .await?
        .filter(|p| p.meeting_id == id && p.left_at.is_none())
        .ok_or(AppError::NotFound("agent not found in this meeting"))?;
    let agent_id = row
        .agent_id
        .ok_or(AppError::BadRequest("only agents have this setting"))?;
    let agent = townhall_agent::Entity::find_by_id(agent_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound("agent not found"))?;
    if agent.owner_user_id != caller.user.id {
        return Err(AppError::Forbidden(
            "only an agent's owner can change who sees it",
        ));
    }
    let mut active: meeting_participant::ActiveModel = row.into();
    active.visible_to_others = Set(req.visible);
    active.update(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- messages ------------------------------------------------------------

#[derive(Serialize)]
pub struct MessageItem {
    pub seq: i64,
    pub participant_id: i32,
    pub speaker: String,
    pub is_agent: bool,
    pub dialog_type: String,
    pub body: String,
    pub revealed: bool,
    pub created_at: chrono::DateTime<Utc>,
}

/// An unrevealed thought is shown as dots, and its text never leaves the server.
fn masked_body(dialog_type: &str, revealed: bool, body: &str, seq: i64) -> String {
    let hidden = DialogType::parse(dialog_type).is_some_and(DialogType::hidden_until_revealed);
    if hidden && !revealed {
        ".".repeat((seq % 4 + 1) as usize)
    } else {
        body.to_string()
    }
}

#[derive(Deserialize)]
pub struct PostMessage {
    pub body: String,
    pub dialog_type: Option<String>,
}

/// POST /api/townhall/meetings/{id}/messages: a person speaks. Agents whose
/// turn mode allows it are then given the chance to answer.
pub async fn post_human_message(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<i32>,
    Json(req): Json<PostMessage>,
) -> Result<Json<MessageItem>, AppError> {
    gate(&caller)?;
    let body = req.body.trim().to_string();
    if body.is_empty() {
        return Err(AppError::BadRequest("a message cannot be empty"));
    }
    let dialog_type = match req.dialog_type.as_deref() {
        Some(s) => DialogType::parse(s).ok_or(AppError::BadRequest("unknown dialogue type"))?,
        None => DialogType::Contribution,
    };
    let participant = require_member(&state.db, &caller, id).await?;
    let posted = post_message(
        &state.db,
        id,
        participant.id,
        false,
        false,
        dialog_type,
        &body,
    )
    .await?
    .ok_or(AppError::Internal("a person's message was held back"))?;
    dispatch_for_message(&state, id, &posted).await?;
    Ok(Json(MessageItem {
        seq: posted.seq,
        participant_id: posted.participant_id,
        speaker: caller.user.username.clone(),
        is_agent: false,
        dialog_type: posted.dialog_type.clone(),
        body: posted.body.clone(),
        revealed: posted.revealed,
        created_at: posted.created_at,
    }))
}

#[derive(Deserialize)]
pub struct CallOn {
    pub participant_id: i32,
}

/// POST /api/townhall/meetings/{id}/call: the moderator calls on an agent,
/// whatever its turn mode.
pub async fn call_on_agent(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<i32>,
    Json(req): Json<CallOn>,
) -> Result<Json<ParticipantSummary>, AppError> {
    gate(&caller)?;
    let meeting = load_meeting(&state.db, id).await?;
    require_moderator(&caller, &meeting)?;
    let row = meeting_participant::Entity::find_by_id(req.participant_id)
        .one(&state.db)
        .await?
        .filter(|p| p.meeting_id == id && p.agent_id.is_some() && p.left_at.is_none())
        .ok_or(AppError::NotFound("agent participant not found"))?;
    start_turn(&state, id, row.id, TurnReason::CalledOn);
    Ok(Json(ParticipantSummary {
        id: row.id,
        turn_mode: row.turn_mode.unwrap_or_default(),
    }))
}

#[derive(Deserialize)]
pub struct MessagesQuery {
    pub since: Option<i64>,
}

/// GET /api/townhall/meetings/{id}/messages?since=N
pub async fn list_messages(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<i32>,
    Query(q): Query<MessagesQuery>,
) -> Result<Json<Vec<MessageItem>>, AppError> {
    gate(&caller)?;
    require_member(&state.db, &caller, id).await?;
    let rows = meeting_message::Entity::find()
        .filter(meeting_message::Column::MeetingId.eq(id))
        .filter(meeting_message::Column::Seq.gt(q.since.unwrap_or(0)))
        .order_by_asc(meeting_message::Column::Seq)
        .all(&state.db)
        .await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let (speaker, is_agent) = speaker_of(&state.db, row.participant_id).await?;
        items.push(MessageItem {
            seq: row.seq,
            participant_id: row.participant_id,
            speaker,
            is_agent,
            body: masked_body(&row.dialog_type, row.revealed, &row.body, row.seq),
            dialog_type: row.dialog_type,
            revealed: row.revealed,
            created_at: row.created_at,
        });
    }
    Ok(Json(items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unrevealed_thought_never_shows_its_text() {
        let shown = masked_body("thought", false, "secret reasoning", 5);
        assert!(shown.chars().all(|c| c == '.'));
        assert!(!shown.contains("secret"));
        assert_eq!(
            masked_body("thought", true, "secret reasoning", 5),
            "secret reasoning"
        );
        assert_eq!(masked_body("contribution", false, "hello", 5), "hello");
    }

    #[test]
    fn the_dots_cycle_through_one_to_four() {
        let lengths: Vec<usize> = (0..8)
            .map(|seq| masked_body("thought", false, "x", seq).len())
            .collect();
        assert_eq!(lengths, [1, 2, 3, 4, 1, 2, 3, 4]);
    }

    #[test]
    fn the_loop_guard_cap_is_bounded() {
        assert!(check_cap(0).is_err());
        assert_eq!(check_cap(3).unwrap(), 3);
        assert!(check_cap(51).is_err());
    }
}
