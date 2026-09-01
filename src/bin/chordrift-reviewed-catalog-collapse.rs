//! Host-local planner for one reviewed catalog-edition collapse.

use std::{env, process::ExitCode};

use chordrift::{config, db, sync_plan};

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
    let playlist = arguments.next().ok_or_else(usage)?;
    let required = arguments
        .next()
        .ok_or_else(usage)?
        .split(',')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let removable = arguments
        .next()
        .ok_or_else(usage)?
        .split(',')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if arguments.next().is_some() {
        return Err(usage());
    }
    let database = db::connect(config::database_config_from_env()?).await?;
    db::require_schema_through(&database, 51).await?;
    let report = sync_plan::create_reviewed_catalog_collapse_plan(
        &database, &account, &playlist, &required, &removable,
    )
    .await?;
    if report.operation_count != 1 || report.reorders != 1 {
        return Err(chordrift::ChordriftError::Configuration(
            "catalog collapse did not produce exactly one replacement order".to_owned(),
        ));
    }
    println!("plan_id: {}", report.plan_id);
    println!("playlist: {playlist}");
    println!("required_editions: {}", required.len());
    println!("removable_editions: {}", removable.len());
    println!("input_hash: {}", report.input_hash);
    println!("spotify_writes: disabled");
    Ok(())
}

fn usage() -> chordrift::ChordriftError {
    chordrift::ChordriftError::Configuration(
        "usage: chordrift-reviewed-catalog-collapse ACCOUNT PLAYLIST REQUIRED_IDS_CSV REMOVABLE_IDS_CSV"
            .to_owned(),
    )
}
