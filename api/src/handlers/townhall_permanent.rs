//! Permanent resident agents: agents that stay in a room to work on a standing
//! objective, with an explicit list of the other permanent agents each may
//! address. See `docs/townhall.md`, "Permanent resident agents".

use std::collections::HashSet;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::Deserialize;

use super::townhall::{gate, load_meeting, require_assigner};
use crate::auth::AuthUser;
use crate::entities::{meeting_converse_rule, meeting_participant};
use crate::error::AppError;
use crate::state::AppState;

/// Bounds on how often a permanent agent may be woken to work on its objective.
const MIN_WAKE_MINUTES: i32 = 5;
const MAX_WAKE_MINUTES: i32 = 24 * 60;
const MAX_OBJECTIVE_CHARS: usize = 2000;

/// An agent that is in the room, in this meeting.
async fn agent_row(
    db: &DatabaseConnection,
    meeting_id: i32,
    pid: i32,
) -> Result<meeting_participant::Model, AppError> {
    meeting_participant::Entity::find_by_id(pid)
        .one(db)
        .await?
        .filter(|p| p.meeting_id == meeting_id && p.left_at.is_none() && p.agent_id.is_some())
        .ok_or(AppError::NotFound("agent not found in this meeting"))
}

/// The ids of the permanent agents in the room.
async fn permanent_ids(db: &DatabaseConnection, meeting_id: i32) -> Result<HashSet<i32>, AppError> {
    Ok(meeting_participant::Entity::find()
        .filter(meeting_participant::Column::MeetingId.eq(meeting_id))
        .filter(meeting_participant::Column::LeftAt.is_null())
        .filter(meeting_participant::Column::Permanent.eq(true))
        .all(db)
        .await?
        .into_iter()
        .map(|p| p.id)
        .collect())
}

/// A permanent agent may address only other permanent agents, never itself.
fn check_targets(
    from: i32,
    targets: &[i32],
    permanent: &HashSet<i32>,
    becoming: Option<i32>,
) -> Result<(), AppError> {
    for &t in targets {
        if t == from {
            return Err(AppError::BadRequest("an agent cannot address itself"));
        }
        if !permanent.contains(&t) && Some(t) != becoming {
            return Err(AppError::BadRequest(
                "an agent can only be allowed to address permanent agents",
            ));
        }
    }
    Ok(())
}

