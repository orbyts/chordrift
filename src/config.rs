use storexa::{DatabaseConfig, PostgresProvider};

use crate::Result;

/// Environment variable containing Chordrift's canonical Neon database URL.
pub const DATABASE_URL_VARIABLE: &str = "CHORDRIFT_DATABASE_URL";

/// Builds Storexa configuration for Chordrift's canonical database.
pub fn database_config_from_env() -> Result<DatabaseConfig> {
    configure(DatabaseConfig::from_env_var(DATABASE_URL_VARIABLE)?)
}

fn configure(config: DatabaseConfig) -> Result<DatabaseConfig> {
    Ok(config
        .with_name("chordrift-primary")?
        .with_provider(PostgresProvider::Neon)?
        .with_min_connections(0)
        .with_max_connections(5))
}

#[cfg(test)]
mod tests {
    use storexa::{DatabaseConfig, PostgresProvider};

    use super::configure;

    #[test]
    fn identifies_the_canonical_neon_pool_without_leaking_its_url() {
        let config = configure(
            DatabaseConfig::from_url("postgresql://listener:secret@example.com/chordrift")
                .expect("valid PostgreSQL URL"),
        )
        .expect("valid Chordrift configuration");

        assert_eq!(config.metadata().name(), "chordrift-primary");
        assert_eq!(config.metadata().provider(), &PostgresProvider::Neon);

        let debug = format!("{config:?}");
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("example.com"));
    }
}
