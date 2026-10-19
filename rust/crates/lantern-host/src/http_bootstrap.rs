//! Explicit opt-in HTTP bootstrap and native-copy reader. No path discovery or writes.
use lantern_host::{http::ReadRegistry, persistence::Store};
use std::{ffi::OsString, net::SocketAddr, path::PathBuf, process::ExitCode};

pub fn run(args: &[OsString]) -> ExitCode {
    serve(args, ReadRegistry::default())
}

pub fn run_native(args: &[OsString]) -> ExitCode {
    if args == [OsString::from("--help")] {
        println!(
            "Usage: lantern-host http-read-copy --experimental --source NATIVE_ROOT --bind LOOPBACK:PORT [--asset-root DIRECTORY]\nRead-only native generation; currently admits uninhabited, versioned chunked maps without NPCs/villages only. No accounts, commands, simulation or writes."
        );
        return ExitCode::SUCCESS;
    }
    if ![5, 7].contains(&args.len())
        || args[0] != "--experimental"
        || args[1] != "--source"
        || args[3] != "--bind"
        || (args.len() == 7 && args[5] != "--asset-root")
    {
        eprintln!("Explicit --experimental --source NATIVE_ROOT --bind LOOPBACK:PORT is required.");
        return ExitCode::FAILURE;
    }
    let store = match Store::open_read_only(&PathBuf::from(&args[2])) {
        Ok(store) => store,
        Err(_) => {
            eprintln!(
                "Native source is unavailable, unsupported or locked. No HTTP listener started."
            );
            return ExitCode::FAILURE;
        }
    };
    let registry = match store.recover() {
        Ok(recovery) if recovery.skipped.is_empty() => recovery.registry,
        _ => {
            eprintln!(
                "Native generation is unsupported or damaged. Use offline inspection/recovery; HTTP will not silently fall back."
            );
            return ExitCode::FAILURE;
        }
    };
    let Some(registry) = registry else {
        eprintln!("Native source has no committed generation.");
        return ExitCode::FAILURE;
    };
    let registry = match ReadRegistry::from_native(registry) {
        Ok(registry) => registry,
        Err(_) => {
            eprintln!(
                "Unsupported HTTP snapshot coverage. Populated worlds, NPCs, villages, adventures and unreviewed payload fields are refused."
            );
            return ExitCode::FAILURE;
        }
    };
    let mut options = vec![
        OsString::from("--experimental-empty"),
        OsString::from("--bind"),
        args[4].clone(),
    ];
    if args.len() == 7 {
        options.extend_from_slice(&args[5..]);
    }
    // Retain the read-only OS lock for the listener's lifetime. No save writer
    // can open this same root, and no writable handles or paths enter HTTP state.
    let result = serve(&options, registry);
    drop(store);
    result
}

fn serve(args: &[OsString], registry: ReadRegistry) -> ExitCode {
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
            "Experimental read-only HTTP listening on {}",
            listener.local_addr().unwrap()
        );
        let app = lantern_host::http::read_router(
            root.join("web"),
            root.join("icons/favicon.ico"),
            registry,
        );
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
