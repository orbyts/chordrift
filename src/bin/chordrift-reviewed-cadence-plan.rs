//! Host-local exact-reorder planner for an approved reviewed cadence.

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
    if arguments.next().is_some() {
        return Err(usage());
    }
    let database = db::connect(config::database_config_from_env()?).await?;
    db::require_schema_through(&database, 51).await?;
    let report = sync_plan::create(&database, &account, None).await?;
    let non_reorder_operations = report.operation_count.saturating_sub(report.reorders);
    if report.operation_count == 0 || non_reorder_operations != 0 {
        return Err(chordrift::ChordriftError::Configuration(format!(
            "reviewed cadence plan must contain only exact reorders; found {} operations including {} non-reorders",
            report.operation_count, non_reorder_operations
        )));
    }
    let proposal_generation_id = report.proposal_generation_id.ok_or_else(|| {
        chordrift::ChordriftError::Configuration(
            "reviewed cadence plan did not retain its approved proposal".to_owned(),
        )
    })?;
    println!("plan_id: {}", report.plan_id);
    println!("proposal_generation_id: {proposal_generation_id}");
    println!("operations: {}", report.operation_count);
    println!("reorders: {}", report.reorders);
    println!("input_hash: {}", report.input_hash);
    println!("reused: {}", report.reused);
    println!("spotify_writes: disabled");
    Ok(())
}

fn usage() -> chordrift::ChordriftError {
    chordrift::ChordriftError::Configuration(
        "usage: chordrift-reviewed-cadence-plan ACCOUNT".to_owned(),
    )
}
