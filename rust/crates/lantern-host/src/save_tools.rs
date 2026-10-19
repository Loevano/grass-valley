//! Offline explicit copy/recovery commands. Never print paths, IDs or credentials.
use lantern_core::ids::WorldId;
use lantern_host::{
    legacy::ChunkKey,
    persistence::{Generation, Store, tools},
};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};
const USAGE: &str = "Usage:
  lantern-host save-convert --source-format native --source ROOT --destination NEW_ROOT
  lantern-host save-convert --source-format python-archive --source FILE --destination NEW_ARCHIVE [--provision FILE] [--chunks WORLD SEED_DIRECTORY]... [--require-chunk WORLD X Y]...
  lantern-host save-inspect --source ROOT [--source-format native|python-archive]
  lantern-host save-recover --source ROOT --destination NEW_ROOT [--generation N]
  lantern-host save-restore-python --source ARCHIVE --destination NEW_PYTHON_ROOT
  lantern-host save-cleanup --source ROOT --pending-only
Native copy/recovery validates the complete generation and never changes its source.
Python archives are preserved-only, never playable native imports. Archive restore
exports the original Python bytes only. Native gameplay reverse export is unsupported.
Cleanup explicitly deletes only uncommitted pending directories.";
struct Options {
    source: PathBuf,
    destination: Option<PathBuf>,
    generation: Option<Generation>,
    format: Option<String>,
    pending_only: bool,
    provision: Option<PathBuf>,
    chunks: Vec<tools::python::ChunkSource>,
}
fn parse(command: &str, args: &[OsString]) -> Result<Options, &'static str> {
    let mut source = None;
    let mut destination = None;
    let mut generation = None;
    let mut format = None;
    let mut pending_only = false;
    let mut provision = None;
    let mut chunks: Vec<tools::python::ChunkSource> = vec![];
    let mut required = vec![];
    let mut index = 0;
    while index < args.len() {
        let key = args[index].to_str().ok_or("Invalid option.")?;
        if key == "--pending-only" {
            if pending_only || command != "save-cleanup" {
                return Err("Unexpected or duplicate option.");
            }
            pending_only = true;
            index += 1;
            continue;
        }
        if ["--chunks", "--require-chunk"].contains(&key) && command == "save-convert" {
            let count = if key == "--chunks" { 2 } else { 3 };
            if args.len() <= index + count {
                return Err("Missing chunk option value.");
            }
            let world = WorldId::new(args[index + 1].to_str().ok_or("Invalid world binding.")?)
                .map_err(|_| "Invalid world binding.")?;
            if count == 2 {
                chunks.push(tools::python::ChunkSource {
                    world,
                    directory: PathBuf::from(&args[index + 2]),
                    required: vec![],
                });
            } else {
                let coordinate = |offset: usize| {
                    args[index + offset]
                        .to_str()
                        .and_then(|s| s.parse::<u32>().ok())
                        .ok_or("Invalid chunk key.")
                };
                required.push((
                    world,
                    ChunkKey {
                        x: coordinate(2)?,
                        y: coordinate(3)?,
                    },
                ));
            }
            index += count + 1;
            continue;
        }
        if args.len() <= index + 1 {
            return Err("Missing option value.");
        }
        let value = &args[index + 1];
        match key {
            "--source" if source.is_none() => source = Some(PathBuf::from(value)),
            "--destination"
                if destination.is_none()
                    && ["save-convert", "save-recover", "save-restore-python"]
                        .contains(&command) =>
            {
                destination = Some(PathBuf::from(value))
            }
            "--source-format"
                if format.is_none() && ["save-convert", "save-inspect"].contains(&command) =>
            {
                format = Some(value.to_str().ok_or("Invalid source format.")?.to_owned())
            }
            "--provision" if provision.is_none() && command == "save-convert" => {
                provision = Some(PathBuf::from(value))
            }
            "--generation" if generation.is_none() && command == "save-recover" => {
                let text = value.to_str().ok_or("Invalid generation.")?;
                let n: u64 = text.parse().map_err(|_| "Invalid generation.")?;
                if n == 0 || n.to_string() != text {
                    return Err("Invalid generation.");
                }
                generation = Some(Generation(n));
            }
            _ => return Err("Unexpected or duplicate option."),
        }
        index += 2;
    }
    if ["save-convert", "save-recover", "save-restore-python"].contains(&command)
        && destination.is_none()
    {
        return Err("An explicit new destination is required.");
    }
    if command == "save-convert" && format.is_none() {
        return Err("An explicit source format is required.");
    }
    if command == "save-cleanup" && !pending_only {
        return Err("Cleanup requires --pending-only.");
    }
    for (world, key) in required {
        chunks
            .iter_mut()
            .find(|c| c.world == world)
            .ok_or("Required chunk has no explicit binding.")?
            .required
            .push(key);
    }
    if (provision.is_some() || !chunks.is_empty()) && format.as_deref() != Some("python-archive") {
        return Err("Chunk/provision paths require explicit python-archive format.");
    }
    Ok(Options {
        source: source.ok_or("An explicit source is required.")?,
        destination,
        generation,
        format,
        pending_only,
        provision,
        chunks,
    })
}
pub fn run(command: &str, args: &[OsString]) -> ExitCode {
    if args == [OsString::from("--help")] {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if command == "save-export" {
        eprintln!(
            "Reverse export of native Rust gameplay to Python is unsupported. Reopening the original Python backup loses all subsequent Rust progress."
        );
        return ExitCode::from(2);
    }
    let options = match parse(command, args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    if options
        .format
        .as_ref()
        .is_some_and(|f| !["native", "python-archive"].contains(&f.as_str()))
    {
        eprintln!(
            "Unsupported source format. Python staging is preserved-only; complete playable import adapters are unavailable. No destination was created."
        );
        return ExitCode::from(2);
    }
    match execute(command, options) {
        Ok(report) => {
            let mut output = io::stdout().lock();
            if serde_json::to_writer(&mut output, &report).is_err() || writeln!(output).is_err() {
                eprintln!("Unable to write save-tool report.");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(_) => {
            eprintln!(
                "Save operation failed: source busy, incomplete, invalid, unsupported or unreadable; destination must be new and disjoint. Source data was not changed by copy/recovery."
            );
            ExitCode::FAILURE
        }
    }
}
fn execute(command: &str, o: Options) -> Result<Value, lantern_host::persistence::Error> {
    if (command == "save-convert" || command == "save-inspect")
        && o.format.as_deref() == Some("python-archive")
        || command == "save-restore-python"
    {
        let report = match command {
            "save-convert" => tools::python::archive(
                &o.source,
                o.provision.as_deref(),
                &o.chunks,
                o.destination
                    .as_deref()
                    .expect("parser requires destination"),
            )?,
            "save-inspect" => tools::python::inspect(&o.source)?,
            "save-restore-python" => tools::python::restore(
                &o.source,
                o.destination
                    .as_deref()
                    .expect("parser requires destination"),
            )?,
            _ => unreachable!(),
        };
        return Ok(
            json!({"operation":command,"format":"python-archive","readiness":"preserved-only","worlds":report.worlds,"chunks":report.chunks,"effective_accounts":report.accounts,"provisioning_present":report.provisioning_present,"source_sha256":report.source_sha256,"source_changed":false,"playable_native_import":false,"reverse_export_scope":"original-python-archive-only"}),
        );
    }
    match command {
        "save-convert" | "save-recover" => {
            let destination = o
                .destination
                .as_deref()
                .expect("parser requires destination");
            let report = if command == "save-convert" {
                tools::convert_native(&o.source, destination)?
            } else {
                tools::recover_native(&o.source, destination, o.generation)?
            };
            Ok(
                json!({"operation":command,"source_generation":report.source_generation.0,"destination_generation":report.destination_generation.0,"skipped_generations":report.skipped.iter().map(|g|g.0).collect::<Vec<_>>(),"source_manifest_sha256":report.source_manifest_sha256,"source_changed":false,"python_reverse_export":false}),
            )
        }
        "save-inspect" => {
            let store = Store::open_read_only(&o.source)?;
            let generations = store.inspect()?;
            let selected = store.recover().ok().and_then(|r| r.generation).map(|g| g.0);
            Ok(
                json!({"operation":command,"generations":generations.into_iter().map(|g|json!({"generation":g.generation.0,"valid":g.valid})).collect::<Vec<_>>(),"selected_generation":selected,"source_changed":false,"python_reverse_export":false}),
            )
        }
        "save-cleanup" if o.pending_only => Ok(
            json!({"operation":command,"removed_pending":tools::cleanup_pending(&o.source)?,"committed_generations_removed":0}),
        ),
        _ => Err(lantern_host::persistence::Error::Invalid(
            "unsupported tool",
        )),
    }
}
