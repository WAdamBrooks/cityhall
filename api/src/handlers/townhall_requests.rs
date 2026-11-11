//! The waiting room and invitations (`/api/townhall/meetings/{id}/requests` and
//! `.../invites`). Joining a meeting, and bringing an agent to one without being
//! allowed to do so unasked, goes through a request that the moderator approves,
//! denies, or leaves pending. People and agents can also be invited, from the
//! directory. See `docs/townhall.md`, "Waiting room" and "Invitations".

use std::collections::HashSet;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};

use super::townhall::{
    admission_error, admit_user, find_user_participant, gate, load_meeting, parse_mode,
    put_agent_in_room, require_moderator,
};
use crate::auth::AuthUser;
use crate::entities::{meeting, meeting_participant, meeting_request, role, townhall_agent, user};
use crate::error::AppError;
use crate::rbac::Perms;
use crate::state::AppState;
use crate::townhall::{check_admission, line_display, TurnMode};
use crate::townhall_runtime::RUNTIME;

const INVITED: &str = "invited";
const PENDING: &str = "pending";
const APPROVED: &str = "approved";
const DENIED: &str = "denied";
const DECLINED: &str = "declined";
const WITHDRAWN: &str = "withdrawn";
const ASK: &str = "ask";
const INVITE: &str = "invite";

/// How many of a person's own requests they are shown, newest first.
const MY_REQUESTS_SHOWN: u64 = 30;

#[derive(Serialize)]
pub struct RequestItem {
    pub id: i32,
    /// `ask` or `invite`.
    pub kind: String,
    /// The person asking, or the person invited; for an agent line, its owner.
    pub user_id: i32,
    pub username: String,
    /// Set for an agent line; absent for a person.
    pub agent_id: Option<i32>,
    pub agent_name: Option<String>,
    pub turn_mode: Option<String>,
    pub status: String,
    /// What to show: `invited`, `waiting`, `in the room`, `denied`, `declined`.
    pub display: String,
    pub invited_by_user_id: Option<i32>,
    pub invited_by: Option<String>,
    /// Whether the viewer sent this invitation.
    pub invited_by_you: bool,
    pub dismissed_earlier: bool,
    /// Whether the requester is already in the meeting, which an agent line needs.
    pub requester_in_room: bool,
    /// Whether the invited person has the meeting open right now.
    pub present: bool,
    pub created_at: chrono::DateTime<Utc>,
}

/// What building an item needs to know about the meeting and the viewer.
struct View<'a> {
    meeting: &'a meeting::Model,
    in_room: HashSet<i32>,
    viewer_id: i32,
}

/// Whether a user's role lets them use meetings. Someone who cannot would be
/// admitted to a room they could not open.
async fn can_use_meetings(db: &DatabaseConnection, user: &user::Model) -> Result<bool, AppError> {
    let role = match user.role_id {
        Some(id) => role::Entity::find_by_id(id).one(db).await?,
        None => None,
    };
    Ok(Perms::from_role(role.as_ref()).can("meetings.use"))
}

async fn username_of(db: &DatabaseConnection, user_id: i32) -> Result<String, AppError> {
    Ok(user::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .map_or_else(|| "someone".to_string(), |u| u.username))
}

