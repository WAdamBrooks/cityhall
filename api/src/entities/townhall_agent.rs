use sea_orm::entity::prelude::*;

/// An agent its owner offers to town hall meetings: an aoe session inside the
/// owner's workspace plus the specialty it is admitted on. See
/// `docs/townhall.md`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "townhall_agents")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub owner_user_id: i32,
    /// The aoe session id inside the owner's workspace.
    pub session_id: String,
    pub name: String,
    /// Specialty, in prose. Must be non-empty for the agent to be admitted.
    pub description: String,
    /// JSON array of tool names. Must be non-empty for the agent to be admitted.
    pub tools: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::OwnerUserId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    Owner,
}

impl ActiveModelBehavior for ActiveModel {}
