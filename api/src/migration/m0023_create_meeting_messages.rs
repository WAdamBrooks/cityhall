use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

use super::m0021_create_meetings::Meetings;
use super::m0022_create_meeting_participants::MeetingParticipants;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // The conversation. `seq` is monotonic per meeting, so "the diff since an
        // agent's last turn" is `seq > meeting_participants.last_turn_seq`.
        manager
            .create_table(
                Table::create()
                    .table(MeetingMessages::Table)
                    .if_not_exists()
                    .col(pk_auto(MeetingMessages::Id))
                    .col(integer(MeetingMessages::MeetingId))
                    .col(big_integer(MeetingMessages::Seq))
                    .col(integer(MeetingMessages::ParticipantId))
                    // contribution, thought, offer_help, progress_report,
                    // clarifying_question, handoff. Validated in code.
                    .col(string(MeetingMessages::DialogType))
                    .col(text(MeetingMessages::Body))
                    // Only meaningful for `thought`: false means other
                    // participants see a dots placeholder, not the body.
                    .col(boolean(MeetingMessages::Revealed).default(false))
                    .col(timestamp_with_time_zone(MeetingMessages::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .from(MeetingMessages::Table, MeetingMessages::MeetingId)
                            .to(Meetings::Table, Meetings::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(MeetingMessages::Table, MeetingMessages::ParticipantId)
                            .to(MeetingParticipants::Table, MeetingParticipants::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_meeting_messages_meeting_seq")
                    .table(MeetingMessages::Table)
                    .col(MeetingMessages::MeetingId)
                    .col(MeetingMessages::Seq)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MeetingMessages::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum MeetingMessages {
    Table,
    Id,
    MeetingId,
    Seq,
    ParticipantId,
    DialogType,
    Body,
    Revealed,
    CreatedAt,
}
