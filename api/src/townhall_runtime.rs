//! Live state and turn handling for town hall meetings: who is busy, what each
//! agent's presence is, message ordering, and running one agent turn from
//! prompt to posted reply. HTTP shapes live in `handlers::townhall`. See
//! `docs/townhall.md`.
//!
//! State here is in memory and per process: presence is a live fact that is
//! meaningless after a restart, and the per-meeting lock only has to serialise
//! this process's writers. A second CityHall process on the same database would
//! need a database-level guard instead; the unique (meeting, seq) index would
//! still refuse a duplicate.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::Serialize;

use crate::entities::{
    meeting, meeting_converse_rule, meeting_message, meeting_participant, townhall_agent, user,
};
use crate::error::AppError;
use crate::state::AppState;
use crate::townhall::{
    agent_turn_allowed, effective_mode, mentioned_names, reachable, within_hourly_budget,
    DialogType, TurnMode,
};
use crate::townhall_bridge::{
    build_prompt, parse_reply, Line, PresenceState, Reply, TurnContext, TurnEnd, TurnProgress,
    WaitReason, WorkspaceClient,
};

/// How often a running turn is polled, and for how long before CityHall stops
/// waiting. A session parked on a rate limit or an approval can outlast this;
/// its late answer is not collected (see `docs/townhall.md`, R16).
const POLL_EVERY: Duration = Duration::from_millis(750);
const GIVE_UP_AFTER: Duration = Duration::from_secs(180);
/// How often permanent agents are checked for a wake-up that is due.
const WAKE_CHECK_EVERY: Duration = Duration::from_secs(30);
/// Consecutive failed polls before the agent is shown as unreachable.
const POLL_FAILURES_BEFORE_UNREACHABLE: u32 = 3;

/// One agent's current presence, as shown next to its name.
#[derive(Clone, Debug, Serialize)]
pub struct Presence {
    pub state: PresenceState,
    /// Set while `state` is `waiting_on`.
    pub waiting_reason: Option<WaitReason>,
    /// Who the agent is waiting on. aoe does not say who will answer an
    /// approval, so this is the agent's owner.
    pub waiting_on_user_id: Option<i32>,
    pub detail: Option<String>,
    pub since: chrono::DateTime<Utc>,
}

impl Presence {
    fn new(state: PresenceState) -> Self {
        Self {
            state,
            waiting_reason: None,
            waiting_on_user_id: None,
            detail: None,
            since: Utc::now(),
        }
    }
}

/// How long a person counts as present after their page last asked for the
/// meeting. Pages poll every couple of seconds.
const PRESENT_FOR: Duration = Duration::from_secs(6);
/// The window the autonomous hourly ceiling is measured over.
const SPEND_WINDOW: Duration = Duration::from_secs(3600);

#[derive(Default)]
pub struct Runtime {
    meeting_locks: Mutex<HashMap<i32, Arc<tokio::sync::Mutex<()>>>>,
    presence: Mutex<HashMap<i32, Presence>>,
    /// Participants with a turn in flight: at most one each.
    busy: Mutex<HashSet<i32>>,
    /// When each (meeting, person) was last seen with the meeting open.
    present: Mutex<HashMap<(i32, i32), Instant>>,
    /// What autonomous turns cost, per meeting: when, and in US cents.
    spend: Mutex<HashMap<i32, Vec<(Instant, f64)>>>,
    /// The last total cost reported for each session, in US dollars, so that a
    /// turn's cost is the change since the one before.
    session_cost: Mutex<HashMap<String, f64>>,
}

pub static RUNTIME: LazyLock<Runtime> = LazyLock::new(Runtime::default);

impl Runtime {
    /// Note that a person has the meeting open right now.
    pub fn touch_person(&self, meeting_id: i32, user_id: i32) {
        self.present
            .lock()
            .unwrap()
            .insert((meeting_id, user_id), Instant::now());
    }

