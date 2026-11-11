use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

use super::m0022_create_meeting_participants::MeetingParticipants;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // An assistant moderator is a person the moderator has trusted to make and
        // adjust permanent agent assignments. It says nothing else about them.
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingParticipants::Table)
                    .add_column(boolean(Alias::new("is_assistant")).default(false))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingParticipants::Table)
                    .drop_column(Alias::new("is_assistant"))
                    .to_owned(),
            )
            .await
    }
}
