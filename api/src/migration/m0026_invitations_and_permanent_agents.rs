use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

use super::m0021_create_meetings::Meetings;
use super::m0022_create_meeting_participants::MeetingParticipants;
use super::m0025_create_meeting_requests::MeetingRequests;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Invitations share the waiting room's table. `kind` tells a person's own
        // ask from an invitation, and `invited_by_user_id` says who invited.
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingRequests::Table)
                    .add_column(string(Alias::new("kind")).default("ask"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingRequests::Table)
                    .add_column(integer_null(Alias::new("invited_by_user_id")))
                    .to_owned(),
            )
            .await?;

        // Permanent resident agents: the flag, the standing objective, and how
        // often the agent is woken to work on it (minutes; null for never).
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingParticipants::Table)
                    .add_column(boolean(Alias::new("permanent")).default(false))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingParticipants::Table)
                    .add_column(string_null(Alias::new("objective")))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingParticipants::Table)
                    .add_column(integer_null(Alias::new("wake_every_minutes")))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MeetingParticipants::Table)
                    .add_column(timestamp_with_time_zone_null(Alias::new("last_woken_at")))
                    .to_owned(),
            )
            .await?;

        // Limits for agents working with no person present: how many permanent
        // agent messages may follow one another, and the most their turns may cost
        // in an hour, in US cents.
        manager
            .alter_table(
                Table::alter()
                    .table(Meetings::Table)
                    .add_column(integer(Alias::new("max_consecutive_permanent_turns")).default(10))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Meetings::Table)
                    .add_column(integer(Alias::new("autonomous_hourly_cents")).default(25))
                    .to_owned(),
            )
            .await?;

        // Who a permanent agent may address. Directional: A may address B says
        // nothing about B addressing A. An agent not listed may not be addressed.
        manager
            .create_table(
                Table::create()
                    .table(ConverseRules::Table)
                    .if_not_exists()
                    .col(pk_auto(ConverseRules::Id))
                    .col(integer(ConverseRules::MeetingId))
                    .col(integer(ConverseRules::FromParticipantId))
                    .col(integer(ConverseRules::ToParticipantId))
                    .foreign_key(
                        ForeignKey::create()
                            .from(ConverseRules::Table, ConverseRules::MeetingId)
                            .to(Meetings::Table, Meetings::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(ConverseRules::Table, ConverseRules::FromParticipantId)
                            .to(MeetingParticipants::Table, MeetingParticipants::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(ConverseRules::Table, ConverseRules::ToParticipantId)
                            .to(MeetingParticipants::Table, MeetingParticipants::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_converse_rules_pair")
                    .table(ConverseRules::Table)
                    .col(ConverseRules::FromParticipantId)
                    .col(ConverseRules::ToParticipantId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ConverseRules::Table).to_owned())
            .await?;
        for column in ["max_consecutive_permanent_turns", "autonomous_hourly_cents"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Meetings::Table)
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        for column in [
            "permanent",
            "objective",
            "wake_every_minutes",
            "last_woken_at",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(MeetingParticipants::Table)
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        for column in ["kind", "invited_by_user_id"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(MeetingRequests::Table)
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum ConverseRules {
    Table,
    Id,
    MeetingId,
    FromParticipantId,
    ToParticipantId,
}