    /// Whether a person has had the meeting open within the last few seconds.
    pub fn person_present(&self, meeting_id: i32, user_id: i32) -> bool {
        self.present
            .lock()
            .unwrap()
            .get(&(meeting_id, user_id))
            .is_some_and(|seen| seen.elapsed() < PRESENT_FOR)
    }

    /// Record what an autonomous turn cost, in US cents.
    fn record_spend(&self, meeting_id: i32, cents: f64) {
        self.spend
            .lock()
            .unwrap()
            .entry(meeting_id)
            .or_default()
            .push((Instant::now(), cents));
    }

    /// What autonomous turns in this meeting cost over the last hour, in cents.
    pub fn spent_last_hour(&self, meeting_id: i32) -> f64 {
        let mut spend = self.spend.lock().unwrap();
        let entries = spend.entry(meeting_id).or_default();
        entries.retain(|(at, _)| at.elapsed() < SPEND_WINDOW);
        entries.iter().map(|(_, cents)| cents).sum()
    }

    /// The change in a session's total cost since this was last called for it,
    /// in US dollars. The first report only sets the baseline.
    fn cost_since_last(&self, session_id: &str, total_usd: f64) -> f64 {
        let mut costs = self.session_cost.lock().unwrap();
        let before = costs.insert(session_id.to_string(), total_usd);
        before.map_or(0.0, |b| (total_usd - b).max(0.0))
    }

    fn meeting_lock(&self, meeting_id: i32) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.meeting_locks.lock().unwrap();
        locks.entry(meeting_id).or_default().clone()
    }

    pub fn presence_of(&self, participant_id: i32) -> Option<Presence> {
        self.presence.lock().unwrap().get(&participant_id).cloned()
    }

    /// Drop what is known about a participant who has left the meeting.
    pub fn forget(&self, participant_id: i32) {
        self.presence.lock().unwrap().remove(&participant_id);
    }

    fn set_presence(&self, participant_id: i32, presence: Presence) {
        self.presence
            .lock()
            .unwrap()
            .insert(participant_id, presence);
    }

    /// Claim the participant's single turn slot; false if a turn is running.
    fn claim(&self, participant_id: i32) -> bool {
        self.busy.lock().unwrap().insert(participant_id)
    }

    fn release(&self, participant_id: i32) {
        self.busy.lock().unwrap().remove(&participant_id);
    }
}

