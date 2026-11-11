use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

use super::m0022_create_meeting_participants::MeetingParticipants;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Whether other people in the room see this agent in the room list. New
        // agents start hidden; agents already in a room stay visible, so nothing
        // vanishes when this is applied. People are unaffected by the flag.
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingParticipants::Table)
                    .add_column(boolean(Alias::new("visible_to_others")).default(false))
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared("UPDATE meeting_participants SET visible_to_others = TRUE")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingParticipants::Table)
                    .drop_column(Alias::new("visible_to_others"))
                    .to_owned(),
            )
            .await
    }
}
