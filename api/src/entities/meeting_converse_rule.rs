use sea_orm::entity::prelude::*;

/// One permission for a permanent agent: `from_participant_id` may address
/// `to_participant_id`. Directional, and an agent not listed may not be addressed.
/// See `docs/townhall.md`, "Permanent resident agents".
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "converse_rules")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub meeting_id: i32,
    pub from_participant_id: i32,
    pub to_participant_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