/// Add a message to a meeting. The next `seq` and, for an agent, the loop
/// guard are decided under one per-meeting lock at insert time, so two agents
/// finishing together cannot both post past the guard or collide on `seq`.
/// Returns `None` when the loop guard holds an agent's message back.
pub async fn post_message(
    db: &DatabaseConnection,
    meeting_id: i32,
    participant_id: i32,
    poster_is_agent: bool,
    poster_is_permanent: bool,
    dialog_type: DialogType,
    body: &str,
) -> Result<Option<meeting_message::Model>, AppError> {
    let lock = RUNTIME.meeting_lock(meeting_id);
    let _guard = lock.lock().await;

    let meeting = meeting::Entity::find_by_id(meeting_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("meeting not found"))?;
    if meeting.status != "open" {
        return Err(AppError::Conflict("meeting is closed"));
    }

    // Ordinary agents are bounded by one cap. Permanent agents talking among
    // themselves, perhaps with no person present, by a separate, larger one that
    // counts only a run of permanent agents' messages.
    let cap = if poster_is_permanent {
        meeting.max_consecutive_permanent_turns
    } else {
        meeting.max_consecutive_agent_turns
    }
    .max(0) as usize;
    let recent = meeting_message::Entity::find()
        .filter(meeting_message::Column::MeetingId.eq(meeting_id))
        .order_by_desc(meeting_message::Column::Seq)
        .limit(cap.max(1) as u64)
        .all(db)
        .await?;
    if poster_is_agent {
        let mut recent_counts = Vec::with_capacity(recent.len());
        // `recent` is newest first; the guard wants newest last.
        for message in recent.iter().rev() {
            let who = meeting_participant::Entity::find_by_id(message.participant_id)
                .one(db)
                .await?;
            recent_counts.push(match who {
                Some(p) if poster_is_permanent => p.agent_id.is_some() && p.permanent,
                Some(p) => p.agent_id.is_some(),
                None => false,
            });
        }
        if !agent_turn_allowed(&recent_counts, cap) {
            return Ok(None);
        }
    }

    let next_seq = meeting_message::Entity::find()
        .filter(meeting_message::Column::MeetingId.eq(meeting_id))
        .order_by_desc(meeting_message::Column::Seq)
        .one(db)
        .await?
        .map_or(1, |m| m.seq + 1);

    let message = meeting_message::ActiveModel {
        meeting_id: Set(meeting_id),
        seq: Set(next_seq),
        participant_id: Set(participant_id),
        dialog_type: Set(dialog_type.as_str().to_string()),
        body: Set(body.to_string()),
        revealed: Set(false),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(Some(message))
}

/// The mode an agent participant actually runs with in this meeting.
pub fn effective_turn_mode(
    participant: &meeting_participant::Model,
    agent: &townhall_agent::Model,
    meeting: &meeting::Model,
) -> TurnMode {
    let owner_mode = participant
        .turn_mode
        .as_deref()
        .and_then(TurnMode::parse)
        .unwrap_or(TurnMode::ModeratorCalled);
    let ceiling = TurnMode::parse(&meeting.agent_mode_ceiling).unwrap_or(TurnMode::ModeratorCalled);
    effective_mode(
        owner_mode,
        ceiling,
        agent.owner_user_id == meeting.moderator_user_id,
    )
}

/// Whether `mode` lets an agent speak up in answer to this message.
fn mode_triggers(mode: TurnMode, agent_name: &str, body: &str) -> bool {
    match mode {
        TurnMode::EveryMessage => true,
        TurnMode::MentionOnly => body
            .to_lowercase()
            .contains(&format!("@{}", agent_name.to_lowercase())),
        TurnMode::ModeratorCalled => false,
    }
}

/// After a message is posted, start a turn for every agent its mode lets speak.
pub async fn dispatch_for_message(
    state: &AppState,
    meeting_id: i32,
    message: &meeting_message::Model,
) -> Result<(), AppError> {
    let meeting = meeting::Entity::find_by_id(meeting_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound("meeting not found"))?;
    let participants = meeting_participant::Entity::find()
        .filter(meeting_participant::Column::MeetingId.eq(meeting_id))
        .filter(meeting_participant::Column::LeftAt.is_null())
        .filter(meeting_participant::Column::AgentId.is_not_null())
        .all(&state.db)
        .await?;

    // A permanent agent prompts only the agents it names and is allowed to
    // reach. Nothing else it says wakes anyone.
    let poster = meeting_participant::Entity::find_by_id(message.participant_id)
        .one(&state.db)
        .await?;
    if poster
        .as_ref()
        .is_some_and(|p| p.permanent && p.agent_id.is_some())
    {
        let allowed = allowed_targets(&state.db, message.participant_id).await?;
        let mut named_ids = Vec::new();
        for name in mentioned_names(&message.body) {
            for p in &participants {
                let Some(agent_id) = p.agent_id else { continue };
                let Some(agent) = townhall_agent::Entity::find_by_id(agent_id)
                    .one(&state.db)
                    .await?
                else {
                    continue;
                };
                if p.permanent && agent.name.eq_ignore_ascii_case(&name) {
                    named_ids.push(p.id);
                }
            }
        }
        for target in reachable(&named_ids, &allowed) {
            if target != message.participant_id {
                start_turn(state, meeting_id, target, TurnReason::FromPermanent);
            }
        }
        return Ok(());
    }

    for participant in participants {
        if participant.id == message.participant_id {
            continue;
        }
        let Some(agent_id) = participant.agent_id else {
            continue;
        };
        let Some(agent) = townhall_agent::Entity::find_by_id(agent_id)
            .one(&state.db)
            .await?
        else {
            continue;
        };
        let mode = effective_turn_mode(&participant, &agent, &meeting);
        if mode_triggers(mode, &agent.name, &message.body) {
            start_turn(state, meeting_id, participant.id, TurnReason::Spoken);
        }
    }
    Ok(())
}

/// Why an agent's turn starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnReason {
    /// A message that its turn mode lets it answer.
    Spoken,
    /// The moderator called on it.
    CalledOn,
    /// A permanent agent's schedule came due.
    Wake,
    /// Another permanent agent addressed it.
    FromPermanent,
}

