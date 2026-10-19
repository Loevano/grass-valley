//! Explicit opt-in no-world HTTP bootstrap. Never discovers or opens saves.
use std::{ffi::OsString, net::SocketAddr, path::PathBuf, process::ExitCode};

pub fn run(args: &[OsString]) -> ExitCode {
    if args == [OsString::from("--help")] {
        println!(
            "Usage: lantern-host http-bootstrap --experimental-empty --bind LOOPBACK:PORT [--asset-root DIRECTORY]\nStatic assets and no-active-world snapshots only. No saves, accounts, commands, ticks or world administration. Python remains the playable host."
        );
        return ExitCode::SUCCESS;
    }
    if ![3, 5].contains(&args.len())
        || args[0] != "--experimental-empty"
        || args[1] != "--bind"
        || (args.len() == 5 && args[3] != "--asset-root")
    {
        eprintln!(
            "An explicit --experimental-empty --bind LOOPBACK:PORT is required. Save loading and active-world HTTP coverage are unsupported."
        );
        return ExitCode::FAILURE;
    }
    let Some(address) = args[2]
        .to_str()
        .and_then(|s| s.parse::<SocketAddr>().ok())
        .filter(|a| a.ip().is_loopback())
    else {
        eprintln!("The experimental bootstrap requires a loopback socket address.");
        return ExitCode::FAILURE;
    };
    let root = if args.len() == 5 {
        PathBuf::from(&args[4])
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
    };
    if !root.join("web/index.html").is_file() || !root.join("icons/favicon.ico").is_file() {
        eprintln!("Asset root must contain web/index.html and icons/favicon.ico.");
        return ExitCode::FAILURE;
    }
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("Unable to start HTTP runtime.");
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(async {
        let listener = match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => listener,
            Err(_) => {
                eprintln!("Unable to bind experimental HTTP listener.");
                return ExitCode::FAILURE;
            }
        };
        println!(
            "Experimental no-world HTTP listening on {}",
            listener.local_addr().unwrap()
        );
        let app =
            lantern_host::http::bootstrap_router(root.join("web"), root.join("icons/favicon.ico"));
        match axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => {
                eprintln!("Experimental HTTP listener failed.");
                ExitCode::FAILURE
            }
        }
    })
}