async fn item_of(
    db: &DatabaseConnection,
    row: meeting_request::Model,
    view: &View<'_>,
) -> Result<RequestItem, AppError> {
    let username = username_of(db, row.user_id).await?;
    let agent_name = match row.agent_id {
        Some(agent_id) => townhall_agent::Entity::find_by_id(agent_id)
            .one(db)
            .await?
            .map(|a| a.name),
        None => None,
    };
    let invited_by = match row.invited_by_user_id {
        Some(id) => Some(username_of(db, id).await?),
        None => None,
    };
    let by_moderator = row.invited_by_user_id == Some(view.meeting.moderator_user_id);
    let present = RUNTIME.person_present(view.meeting.id, row.user_id);
    let display = line_display(
        &row.status,
        &row.kind,
        by_moderator,
        row.agent_id.is_some(),
        present,
    );
    Ok(RequestItem {
        id: row.id,
        kind: row.kind,
        user_id: row.user_id,
        username,
        agent_id: row.agent_id,
        agent_name,
        turn_mode: row.turn_mode,
        status: row.status,
        display: display.to_string(),
        invited_by_user_id: row.invited_by_user_id,
        invited_by,
        invited_by_you: row.invited_by_user_id == Some(view.viewer_id),
        dismissed_earlier: row.dismissed_earlier,
        requester_in_room: view.in_room.contains(&row.user_id),
        present,
        created_at: row.created_at,
    })
}

/// The users currently in the meeting's room.
async fn users_in_room(db: &DatabaseConnection, meeting_id: i32) -> Result<HashSet<i32>, AppError> {
    Ok(meeting_participant::Entity::find()
        .filter(meeting_participant::Column::MeetingId.eq(meeting_id))
        .filter(meeting_participant::Column::LeftAt.is_null())
        .filter(meeting_participant::Column::UserId.is_not_null())
        .all(db)
        .await?
        .into_iter()
        .filter_map(|p| p.user_id)
        .collect())
}

async fn items_of(
    db: &DatabaseConnection,
    rows: Vec<meeting_request::Model>,
    meeting: &meeting::Model,
    viewer_id: i32,
) -> Result<Vec<RequestItem>, AppError> {
    let view = View {
        meeting,
        in_room: users_in_room(db, meeting.id).await?,
        viewer_id,
    };
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(item_of(db, row, &view).await?);
    }
    Ok(items)
}

#[derive(Deserialize)]
pub struct AgentAsk {
    pub agent_id: i32,
    pub turn_mode: Option<String>,
}

#[derive(Deserialize)]
pub struct AskToJoin {
    #[serde(default)]
    pub agents: Vec<AgentAsk>,
}

/// A line for this exact subject that is still open (invited or pending).
async fn open_line(
    db: &DatabaseConnection,
    meeting_id: i32,
    user_id: i32,
    agent_id: Option<i32>,
) -> Result<Option<meeting_request::Model>, AppError> {
    let mut query = meeting_request::Entity::find()
        .filter(meeting_request::Column::MeetingId.eq(meeting_id))
        .filter(meeting_request::Column::UserId.eq(user_id))
        .filter(
            Condition::any()
                .add(meeting_request::Column::Status.eq(INVITED))
                .add(meeting_request::Column::Status.eq(PENDING)),
        );
    query = match agent_id {
        Some(id) => query.filter(meeting_request::Column::AgentId.eq(id)),
        None => query.filter(meeting_request::Column::AgentId.is_null()),
    };
    Ok(query.one(db).await?)
}

/// How a new line starts out.
struct Start {
    kind: &'static str,
    status: &'static str,
    invited_by: Option<i32>,
    dismissed_earlier: bool,
}

impl Start {
    fn ask(dismissed_earlier: bool) -> Self {
        Self {
            kind: ASK,
            status: PENDING,
            invited_by: None,
            dismissed_earlier,
        }
    }

    fn invite(by: i32) -> Self {
        Self {
            kind: INVITE,
            status: INVITED,
            invited_by: Some(by),
            dismissed_earlier: false,
        }
    }
}