impl TurnReason {
    /// A turn nobody asked for in the moment, which the hourly ceiling bounds.
    fn is_autonomous(self) -> bool {
        matches!(self, TurnReason::Wake | TurnReason::FromPermanent)
    }
}

/// The permanent agents `from` may address.
async fn allowed_targets(db: &DatabaseConnection, from: i32) -> Result<Vec<i32>, AppError> {
    Ok(meeting_converse_rule::Entity::find()
        .filter(meeting_converse_rule::Column::FromParticipantId.eq(from))
        .all(db)
        .await?
        .into_iter()
        .map(|r| r.to_participant_id)
        .collect())
}

/// Start one agent turn in the background, unless one is already running.
pub fn start_turn(state: &AppState, meeting_id: i32, participant_id: i32, reason: TurnReason) {
    if !RUNTIME.claim(participant_id) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        run_turn(&state, meeting_id, participant_id, reason).await;
        RUNTIME.release(participant_id);
    });
}

/// Wake each permanent agent whose schedule has come due, so it can work on its
/// objective with no person present. Runs for the life of the process.
pub async fn permanent_ticker(state: AppState) {
    let mut every = tokio::time::interval(WAKE_CHECK_EVERY);
    loop {
        every.tick().await;
        if let Err(e) = wake_due_agents(&state).await {
            tracing::warn!("town hall wake check failed: {e}");
        }
    }
}

async fn wake_due_agents(state: &AppState) -> Result<(), AppError> {
    let due = meeting_participant::Entity::find()
        .filter(meeting_participant::Column::Permanent.eq(true))
        .filter(meeting_participant::Column::LeftAt.is_null())
        .filter(meeting_participant::Column::WakeEveryMinutes.is_not_null())
        .filter(meeting_participant::Column::Objective.is_not_null())
        .all(&state.db)
        .await?;
    let now = Utc::now();
    for row in due {
        let Some(minutes) = row.wake_every_minutes else {
            continue;
        };
        let overdue = row
            .last_woken_at
            .is_none_or(|at| now - at >= chrono::Duration::minutes(i64::from(minutes)));
        if !overdue {
            continue;
        }
        let open = meeting::Entity::find_by_id(row.meeting_id)
            .one(&state.db)
            .await?
            .is_some_and(|m| m.status == "open");
        if !open {
            continue;
        }
        let (meeting_id, id) = (row.meeting_id, row.id);
        let mut woken: meeting_participant::ActiveModel = row.into();
        woken.last_woken_at = Set(Some(now));
        woken.update(&state.db).await?;
        start_turn(state, meeting_id, id, TurnReason::Wake);
    }
    Ok(())
}

async fn run_turn(state: &AppState, meeting_id: i32, participant_id: i32, reason: TurnReason) {
    if let Err(e) = try_run_turn(state, meeting_id, participant_id, reason).await {
        tracing::warn!("town hall turn failed: {e}");
        let mut presence = Presence::new(PresenceState::Unreachable);
        presence.detail = Some(e.to_string());
        RUNTIME.set_presence(participant_id, presence);
    }
}

