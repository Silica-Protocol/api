pub use sea_orm_migration::prelude::*;

mod m20251113_000001_create_base_tables;
mod m20251113_000002_add_identity_tables;
mod m20251113_000003_add_stealth_outputs;
mod m20251113_000004_add_governance_tables;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20251113_000001_create_base_tables::Migration),
            Box::new(m20251113_000002_add_identity_tables::Migration),
            Box::new(m20251113_000003_add_stealth_outputs::Migration),
            Box::new(m20251113_000004_add_governance_tables::Migration),
        ]
    }
}