async fn insert_line(
    db: &DatabaseConnection,
    meeting_id: i32,
    user_id: i32,
    agent_id: Option<i32>,
    turn_mode: Option<String>,
    start: Start,
) -> Result<meeting_request::Model, AppError> {
    let Start {
        kind,
        status,
        invited_by,
        dismissed_earlier,
    } = start;
    Ok(meeting_request::ActiveModel {
        meeting_id: Set(meeting_id),
        user_id: Set(user_id),
        agent_id: Set(agent_id),
        turn_mode: Set(turn_mode),
        status: Set(status.to_string()),
        kind: Set(kind.to_string()),
        invited_by_user_id: Set(invited_by),
        dismissed_earlier: Set(dismissed_earlier),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await?)
}

/// Put an agent the moderator dismissed in the waiting room, marked as dismissed
/// earlier. Only the moderator can let it back.
pub(super) async fn send_to_waiting_room(
    db: &DatabaseConnection,
    meeting_id: i32,
    owner_user_id: i32,
    agent_id: i32,
    turn_mode: Option<String>,
) -> Result<(), AppError> {
    if open_line(db, meeting_id, owner_user_id, Some(agent_id))
        .await?
        .is_none()
    {
        insert_line(
            db,
            meeting_id,
            owner_user_id,
            Some(agent_id),
            turn_mode,
            Start::ask(true),
        )
        .await?;
    }
    Ok(())
}

/// POST /api/townhall/meetings/{id}/requests: ask to come in, and/or to bring
/// some of your own agents. One open line per person and per agent.
pub async fn ask_to_join(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<i32>,
    Json(req): Json<AskToJoin>,
) -> Result<Json<Vec<RequestItem>>, AppError> {
    gate(&caller)?;
    let db = &state.db;
    let meeting = load_meeting(db, id).await?;
    if meeting.status != "open" {
        return Err(AppError::Conflict("meeting is closed"));
    }
    if meeting.moderator_user_id == caller.user.id {
        return Err(AppError::BadRequest("the moderator does not need to ask"));
    }
    let member = find_user_participant(db, id, caller.user.id)
        .await?
        .is_some();
    let earlier_row = meeting_participant::Entity::find()
        .filter(meeting_participant::Column::MeetingId.eq(id))
        .filter(meeting_participant::Column::UserId.eq(caller.user.id))
        .one(db)
        .await?;
    let dismissed_earlier = earlier_row.is_some_and(|r| r.left_at.is_some());

    // Check everything before writing anything.
    let mut asks: Vec<(townhall_agent::Model, TurnMode)> = Vec::new();
    for ask in &req.agents {
        let agent = townhall_agent::Entity::find_by_id(ask.agent_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("agent not found"))?;
        if agent.owner_user_id != caller.user.id {
            return Err(AppError::Forbidden("you can only bring your own agents"));
        }
        check_admission(&agent.description, &agent.tools).map_err(admission_error)?;
        if agent_in_room(db, id, agent.id).await? {
            return Err(AppError::Conflict("that agent is already in the meeting"));
        }
        let mode = match &ask.turn_mode {
            Some(s) => parse_mode(s)?,
            None => TurnMode::MentionOnly,
        };
        if !asks.iter().any(|(a, _)| a.id == agent.id) {
            asks.push((agent, mode));
        }
    }
    if member && asks.is_empty() {
        return Err(AppError::BadRequest("there is nothing to ask for"));
    }

    let mut created = Vec::new();
    if !member && open_line(db, id, caller.user.id, None).await?.is_none() {
        created.push(
            insert_line(
                db,
                id,
                caller.user.id,
                None,
                None,
                Start::ask(dismissed_earlier),
            )
            .await?,
        );
    }
    for (agent, mode) in asks {
        if open_line(db, id, caller.user.id, Some(agent.id))
            .await?
            .is_some()
        {
            continue;
        }
        created.push(
            insert_line(
                db,
                id,
                caller.user.id,
                Some(agent.id),
                Some(mode.as_str().to_string()),
                Start::ask(dismissed_earlier),
            )
            .await?,
        );
    }
    Ok(Json(items_of(db, created, &meeting, caller.user.id).await?))
}

async fn agent_in_room(
    db: &DatabaseConnection,
    meeting_id: i32,
    agent_id: i32,
) -> Result<bool, AppError> {
    Ok(meeting_participant::Entity::find()
        .filter(meeting_participant::Column::MeetingId.eq(meeting_id))
        .filter(meeting_participant::Column::AgentId.eq(agent_id))
        .filter(meeting_participant::Column::LeftAt.is_null())
        .one(db)
        .await?
        .is_some())
}

#[derive(Deserialize)]
pub struct Invite {
    #[serde(default)]
    pub people: Vec<i32>,
    #[serde(default)]
    pub agents: Vec<i32>,
}

/// POST /api/townhall/meetings/{id}/invites: invite people and agents from the
/// directory. A moderator's invitation lets the person straight in when they
/// arrive; anyone else's puts them in the waiting room once they do. An agent is
/// an offer to its owner, who has to accept.
pub async fn invite(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<i32>,
    Json(req): Json<Invite>,
) -> Result<Json<Vec<RequestItem>>, AppError> {
    gate(&caller)?;
    let db = &state.db;
    let meeting = load_meeting(db, id).await?;
    if meeting.status != "open" {
        return Err(AppError::Conflict("meeting is closed"));
    }
    if find_user_participant(db, id, caller.user.id)
        .await?
        .is_none()
    {
        return Err(AppError::Forbidden("only people in the meeting can invite"));
    }
    if req.people.is_empty() && req.agents.is_empty() {
        return Err(AppError::BadRequest("choose someone to invite"));
    }

    let in_room = users_in_room(db, id).await?;
    for &person in &req.people {
        if person == caller.user.id {
            return Err(AppError::BadRequest("you cannot invite yourself"));
        }
        let invitee = user::Entity::find_by_id(person)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("that person is not in the directory"))?;
        if !can_use_meetings(db, &invitee).await? {
            return Err(AppError::BadRequest(
                "that person is not allowed to use meetings",
            ));
        }
        if in_room.contains(&person) {
            return Err(AppError::Conflict("that person is already in the meeting"));
        }
    }
    let mut agents = Vec::new();
    for &agent_id in &req.agents {
        let agent = townhall_agent::Entity::find_by_id(agent_id)
            .one(db)
            .await?
            .filter(|a| a.retired_at.is_none())
            .ok_or(AppError::NotFound("that agent is not in the directory"))?;
        if agent.owner_user_id == caller.user.id {
            return Err(AppError::BadRequest(
                "that is your own agent: enroll it, or ask to bring it",
            ));
        }
        check_admission(&agent.description, &agent.tools).map_err(admission_error)?;
        if agent_in_room(db, id, agent.id).await? {
            return Err(AppError::Conflict("that agent is already in the meeting"));
        }
        agents.push(agent);
    }

    let mut created = Vec::new();
    for person in req.people {
        if open_line(db, id, person, None).await?.is_none() {
            created.push(
                insert_line(db, id, person, None, None, Start::invite(caller.user.id)).await?,
            );
        }
    }
    for agent in agents {
        if open_line(db, id, agent.owner_user_id, Some(agent.id))
            .await?
            .is_none()
        {
            created.push(
                insert_line(
                    db,
                    id,
                    agent.owner_user_id,
                    Some(agent.id),
                    Some(TurnMode::MentionOnly.as_str().to_string()),
                    Start::invite(caller.user.id),
                )
                .await?,
            );
        }
    }
    Ok(Json(items_of(db, created, &meeting, caller.user.id).await?))
}

