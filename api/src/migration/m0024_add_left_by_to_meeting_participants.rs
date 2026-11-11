use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

use super::m0022_create_meeting_participants::MeetingParticipants;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Who ended a participation: the moderator dismissing someone, or an
        // owner dismissing their own agent. A dismissal by the moderator stands
        // against anyone else bringing that person or agent back. Null while the
        // participant is in the meeting. Not a foreign key: the record of who
        // acted should survive that user being deleted.
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingParticipants::Table)
                    .add_column(integer_null(MeetingParticipants::LeftByUserId))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingParticipants::Table)
                    .drop_column(MeetingParticipants::LeftByUserId)
                    .to_owned(),
            )
            .await
    }
}
