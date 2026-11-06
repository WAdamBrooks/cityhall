use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

use super::m0001_create_users::Users;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // A registered agent: an aoe session its owner offers to town hall
        // meetings. Admission (a non-empty description and tool list) is checked
        // in code; the columns are NOT NULL so the specialty is always present.
        manager
            .create_table(
                Table::create()
                    .table(TownhallAgents::Table)
                    .if_not_exists()
                    .col(pk_auto(TownhallAgents::Id))
                    .col(integer(TownhallAgents::OwnerUserId))
                    .col(string(TownhallAgents::SessionId))
                    .col(string(TownhallAgents::Name))
                    .col(text(TownhallAgents::Description))
                    // JSON array of tool names.
                    .col(text(TownhallAgents::Tools))
                    .col(timestamp_with_time_zone(TownhallAgents::CreatedAt))
                    .col(timestamp_with_time_zone(TownhallAgents::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .from(TownhallAgents::Table, TownhallAgents::OwnerUserId)
                            .to(Users::Table, Users::Id)
                            // Deleting the owner removes their agents with their
                            // workspace.
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_townhall_agents_owner_session")
                    .table(TownhallAgents::Table)
                    .col(TownhallAgents::OwnerUserId)
                    .col(TownhallAgents::SessionId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(TownhallAgents::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum TownhallAgents {
    Table,
    Id,
    OwnerUserId,
    SessionId,
    Name,
    Description,
    Tools,
    CreatedAt,
    UpdatedAt,
}