/// GET /api/townhall/meetings/{id}/requests: the moderator's waiting room, the
/// lines still open (invited or waiting), oldest first.
pub async fn list_requests(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<i32>,
) -> Result<Json<Vec<RequestItem>>, AppError> {
    gate(&caller)?;
    let meeting = load_meeting(&state.db, id).await?;
    require_moderator(&caller, &meeting)?;
    let rows = meeting_request::Entity::find()
        .filter(meeting_request::Column::MeetingId.eq(id))
        .filter(
            Condition::any()
                .add(meeting_request::Column::Status.eq(INVITED))
                .add(meeting_request::Column::Status.eq(PENDING)),
        )
        .order_by_asc(meeting_request::Column::Id)
        .all(&state.db)
        .await?;
    Ok(Json(
        items_of(&state.db, rows, &meeting, caller.user.id).await?,
    ))
}

#[derive(Serialize)]
pub struct MyRequests {
    pub title: String,
    pub meeting_status: String,
    /// Whether the caller is in the room (the moderator always is).
    pub member: bool,
    /// What the caller asked for, and what they were invited to.
    pub items: Vec<RequestItem>,
    /// Invitations the caller sent, and how each stands.
    pub sent: Vec<RequestItem>,
}

/// GET /api/townhall/meetings/{id}/my-requests: what the caller has asked for,
/// was invited to, and invited, with how each stands. Open to someone who is not
/// in the meeting, who otherwise sees nothing of it but its title. Opening it is
/// what makes an invited person present: a moderator's invitation admits them,
/// anyone else's moves them to waiting.
pub async fn my_requests(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<i32>,
) -> Result<Json<MyRequests>, AppError> {
    gate(&caller)?;
    let db = &state.db;
    let meeting = load_meeting(db, id).await?;
    RUNTIME.touch_person(id, caller.user.id);

    // The caller arrives: take up invitations addressed to them as a person.
    if meeting.status == "open" {
        let arrived = meeting_request::Entity::find()
            .filter(meeting_request::Column::MeetingId.eq(id))
            .filter(meeting_request::Column::UserId.eq(caller.user.id))
            .filter(meeting_request::Column::AgentId.is_null())
            .filter(meeting_request::Column::Kind.eq(INVITE))
            .filter(meeting_request::Column::Status.eq(INVITED))
            .all(db)
            .await?;
        for line in arrived {
            let by_moderator = line.invited_by_user_id == Some(meeting.moderator_user_id);
            let mut active: meeting_request::ActiveModel = line.into();
            if by_moderator {
                admit_user(db, id, caller.user.id).await?;
                active.status = Set(APPROVED.to_string());
                active.decided_at = Set(Some(Utc::now()));
                active.decided_by_user_id = Set(Some(meeting.moderator_user_id));
            } else {
                active.status = Set(PENDING.to_string());
            }
            active.update(db).await?;
        }
    }

    let member = find_user_participant(db, id, caller.user.id)
        .await?
        .is_some();
    let rows = meeting_request::Entity::find()
        .filter(meeting_request::Column::MeetingId.eq(id))
        .filter(meeting_request::Column::UserId.eq(caller.user.id))
        .filter(meeting_request::Column::Status.ne(WITHDRAWN))
        .order_by_desc(meeting_request::Column::Id)
        .limit(MY_REQUESTS_SHOWN)
        .all(db)
        .await?;
    let sent_rows = meeting_request::Entity::find()
        .filter(meeting_request::Column::MeetingId.eq(id))
        .filter(meeting_request::Column::InvitedByUserId.eq(caller.user.id))
        .filter(meeting_request::Column::Status.ne(WITHDRAWN))
        .order_by_desc(meeting_request::Column::Id)
        .limit(MY_REQUESTS_SHOWN)
        .all(db)
        .await?;
    let items = items_of(db, rows, &meeting, caller.user.id).await?;
    let sent = items_of(db, sent_rows, &meeting, caller.user.id).await?;
    Ok(Json(MyRequests {
        title: meeting.title,
        meeting_status: meeting.status,
        member,
        items,
        sent,
    }))
}

