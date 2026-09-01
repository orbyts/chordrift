//! Host-local read-only catalog audit for an explicit migration cohort.

use std::{env, fs, process::ExitCode};

use chordrift::{config, db, operator_catalog};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> chordrift::Result<()> {
    let mut arguments = env::args().skip(1);
    let account = arguments.next().ok_or_else(usage)?;
    let input = arguments.next().ok_or_else(usage)?;
    let output = arguments.next().ok_or_else(usage)?;
    if arguments.next().is_some() {
        return Err(usage());
    }
    let track_ids: Vec<String> = serde_json::from_slice(&fs::read(input)?)?;
    let database = db::connect(config::database_config_from_env()?).await?;
    db::require_schema_through(&database, 51).await?;
    let result =
        operator_catalog::resolve_hosted_catalog_tracks(&database, &account, &track_ids).await?;
    fs::write(output, serde_json::to_vec_pretty(&result)?)?;
    Ok(())
}

fn usage() -> chordrift::ChordriftError {
    chordrift::ChordriftError::Configuration(
        "usage: chordrift-catalog-audit ACCOUNT INPUT_IDS_JSON OUTPUT_JSON".to_owned(),
    )
}
