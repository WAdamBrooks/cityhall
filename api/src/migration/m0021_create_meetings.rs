use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

use super::m0001_create_users::Users;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // A town hall meeting. The moderator's settings gate who may bring
        // agents and how much turn-taking privilege those agents can have; see
        // docs/townhall.md.
        manager
            .create_table(
                Table::create()
                    .table(Meetings::Table)
                    .if_not_exists()
                    .col(pk_auto(Meetings::Id))
                    .col(string(Meetings::Title))
                    .col(integer(Meetings::ModeratorUserId))
                    // "open" or "closed".
                    .col(string(Meetings::Status).default("open"))
                    // Whether people other than the moderator may enroll agents.
                    .col(boolean(Meetings::AllowMemberAgents).default(false))
                    // The most privileged turn mode another person's agent may
                    // use: moderator_called < mention_only < every_message.
                    .col(string(Meetings::AgentModeCeiling).default("mention_only"))
                    // Loop guard: agent messages in a row, with no human message
                    // between them, after which agents must wait for a human.
                    .col(integer(Meetings::MaxConsecutiveAgentTurns).default(3))
                    .col(timestamp_with_time_zone(Meetings::CreatedAt))
                    .col(timestamp_with_time_zone_null(Meetings::ClosedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .from(Meetings::Table, Meetings::ModeratorUserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Meetings::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Meetings {
    Table,
    Id,
    Title,
    ModeratorUserId,
    Status,
    AllowMemberAgents,
    AgentModeCeiling,
    MaxConsecutiveAgentTurns,
    CreatedAt,
    ClosedAt,
}