#[derive(Deserialize)]
pub struct Decision {
    pub approve: bool,
}

/// POST /api/townhall/meetings/{id}/requests/{rid}: the moderator approves or
/// denies one line. A person's line may be decided as soon as it exists; an agent
/// line only once it is waiting (its owner has asked or accepted).
pub async fn decide_request(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((id, rid)): Path<(i32, i32)>,
    Json(decision): Json<Decision>,
) -> Result<Json<RequestItem>, AppError> {
    gate(&caller)?;
    let db = &state.db;
    let meeting = load_meeting(db, id).await?;
    require_moderator(&caller, &meeting)?;
    if meeting.status != "open" {
        return Err(AppError::Conflict("meeting is closed"));
    }
    let row = open_row(db, id, rid).await?;
    if row.agent_id.is_some() && row.status != PENDING {
        return Err(AppError::Conflict(
            "its owner has not accepted the invitation yet",
        ));
    }

    if decision.approve {
        match row.agent_id {
            None => {
                admit_user(db, id, row.user_id).await?;
            }
            Some(agent_id) => {
                // A person's own ask needs the person in the room; an invited
                // agent's owner need not be.
                if row.kind == ASK && find_user_participant(db, id, row.user_id).await?.is_none() {
                    return Err(AppError::BadRequest("admit the person before their agents"));
                }
                enter_agent(db, id, agent_id, row.turn_mode.as_deref()).await?;
            }
        }
    }
    let saved = finish(
        db,
        row,
        if decision.approve { APPROVED } else { DENIED },
        caller.user.id,
    )
    .await?;
    let mut items = items_of(db, vec![saved], &meeting, caller.user.id).await?;
    Ok(Json(items.remove(0)))
}

