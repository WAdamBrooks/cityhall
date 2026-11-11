use sea_orm::entity::prelude::*;

/// A town hall meeting and its moderator-controlled settings. See
/// `docs/townhall.md`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "meetings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub title: String,
    pub moderator_user_id: i32,
    /// `open` or `closed`.
    pub status: String,
    /// Whether people other than the moderator may enroll their agents.
    pub allow_member_agents: bool,
    /// The most privileged turn mode another person's agent may use; see
    /// `crate::townhall::TurnMode`.
    pub agent_mode_ceiling: String,
    /// Agent messages in a row, with no human message between them, after which
    /// agents must wait for a human.
    pub max_consecutive_agent_turns: i32,
    /// Permanent agent messages in a row, with no person between, after which
    /// they must wait for a person.
    pub max_consecutive_permanent_turns: i32,
    /// The most the autonomous turns (permanent agents working with no person
    /// prompting them) may cost in an hour, in US cents.
    pub autonomous_hourly_cents: i32,
    pub created_at: DateTimeUtc,
    pub closed_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::ModeratorUserId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    Moderator,
}

impl ActiveModelBehavior for ActiveModel {}
