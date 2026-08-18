use std::io::{self, Write};

use clap::{Parser, Subcommand};

use crate::{Result, config, db};

/// Chordrift command-line interface.
#[derive(Clone, Debug, Parser)]
#[command(name = "chordrift", version, about)]
pub struct Cli {
    /// Operation to perform.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level Chordrift commands.
#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Inspect or migrate the canonical database.
    Db {
        /// Database operation to perform.
        #[command(subcommand)]
        command: DbCommand,
    },
}

/// Canonical database commands.
#[derive(Clone, Debug, Subcommand)]
pub enum DbCommand {
    /// Check connectivity and report migration state without changing it.
    Status,
    /// Apply pending Chordrift schema migrations.
    Migrate,
}

/// Runs a parsed CLI command and writes its report to standard output.
pub async fn run(cli: Cli) -> Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    run_with_writer(cli, &mut output).await
}

async fn run_with_writer(cli: Cli, output: &mut impl Write) -> Result<()> {
    match cli.command {
        Command::Db { command } => {
            let config = config::database_config_from_env()?;
            let database = db::connect(config).await?;
            let result = run_db_command(command, output, &database).await;
            database.close().await;
            result?;
        }
    }

    Ok(())
}

async fn run_db_command(
    command: DbCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    if matches!(command, DbCommand::Migrate) {
        let report = db::migrate(database).await?;
        writeln!(
            output,
            "migrations: {} available migration(s) current in {} ms",
            report.available,
            report.elapsed.as_millis()
        )?;
    }

    let status = db::status(database).await?;
    write_status(output, &status)
}

fn write_status(output: &mut impl Write, status: &db::DatabaseStatus) -> Result<()> {
    writeln!(output, "database: chordrift-primary")?;
    writeln!(output, "provider: neon")?;
    writeln!(output, "status: healthy")?;
    writeln!(output, "server: {}", status.server_version)?;
    writeln!(output, "latency_ms: {}", status.latency.as_millis())?;
    writeln!(
        output,
        "migrations: {}/{} applied, {} pending, {} failed",
        status.applied_migrations,
        status.available_migrations,
        status.pending_migrations,
        status.failed_migrations
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use clap::Parser;

    use super::{Cli, Command, DbCommand, write_status};
    use crate::db::DatabaseStatus;

    #[test]
    fn parses_database_status() {
        let cli = Cli::try_parse_from(["chordrift", "db", "status"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Db {
                command: DbCommand::Status
            }
        ));
    }

    #[test]
    fn renders_secret_free_status() {
        let mut output = Vec::new();
        write_status(
            &mut output,
            &DatabaseStatus {
                server_version: "18.1".to_owned(),
                latency: Duration::from_millis(12),
                available_migrations: 1,
                applied_migrations: 0,
                pending_migrations: 1,
                failed_migrations: 0,
            },
        )
        .expect("writable buffer");

        let output = String::from_utf8(output).expect("UTF-8 output");
        assert!(output.contains("status: healthy"));
        assert!(output.contains("migrations: 0/1 applied, 1 pending, 0 failed"));
        assert!(!output.contains("postgresql://"));
    }
}
