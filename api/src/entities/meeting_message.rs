use sea_orm::entity::prelude::*;

/// One message in a meeting. `seq` is monotonic per meeting, so the diff since a
/// participant's last turn is `seq > last_turn_seq`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "meeting_messages")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub meeting_id: i32,
    pub seq: i64,
    pub participant_id: i32,
    /// See `crate::townhall::DialogType`.
    pub dialog_type: String,
    pub body: String,
    /// Only meaningful for `thought`: while false, other participants must not
    /// be sent the body, only a dots placeholder.
    pub revealed: bool,
    pub created_at: DateTimeUtc,
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
        belongs_to = "super::meeting_participant::Entity",
        from = "Column::ParticipantId",
        to = "super::meeting_participant::Column::Id",
        on_delete = "Cascade"
    )]
    Participant,
}

impl ActiveModelBehavior for ActiveModel {}
