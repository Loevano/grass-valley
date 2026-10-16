//! Offline replay smoke test and read-only copy inspection. No gameplay or HTTP.

mod inspect;
mod replay;

use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "inspect-copy") {
        return inspect::run(&args[1..]);
    }
    if args != [std::ffi::OsString::from("replay")] {
        eprintln!(
            "Usage: lantern-host replay < request.json (transport smoke only; no gameplay)\n       lantern-host inspect-copy --help (read-only synthetic copies)"
        );
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
