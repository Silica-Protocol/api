use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_query::Expr;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(StealthOutputs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(StealthOutputs::TxId)
                            .string_len(130)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::OutputIndex)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::BlockNumber)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::Sender)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(ColumnDef::new(StealthOutputs::Fee).big_integer().not_null())
                    .col(
                        ColumnDef::new(StealthOutputs::Timestamp)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::Commitment)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::StealthPublicKey)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::TxPublicKey)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(ColumnDef::new(StealthOutputs::Amount).big_integer().null())
                    .col(
                        ColumnDef::new(StealthOutputs::MemoPlaintext)
                            .string_len(1_024)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::EncryptedMemoCiphertext)
                            .binary()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::EncryptedMemoNonce)
                            .binary_len(12)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::EncryptedMemoMessageNumber)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::OutputCreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StealthOutputs::InsertedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .name("pk_stealth_outputs")
                            .col(StealthOutputs::TxId)
                            .col(StealthOutputs::OutputIndex),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_stealth_outputs_transaction")
                            .from(StealthOutputs::Table, StealthOutputs::TxId)
                            .to(ChainTransactions::Table, ChainTransactions::TxId)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("idx_stealth_outputs_block")
                            .col(StealthOutputs::BlockNumber),
                    )
                    .index(
                        Index::create()
                            .name("idx_stealth_outputs_sender")
                            .col(StealthOutputs::Sender),
                    )
                    .index(
                        Index::create()
                            .name("idx_stealth_outputs_commitment")
                            .unique()
                            .col(StealthOutputs::Commitment),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(StealthOutputs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum StealthOutputs {
    Table,
    TxId,
    OutputIndex,
    BlockNumber,
    Sender,
    Fee,
    Timestamp,
    Commitment,
    StealthPublicKey,
    TxPublicKey,
    Amount,
    MemoPlaintext,
    EncryptedMemoCiphertext,
    EncryptedMemoNonce,
    EncryptedMemoMessageNumber,
    OutputCreatedAt,
    InsertedAt,
}

#[derive(DeriveIden)]
enum ChainTransactions {
    Table,
    TxId,
}
