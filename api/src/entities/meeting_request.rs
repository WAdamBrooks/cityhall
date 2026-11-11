use sea_orm::entity::prelude::*;

/// One line in a meeting's waiting room: a person asking to come in (no agent),
/// or an agent its owner asks to bring (`agent_id` set). Each line is decided on
/// its own. See `docs/townhall.md`, "Waiting room".
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "meeting_requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub meeting_id: i32,
    /// The person asking: for an agent line, its owner.
    pub user_id: i32,
    pub agent_id: Option<i32>,
    /// The owner's wanted turn mode for an agent line; `None` for a person.
    pub turn_mode: Option<String>,
    /// `invited` (an invitation not yet taken up), `pending` (waiting for the
    /// moderator), `approved`, `denied`, `declined` (the invitee said no) or
    /// `withdrawn` (removed without a verdict).
    pub status: String,
    /// Whether the requester had been dismissed from this meeting when they asked.
    pub dismissed_earlier: bool,
    pub created_at: DateTimeUtc,
    pub decided_at: Option<DateTimeUtc>,
    pub decided_by_user_id: Option<i32>,
    /// `ask` (the requester asked) or `invite` (someone invited them).
    pub kind: String,
    /// Who sent the invitation, for `kind = invite`.
    pub invited_by_user_id: Option<i32>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::meeting::Entity",
        from = "Column::MeetingId",
        to = "super::meeting::Column::Id",
        on_delete = "Cascade"
    )]
    Meeting,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::townhall_agent::Entity",
        from = "Column::AgentId",
        to = "super::townhall_agent::Column::Id",
        on_delete = "Cascade"
    )]
    Agent,
}

impl ActiveModelBehavior for ActiveModel {}
