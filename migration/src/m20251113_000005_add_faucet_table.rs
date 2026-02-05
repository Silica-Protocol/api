use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_query::Expr;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Faucet requests table for tracking token distributions
        manager
            .create_table(
                Table::create()
                    .table(FaucetRequests::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(FaucetRequests::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(FaucetRequests::RecipientAddress)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FaucetRequests::IpAddress)
                            .string_len(45) // IPv6 max length
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FaucetRequests::Amount)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FaucetRequests::TxHash)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FaucetRequests::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    // Index for rate limiting by address
                    .index(
                        Index::create()
                            .name("idx_faucet_address_time")
                            .col(FaucetRequests::RecipientAddress)
                            .col(FaucetRequests::CreatedAt),
                    )
                    // Index for rate limiting by IP
                    .index(
                        Index::create()
                            .name("idx_faucet_ip_time")
                            .col(FaucetRequests::IpAddress)
                            .col(FaucetRequests::CreatedAt),
                    )
                    // Index for transaction lookup
                    .index(
                        Index::create()
                            .name("idx_faucet_tx_hash")
                            .col(FaucetRequests::TxHash),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(FaucetRequests::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum FaucetRequests {
    Table,
    Id,
    RecipientAddress,
    IpAddress,
    Amount,
    TxHash,
    CreatedAt,
}
