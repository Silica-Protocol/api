use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(IdentityProfiles::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(IdentityProfiles::IdentityId)
                            .binary_len(32)
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(IdentityProfiles::DisplayName)
                            .string_len(64)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(IdentityProfiles::DisplayNameSearch)
                            .string_len(64)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(IdentityProfiles::AvatarHash)
                            .binary_len(32)
                            .null(),
                    )
                    .col(ColumnDef::new(IdentityProfiles::Bio).string_len(512).null())
                    .col(
                        ColumnDef::new(IdentityProfiles::StatsVisibility)
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(IdentityProfiles::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(IdentityProfiles::UpdatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(IdentityProfiles::LastSyncedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(IdentityProfiles::ProfileVersion)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_identity_profiles_display_name")
                    .table(IdentityProfiles::Table)
                    .col(IdentityProfiles::DisplayNameSearch)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(WalletLinks::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WalletLinks::IdentityId)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WalletLinks::WalletAddress)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WalletLinks::LinkType)
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WalletLinks::ProofSignature)
                            .binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WalletLinks::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WalletLinks::VerifiedAt).big_integer().null())
                    .col(
                        ColumnDef::new(WalletLinks::LastSyncedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .name("pk_wallet_links")
                            .col(WalletLinks::IdentityId)
                            .col(WalletLinks::WalletAddress),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_wallet_links_identity")
                            .from(WalletLinks::Table, WalletLinks::IdentityId)
                            .to(IdentityProfiles::Table, IdentityProfiles::IdentityId)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_wallet_links_address")
                    .table(WalletLinks::Table)
                    .col(WalletLinks::WalletAddress)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WalletLinks::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(IdentityProfiles::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum IdentityProfiles {
    Table,
    IdentityId,
    DisplayName,
    DisplayNameSearch,
    AvatarHash,
    Bio,
    StatsVisibility,
    CreatedAt,
    UpdatedAt,
    LastSyncedBlock,
    ProfileVersion,
}

#[derive(DeriveIden)]
enum WalletLinks {
    Table,
    IdentityId,
    WalletAddress,
    LinkType,
    ProofSignature,
    CreatedAt,
    VerifiedAt,
    LastSyncedBlock,
}
