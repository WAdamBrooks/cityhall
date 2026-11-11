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
        // The waiting room: one row per thing asked for. A row with no agent is
        // the person asking to come in; a row with an agent is that agent being
        // brought by its owner (`user_id`). Each is decided on its own. "Ignored"
        // is not a status: an ignored request simply stays pending.
        manager
            .create_table(
                Table::create()
                    .table(MeetingRequests::Table)
                    .if_not_exists()
                    .col(pk_auto(MeetingRequests::Id))
                    .col(integer(MeetingRequests::MeetingId))
                    .col(integer(MeetingRequests::UserId))
                    .col(integer_null(MeetingRequests::AgentId))
                    // The owner's wanted turn mode for an agent; null for a person.
                    .col(string_null(MeetingRequests::TurnMode))
                    // `pending`, `approved` or `denied`.
                    .col(string(MeetingRequests::Status).default("pending"))
                    // Whether the requester had been dismissed from this meeting
                    // when they asked, so the moderator can see it.
                    .col(boolean(MeetingRequests::DismissedEarlier).default(false))
                    .col(timestamp_with_time_zone(MeetingRequests::CreatedAt))
                    .col(timestamp_with_time_zone_null(MeetingRequests::DecidedAt))
                    .col(integer_null(MeetingRequests::DecidedByUserId))
                    .foreign_key(
                        ForeignKey::create()
                            .from(MeetingRequests::Table, MeetingRequests::MeetingId)
                            .to(Meetings::Table, Meetings::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(MeetingRequests::Table, MeetingRequests::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(MeetingRequests::Table, MeetingRequests::AgentId)
                            .to(TownhallAgents::Table, TownhallAgents::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_meeting_requests_meeting_status")
                    .table(MeetingRequests::Table)
                    .col(MeetingRequests::MeetingId)
                    .col(MeetingRequests::Status)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MeetingRequests::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum MeetingRequests {
    Table,
    Id,
    MeetingId,
    UserId,
    AgentId,
    TurnMode,
    Status,
    DismissedEarlier,
    CreatedAt,
    DecidedAt,
    DecidedByUserId,
}
