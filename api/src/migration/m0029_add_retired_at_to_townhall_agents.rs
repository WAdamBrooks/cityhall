use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // An owner deleting an agent retires it instead of removing the row: the
        // agent's messages in past meetings hang off it, and deleting it would
        // erase them from every other person's history.
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("townhall_agents"))
                    .add_column(timestamp_with_time_zone_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("townhall_agents"))
                    .drop_column(Alias::new("retired_at"))
                    .to_owned(),
            )
            .await
    }
}

fn timestamp_with_time_zone_null() -> ColumnDef {
    ColumnDef::new(Alias::new("retired_at"))
        .timestamp_with_time_zone()
        .null()
        .to_owned()
}
