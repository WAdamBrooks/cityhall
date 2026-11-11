use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

use super::m0001_create_users::Users;
use super::m0020_create_townhall_agents::TownhallAgents;
use super::m0021_create_meetings::Meetings;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // A participant is a user or an agent. Exactly one of `user_id` and
        // `agent_id` is set; that is checked in code because a portable CHECK
        // constraint is awkward across SQLite, Postgres and MySQL. Rows are kept
        // after a participant leaves (`left_at`) so history keeps its speaker.
        manager
            .create_table(
                Table::create()
                    .table(MeetingParticipants::Table)
                    .if_not_exists()
                    .col(pk_auto(MeetingParticipants::Id))
                    .col(integer(MeetingParticipants::MeetingId))
                    .col(integer_null(MeetingParticipants::UserId))
                    .col(integer_null(MeetingParticipants::AgentId))
                    // The owner's chosen turn mode, stored as chosen. The
                    // effective mode (clamped by the meeting's ceiling) is
                    // derived, never written here. Null for humans.
                    .col(string_null(MeetingParticipants::TurnMode))
                    // Highest message seq this participant had been shown when it
                    // last took a turn: the "diff since last turn" cursor.
                    .col(big_integer(MeetingParticipants::LastTurnSeq).default(0))
                    .col(timestamp_with_time_zone(MeetingParticipants::JoinedAt))
                    .col(timestamp_with_time_zone_null(MeetingParticipants::LeftAt))
                    .foreign_key(
                        ForeignKey::create()
                            .from(MeetingParticipants::Table, MeetingParticipants::MeetingId)
                            .to(Meetings::Table, Meetings::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(MeetingParticipants::Table, MeetingParticipants::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(MeetingParticipants::Table, MeetingParticipants::AgentId)
                            .to(TownhallAgents::Table, TownhallAgents::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        // A person or an agent joins a given meeting at most once. SQL treats
        // NULLs as distinct in a unique index, so the unset side never collides.
        manager
            .create_index(
                Index::create()
                    .name("idx_meeting_participants_meeting_user")
                    .table(MeetingParticipants::Table)
                    .col(MeetingParticipants::MeetingId)
                    .col(MeetingParticipants::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_meeting_participants_meeting_agent")
                    .table(MeetingParticipants::Table)
                    .col(MeetingParticipants::MeetingId)
                    .col(MeetingParticipants::AgentId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MeetingParticipants::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum MeetingParticipants {
    Table,
    Id,
    MeetingId,
    UserId,
    AgentId,
    TurnMode,
    LastTurnSeq,
    JoinedAt,
    LeftAt,
    LeftByUserId,
}
