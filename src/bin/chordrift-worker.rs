use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match chordrift::hosted_worker::run_from_env().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            if let Some(diagnostic) = error.safe_diagnostic() {
                eprintln!("diagnostic: {diagnostic}");
            }
            ExitCode::FAILURE
        }
    }
}