async fn try_run_turn(
    state: &AppState,
    meeting_id: i32,
    participant_id: i32,
    reason: TurnReason,
) -> Result<(), AppError> {
    let db = &state.db;
    let called_on = reason == TurnReason::CalledOn;
    let meeting = meeting::Entity::find_by_id(meeting_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("meeting not found"))?;
    let participant = meeting_participant::Entity::find_by_id(participant_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("participant not found"))?;
    let agent = townhall_agent::Entity::find_by_id(
        participant
            .agent_id
            .ok_or(AppError::BadRequest("participant is not an agent"))?,
    )
    .one(db)
    .await?
    .ok_or(AppError::NotFound("agent not found"))?;

    // The diff: everything after this agent's last turn.
    let unseen = meeting_message::Entity::find()
        .filter(meeting_message::Column::MeetingId.eq(meeting_id))
        .filter(meeting_message::Column::Seq.gt(participant.last_turn_seq))
        .order_by_asc(meeting_message::Column::Seq)
        .all(db)
        .await?;
    // Autonomous turns are bounded by an hourly ceiling on what they cost.
    if reason.is_autonomous()
        && !within_hourly_budget(
            RUNTIME.spent_last_hour(meeting_id),
            meeting.autonomous_hourly_cents,
        )
    {
        let mut presence = Presence::new(PresenceState::Available);
        presence.detail = Some(format!(
            "held back: the hourly ceiling of ${:.2} is spent",
            f64::from(meeting.autonomous_hourly_cents) / 100.0
        ));
        RUNTIME.set_presence(participant_id, presence);
        return Ok(());
    }
    // A woken permanent agent works on its objective even if nothing was said.
    let highest_shown = match unseen.last().map(|m| m.seq) {
        Some(seq) => seq,
        None if reason == TurnReason::Wake => participant.last_turn_seq,
        None if !called_on => return Ok(()),
        None => return Err(AppError::BadRequest("nothing new to respond to")),
    };
    let mut speakers: HashMap<i32, (String, bool)> = HashMap::new();
    for message in &unseen {
        if let std::collections::hash_map::Entry::Vacant(slot) =
            speakers.entry(message.participant_id)
        {
            slot.insert(speaker_of(db, message.participant_id).await?);
        }
    }
    let lines: Vec<Line> = unseen
        .iter()
        .map(|m| {
            let (name, is_agent) = &speakers[&m.participant_id];
            Line {
                speaker: name,
                is_agent: *is_agent,
                body: &m.body,
            }
        })
        .collect();
    // A permanent agent is told its objective and exactly whom it may address.
    let may_address: Option<Vec<String>> = if participant.permanent {
        let mut names = Vec::new();
        for target in allowed_targets(db, participant_id).await? {
            names.push(speaker_of(db, target).await?.0);
        }
        Some(names)
    } else {
        None
    };
    let prompt = build_prompt(
        &meeting.title,
        &agent.name,
        &TurnContext {
            called_on,
            objective: participant
                .permanent
                .then_some(participant.objective.as_deref())
                .flatten(),
            may_address: may_address.as_deref(),
        },
        &lines,
    );
    let is_permanent = participant.permanent;

    RUNTIME.set_presence(participant_id, Presence::new(PresenceState::Waking));
    let client = WorkspaceClient::for_user(state, agent.owner_user_id).await?;
    let ack = client.send_prompt(&agent.session_id, &prompt).await?;
    tracing::info!(
        "town hall: prompt to agent {} accepted ({}), seq {}",
        agent.id,
        ack.disposition,
        ack.seq
    );

    let mut progress = TurnProgress::new(ack.seq.saturating_sub(1));
    let started = tokio::time::Instant::now();
    let mut failures = 0u32;
    while progress.ended.is_none() {
        if started.elapsed() > GIVE_UP_AFTER {
            let mut presence = presence_from(&progress, agent.owner_user_id);
            presence.detail = Some("gave up waiting for an answer".to_string());
            RUNTIME.set_presence(participant_id, presence);
            return Ok(());
        }
        tokio::time::sleep(POLL_EVERY).await;
        match client
            .events_since(&agent.session_id, progress.last_seq)
            .await
        {
            Ok(frames) => {
                failures = 0;
                progress.apply(&frames);
                RUNTIME.set_presence(
                    participant_id,
                    presence_from(&progress, agent.owner_user_id),
                );
            }
            Err(e) => {
                failures += 1;
                if failures >= POLL_FAILURES_BEFORE_UNREACHABLE {
                    let mut presence = Presence::new(PresenceState::Unreachable);
                    presence.detail = Some(e.to_string());
                    RUNTIME.set_presence(participant_id, presence);
                    return Ok(());
                }
            }
        }
    }

    // Dismissed while it was thinking: whatever it says is discarded.
    let still_in_room = meeting_participant::Entity::find_by_id(participant_id)
        .one(db)
        .await?
        .is_some_and(|p| p.left_at.is_none());
    if !still_in_room {
        RUNTIME.forget(participant_id);
        return Ok(());
    }

    // What the turn cost counts against the hourly ceiling when nobody asked for it.
    if let Some(total) = progress.cost_usd {
        let cents = RUNTIME.cost_since_last(&agent.session_id, total) * 100.0;
        if reason.is_autonomous() {
            RUNTIME.record_spend(meeting_id, cents);
        }
    }

    // The turn is over: either way the agent has now seen the diff.
    let mut seen = meeting_participant::ActiveModel::from(participant);
    seen.last_turn_seq = Set(highest_shown);
    seen.update(db).await?;

    match progress.ended {
        Some(TurnEnd::Failed(message)) => {
            let mut presence = Presence::new(PresenceState::Unreachable);
            presence.detail = Some(message);
            RUNTIME.set_presence(participant_id, presence);
        }
        _ => {
            let mut presence = Presence::new(PresenceState::Available);
            match parse_reply(&progress.reply) {
                Reply::Pass => presence.detail = Some("passed".to_string()),
                Reply::Say { dialog_type, body } => {
                    match post_message(
                        db,
                        meeting_id,
                        participant_id,
                        true,
                        is_permanent,
                        dialog_type,
                        &body,
                    )
                    .await?
                    {
                        Some(posted) => {
                            presence.detail = Some("spoke".to_string());
                            // Other agents may want to answer this one; the loop
                            // guard decides how far that goes.
                            dispatch_for_message(state, meeting_id, &posted).await?;
                        }
                        None => presence.detail = Some("held back by the loop guard".to_string()),
                    }
                }
            }
            RUNTIME.set_presence(participant_id, presence);
        }
    }
    Ok(())
}

