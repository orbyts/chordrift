use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let result = if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        chordrift::hosted::healthcheck_from_env().await
    } else {
        chordrift::hosted::run_from_env().await
    };
    match result {
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
