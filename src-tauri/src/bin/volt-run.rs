//! The collection runner, for a terminal and for CI.
//!
//! Everything it does lives in `volt_lib::runner`, so a run here is the same
//! run the app does.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    volt_lib::runner::cli(std::env::args().skip(1).collect()).await
}
