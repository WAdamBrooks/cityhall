use sea_orm::entity::prelude::*;

/// A meeting participant: a user or an agent. Exactly one of `user_id` and
/// `agent_id` is set (checked in code). Rows are kept after a participant
/// leaves so history keeps its speaker.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "meeting_participants")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub meeting_id: i32,
    pub user_id: Option<i32>,
    pub agent_id: Option<i32>,
    /// The owner's chosen turn mode, stored as chosen. The effective mode is
    /// derived with `crate::townhall::effective_mode`, never written here.
    /// `None` for humans.
    pub turn_mode: Option<String>,
    /// Highest message `seq` this participant had been shown when it last took a
    /// turn: the "diff since last turn" cursor.
    pub last_turn_seq: i64,
    pub joined_at: DateTimeUtc,
    pub left_at: Option<DateTimeUtc>,
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