async fn set_rules(
    db: &DatabaseConnection,
    meeting_id: i32,
    from: i32,
    targets: &[i32],
) -> Result<(), AppError> {
    meeting_converse_rule::Entity::delete_many()
        .filter(meeting_converse_rule::Column::FromParticipantId.eq(from))
        .exec(db)
        .await?;
    let mut seen = HashSet::new();
    for &to in targets {
        if seen.insert(to) {
            meeting_converse_rule::ActiveModel {
                meeting_id: Set(meeting_id),
                from_participant_id: Set(from),
                to_participant_id: Set(to),
                ..Default::default()
            }
            .insert(db)
            .await?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct OtherList {
    pub participant_id: i32,
    pub converse_with: Vec<i32>,
}

#[derive(Deserialize)]
pub struct MakePermanent {
    /// Step one: the agents the new permanent agent may address.
    pub converse_with: Vec<i32>,
    /// Step two: the new lists of the permanent agents already in the room, which
    /// may now include the new one.
    #[serde(default)]
    pub others: Vec<OtherList>,
    /// What it is working toward, and how often (minutes) to wake it for that.
    pub objective: Option<String>,
    pub wake_every_minutes: Option<i32>,
}

/// PUT /api/townhall/meetings/{id}/participants/{pid}/permanent: the moderator
/// makes an agent a permanent resident, or changes its terms.
pub async fn make_permanent(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((id, pid)): Path<(i32, i32)>,
    Json(req): Json<MakePermanent>,
) -> Result<StatusCode, AppError> {
    gate(&caller)?;
    let db = &state.db;
    let meeting = load_meeting(db, id).await?;
    require_assigner(db, &caller, &meeting).await?;
    if meeting.status != "open" {
        return Err(AppError::Conflict("meeting is closed"));
    }
    let row = agent_row(db, id, pid).await?;

    let objective = req
        .objective
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if objective.is_some_and(|o| o.chars().count() > MAX_OBJECTIVE_CHARS) {
        return Err(AppError::BadRequest("the objective is too long"));
    }
    if let Some(minutes) = req.wake_every_minutes {
        if objective.is_none() {
            return Err(AppError::BadRequest(
                "a schedule needs an objective to work on",
            ));
        }
        if !(MIN_WAKE_MINUTES..=MAX_WAKE_MINUTES).contains(&minutes) {
            return Err(AppError::BadRequest(
                "wake-ups must be between 5 minutes and 24 hours apart",
            ));
        }
    }

    // Check every list before changing any.
    let permanent = permanent_ids(db, id).await?;
    check_targets(pid, &req.converse_with, &permanent, None)?;
    for other in &req.others {
        if !permanent.contains(&other.participant_id) || other.participant_id == pid {
            return Err(AppError::BadRequest(
                "only the permanent agents already in the room can be adjusted",
            ));
        }
        check_targets(
            other.participant_id,
            &other.converse_with,
            &permanent,
            Some(pid),
        )?;
    }

    let mut active: meeting_participant::ActiveModel = row.into();
    active.permanent = Set(true);
    active.objective = Set(objective.map(str::to_string));
    active.wake_every_minutes = Set(req.wake_every_minutes);
    active.last_woken_at = Set(None);
    active.update(db).await?;
    set_rules(db, id, pid, &req.converse_with).await?;
    for other in &req.others {
        set_rules(db, id, other.participant_id, &other.converse_with).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SetConverse {
    pub converse_with: Vec<i32>,
}

/// PUT /api/townhall/meetings/{id}/participants/{pid}/converse: the moderator
/// changes whom a permanent agent may address.
pub async fn set_converse(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((id, pid)): Path<(i32, i32)>,
    Json(req): Json<SetConverse>,
) -> Result<StatusCode, AppError> {
    gate(&caller)?;
    let db = &state.db;
    let meeting = load_meeting(db, id).await?;
    require_assigner(db, &caller, &meeting).await?;
    let row = agent_row(db, id, pid).await?;
    if !row.permanent {
        return Err(AppError::BadRequest("that agent is not permanent"));
    }
    let permanent = permanent_ids(db, id).await?;
    check_targets(pid, &req.converse_with, &permanent, None)?;
    set_rules(db, id, pid, &req.converse_with).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/townhall/meetings/{id}/participants/{pid}/permanent: the agent
/// stays in the room but is no longer a permanent resident. Everyone's permission
/// to address it ends with that.
pub async fn end_permanence(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((id, pid)): Path<(i32, i32)>,
) -> Result<StatusCode, AppError> {
    gate(&caller)?;
    let db = &state.db;
    let meeting = load_meeting(db, id).await?;
    require_assigner(db, &caller, &meeting).await?;
    let row = agent_row(db, id, pid).await?;
    let mut active: meeting_participant::ActiveModel = row.into();
    active.permanent = Set(false);
    active.objective = Set(None);
    active.wake_every_minutes = Set(None);
    active.last_woken_at = Set(None);
    active.update(db).await?;
    meeting_converse_rule::Entity::delete_many()
        .filter(
            meeting_converse_rule::Column::FromParticipantId
                .eq(pid)
                .or(meeting_converse_rule::Column::ToParticipantId.eq(pid)),
        )
        .exec(db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[i32]) -> HashSet<i32> {
        ids.iter().copied().collect()
    }

    #[test]
    fn a_permanent_agent_may_list_only_other_permanent_agents() {
        let permanent = set(&[2, 3]);
        assert!(check_targets(1, &[2, 3], &permanent, None).is_ok());
        assert!(check_targets(1, &[], &permanent, None).is_ok());
        // Not permanent, and not the agent being made permanent now.
        assert!(check_targets(1, &[9], &permanent, None).is_err());
        // Itself is never a valid target.
        assert!(check_targets(2, &[2], &permanent, None).is_err());
    }

    #[test]
    fn step_two_may_name_the_agent_becoming_permanent() {
        let permanent = set(&[2]);
        assert!(check_targets(2, &[7], &permanent, Some(7)).is_ok());
        assert!(check_targets(2, &[7], &permanent, None).is_err());
    }
}