/// A line of this meeting that is still open.
async fn open_row(
    db: &DatabaseConnection,
    meeting_id: i32,
    rid: i32,
) -> Result<meeting_request::Model, AppError> {
    let row = meeting_request::Entity::find_by_id(rid)
        .one(db)
        .await?
        .filter(|r| r.meeting_id == meeting_id)
        .ok_or(AppError::NotFound("request not found"))?;
    if row.status != INVITED && row.status != PENDING {
        return Err(AppError::Conflict("that request was already decided"));
    }
    Ok(row)
}

async fn finish(
    db: &DatabaseConnection,
    row: meeting_request::Model,
    status: &str,
    by: i32,
) -> Result<meeting_request::Model, AppError> {
    let mut active: meeting_request::ActiveModel = row.into();
    active.status = Set(status.to_string());
    active.decided_at = Set(Some(Utc::now()));
    active.decided_by_user_id = Set(Some(by));
    Ok(active.update(db).await?)
}

/// Bring an agent into the room, checking again that it is admissible.
async fn enter_agent(
    db: &DatabaseConnection,
    meeting_id: i32,
    agent_id: i32,
    turn_mode: Option<&str>,
) -> Result<(), AppError> {
    let agent = townhall_agent::Entity::find_by_id(agent_id)
        .one(db)
        .await?
        .filter(|a| a.retired_at.is_none())
        .ok_or(AppError::NotFound("agent not found"))?;
    // A specialty may have changed since the request was made.
    check_admission(&agent.description, &agent.tools).map_err(admission_error)?;
    let earlier = meeting_participant::Entity::find()
        .filter(meeting_participant::Column::MeetingId.eq(meeting_id))
        .filter(meeting_participant::Column::AgentId.eq(agent_id))
        .one(db)
        .await?;
    if earlier.as_ref().is_some_and(|p| p.left_at.is_none()) {
        return Err(AppError::Conflict("that agent is already in the meeting"));
    }
    let mode = turn_mode
        .and_then(TurnMode::parse)
        .unwrap_or(TurnMode::MentionOnly);
    put_agent_in_room(db, meeting_id, agent_id, mode, earlier).await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct Response {
    pub accept: bool,
}

/// POST /api/townhall/meetings/{id}/requests/{rid}/respond: the invited person,
/// or an invited agent's owner, answers an invitation. Accepting a moderator's
/// invitation for an agent puts it in the room; accepting anyone else's puts it
/// in the waiting room.
pub async fn respond_to_invite(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((id, rid)): Path<(i32, i32)>,
    Json(answer): Json<Response>,
) -> Result<Json<RequestItem>, AppError> {
    gate(&caller)?;
    let db = &state.db;
    let meeting = load_meeting(db, id).await?;
    if meeting.status != "open" {
        return Err(AppError::Conflict("meeting is closed"));
    }
    let row = open_row(db, id, rid).await?;
    if row.kind != INVITE || row.user_id != caller.user.id || row.status != INVITED {
        return Err(AppError::Forbidden("that is not an invitation to you"));
    }
    let by_moderator = row.invited_by_user_id == Some(meeting.moderator_user_id);

    let saved = if !answer.accept {
        finish(db, row, DECLINED, caller.user.id).await?
    } else {
        match row.agent_id {
            // A person accepts by arriving; this does the same on request.
            None if by_moderator => {
                admit_user(db, id, caller.user.id).await?;
                finish(db, row, APPROVED, meeting.moderator_user_id).await?
            }
            Some(agent_id) if by_moderator => {
                enter_agent(db, id, agent_id, row.turn_mode.as_deref()).await?;
                finish(db, row, APPROVED, meeting.moderator_user_id).await?
            }
            _ => {
                let mut active: meeting_request::ActiveModel = row.into();
                active.status = Set(PENDING.to_string());
                active.update(db).await?
            }
        }
    };
    let mut items = items_of(db, vec![saved], &meeting, caller.user.id).await?;
    Ok(Json(items.remove(0)))
}

/// DELETE /api/townhall/meetings/{id}/requests/{rid}: remove a line from the
/// waiting room without a verdict. The moderator, the person it is for (an
/// agent's owner), or whoever sent an invitation may do it.
pub async fn remove_request(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((id, rid)): Path<(i32, i32)>,
) -> Result<StatusCode, AppError> {
    gate(&caller)?;
    let db = &state.db;
    let meeting = load_meeting(db, id).await?;
    let row = open_row(db, id, rid).await?;
    let allowed = meeting.moderator_user_id == caller.user.id
        || row.user_id == caller.user.id
        || row.invited_by_user_id == Some(caller.user.id);
    if !allowed {
        return Err(AppError::Forbidden(
            "only the moderator, the person it is for, or whoever invited them can remove it",
        ));
    }
    finish(db, row, WITHDRAWN, caller.user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct DirectoryPerson {
    pub id: i32,
    pub username: String,
}

#[derive(Serialize)]
pub struct DirectoryAgent {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub owner_user_id: i32,
    pub owner: String,
}

#[derive(Serialize)]
pub struct Directory {
    pub people: Vec<DirectoryPerson>,
    pub agents: Vec<DirectoryAgent>,
}

/// GET /api/townhall/directory: the people and the registered agents that can be
/// invited, other than the caller's own.
pub async fn directory(
    State(state): State<AppState>,
    caller: AuthUser,
) -> Result<Json<Directory>, AppError> {
    gate(&caller)?;
    let db = &state.db;
    let users = user::Entity::find()
        .order_by_asc(user::Column::Username)
        .all(db)
        .await?;
    // Only people who can use meetings are listed.
    let mut people = Vec::new();
    for u in users.iter().filter(|u| u.id != caller.user.id) {
        if can_use_meetings(db, u).await? {
            people.push(DirectoryPerson {
                id: u.id,
                username: u.username.clone(),
            });
        }
    }
    let agents = townhall_agent::Entity::find()
        .filter(townhall_agent::Column::OwnerUserId.ne(caller.user.id))
        .filter(townhall_agent::Column::RetiredAt.is_null())
        .order_by_asc(townhall_agent::Column::Name)
        .all(db)
        .await?
        .into_iter()
        .map(|a| DirectoryAgent {
            owner: users
                .iter()
                .find(|u| u.id == a.owner_user_id)
                .map_or_else(|| "someone".to_string(), |u| u.username.clone()),
            id: a.id,
            name: a.name,
            description: a.description,
            owner_user_id: a.owner_user_id,
        })
        .collect();
    Ok(Json(Directory { people, agents }))
}
