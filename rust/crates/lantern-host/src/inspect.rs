//! Explicit copy inspection only. No discovery, source writes or simulation.
use lantern_core::ids::WorldId;
use lantern_host::legacy::{ChunkDirectory, ChunkKey, read_copies_with_chunks};
use std::{
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

const USAGE: &str = "Usage: lantern-host inspect-copy --source FILE [--provision FILE] [--chunks WORLD SEED_DIRECTORY]... [--require-chunk WORLD X Y]...\nReads explicit synthetic save copies only; preserves unported data privately. No discovery, migrations, writes, generation or simulation. Successful inspection does not mean playable conversion is supported.";

struct Binding {
    world: WorldId,
    directory: PathBuf,
    required: Vec<ChunkKey>,
}
struct Options {
    source: PathBuf,
    provision: Option<PathBuf>,
    chunks: Vec<Binding>,
}

fn parse(args: &[OsString]) -> Result<Options, &'static str> {
    let mut source = None;
    let mut provision = None;
    let mut chunks = vec![];
    let mut required = vec![];
    let mut index = 0;
    while index < args.len() {
        let count = match args[index].to_str() {
            Some("--source" | "--provision") => 1,
            Some("--chunks") => 2,
            Some("--require-chunk") => 3,
            _ => return Err("Unknown inspection option."),
        };
        if args.len() - index <= count {
            return Err("Missing inspection option value.");
        }
        match args[index].to_str().unwrap() {
            "--source" => {
                if source.replace(PathBuf::from(&args[index + 1])).is_some() {
                    return Err("Duplicate source option.");
                }
            }
            "--provision" => {
                if provision.replace(PathBuf::from(&args[index + 1])).is_some() {
                    return Err("Duplicate provisioning option.");
                }
            }
            "--chunks" | "--require-chunk" => {
                let world = args[index + 1].to_str().ok_or("Invalid world owner.")?;
                let world = WorldId::new(world).map_err(|_| "Invalid world owner.")?;
                if count == 2 {
                    chunks.push(Binding {
                        world,
                        directory: PathBuf::from(&args[index + 2]),
                        required: vec![],
                    });
                } else {
                    let coordinate = |offset: usize| {
                        args[index + offset]
                            .to_str()
                            .and_then(|s| s.parse::<u32>().ok())
                            .ok_or("Invalid required chunk key.")
                    };
                    required.push((
                        world,
                        ChunkKey {
                            x: coordinate(2)?,
                            y: coordinate(3)?,
                        },
                    ));
                }
            }
            _ => unreachable!(),
        }
        index += count + 1;
    }
    for (world, key) in required {
        let binding = chunks
            .iter_mut()
            .find(|b| b.world == world)
            .ok_or("Required chunk has no explicit world directory.")?;
        binding.required.push(key);
    }
    Ok(Options {
        source: source.ok_or("An explicit source copy is required.")?,
        provision,
        chunks,
    })
}

pub fn run(args: &[OsString]) -> ExitCode {
    if args == [OsString::from("--help")] {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let options = match parse(args) {
        Ok(options) => options,
        Err(reason) => {
            eprintln!("{reason}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    // The library's missing-save => empty-registry behavior is useful to future
    // hosts; an inspection typo must instead fail without claiming a valid copy.
    if !options.source.is_file() {
        eprintln!("Inspection source copy is missing or is not a regular file.");
        return ExitCode::FAILURE;
    }
    let directories: Vec<_> = options
        .chunks
        .iter()
        .map(|binding| {
            ChunkDirectory::new(&binding.world, &binding.directory).requiring(&binding.required)
        })
        .collect();
    match read_copies_with_chunks(&options.source, options.provision.as_deref(), &directories) {
        Ok(imported) => {
            let mut output = io::stdout().lock();
            if serde_json::to_writer_pretty(&mut output, &imported.report()).is_err()
                || writeln!(output).is_err()
            {
                eprintln!("Unable to write inspection report.");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            // BoundaryError only contains audited static categories/schema paths.
            eprintln!("Inspection failed: {error:?}");
            ExitCode::FAILURE
        }
    }
}
