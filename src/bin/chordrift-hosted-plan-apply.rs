//! Host-local hosted-vault executor for an already-reviewed sync plan.

use std::{env, process::ExitCode};

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
    let plan_id = arguments
        .next()
        .ok_or_else(usage)?
        .parse()
        .map_err(|_| usage())?;
    if arguments.next().is_some() {
        return Err(usage());
    }
    let report =
        chordrift::hosted_worker::apply_reviewed_sync_plan_from_env(&account, plan_id).await?;
    println!("apply_run_id: {}", report.apply_run_id);
    println!("plan_id: {}", report.plan_id);
    println!("assessment_id: {}", report.assessment_id);
    println!("status: {}", report.status);
    println!("operations: {}", report.operation_count);
    println!("succeeded: {}", report.succeeded_count);
    println!("failed: {}", report.failed_count);
    Ok(())
}

fn usage() -> chordrift::ChordriftError {
    chordrift::ChordriftError::Configuration(
        "usage: chordrift-hosted-plan-apply ACCOUNT PLAN_ID".to_owned(),
    )
}
