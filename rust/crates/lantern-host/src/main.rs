//! Offline replay transport smoke test. No gameplay, saves, or HTTP listener.

mod replay;

use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args != ["replay"] {
        eprintln!("Usage: lantern-host replay < request.json (transport smoke only; no gameplay)");
        return ExitCode::FAILURE;
    }
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("Unable to read replay request.");
        return ExitCode::FAILURE;
    }
    match replay::run(&input) {
        Ok(response) => {
            let mut output = io::stdout().lock();
            if serde_json::to_writer(&mut output, &response).is_err() || writeln!(output).is_err() {
                eprintln!("Unable to write replay response.");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(replay::Error::Invalid) => {
            eprintln!("Invalid replay protocol v1 request.");
            ExitCode::FAILURE
        }
        Err(replay::Error::Unsupported) => {
            eprintln!(
                "Unsupported replay workload: only transport_smoke_v1 with zero-time movement advances and no random draws is implemented; gameplay is not ported."
            );
            ExitCode::from(2)
        }
    }
}