fn presence_from(progress: &TurnProgress, owner_user_id: i32) -> Presence {
    let mut presence = Presence::new(progress.state);
    if progress.state == PresenceState::WaitingOn {
        presence.waiting_reason = progress.waiting;
        presence.waiting_on_user_id = Some(owner_user_id);
    }
    presence
}

/// A participant's display name and whether it is an agent.
pub async fn speaker_of(
    db: &DatabaseConnection,
    participant_id: i32,
) -> Result<(String, bool), AppError> {
    let participant = meeting_participant::Entity::find_by_id(participant_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("participant not found"))?;
    if let Some(agent_id) = participant.agent_id {
        let name = townhall_agent::Entity::find_by_id(agent_id)
            .one(db)
            .await?
            .map_or_else(|| "an agent".to_string(), |a| a.name);
        return Ok((name, true));
    }
    let name = match participant.user_id {
        Some(user_id) => user::Entity::find_by_id(user_id)
            .one(db)
            .await?
            .map_or_else(|| "someone".to_string(), |u| u.username),
        None => "someone".to_string(),
    };
    Ok((name, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_decide_what_triggers_an_agent() {
        assert!(mode_triggers(TurnMode::EveryMessage, "Rev", "anything"));
        assert!(mode_triggers(TurnMode::MentionOnly, "Rev", "thanks @rev!"));
        assert!(!mode_triggers(TurnMode::MentionOnly, "Rev", "no mention"));
        assert!(!mode_triggers(TurnMode::ModeratorCalled, "Rev", "@Rev"));
    }

    #[test]
    fn a_participant_gets_one_turn_slot_at_a_time() {
        let runtime = Runtime::default();
        assert!(runtime.claim(7));
        assert!(!runtime.claim(7));
        runtime.release(7);
        assert!(runtime.claim(7));
    }
}
