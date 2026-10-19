use super::{
    Error, RegistrySnapshot, RestoredRegistry, Result, invalid,
    snapshot::{Bundle, ChunkBlob},
};
use crate::legacy::ChunkKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const MARKER: &str = ".lantern-rust-save";
const MAGIC: &[u8] = b"lantern-rust-generations-v1\n";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Generation(pub u64);
/// Fallback is explicit: skipped generations were incomplete/corrupt, and their
/// progress is absent from registry. Pending directories were never committed.
pub struct Recovery {
    pub generation: Option<Generation>,
    pub registry: Option<RestoredRegistry>,
    pub skipped: Vec<Generation>,
}
/// An exclusively locked, dedicated experimental root. No root discovery.
pub struct Store {
    root: PathBuf,
    _lock: File,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Checkpoint {
    Stage,
    Created(usize),
    Written(usize),
    Synced(usize),
    ManifestWritten,
    ManifestSynced,
    StageSynced,
    Published,
    RootSynced,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u32,
    generation: String,
    files: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    role: Role,
    length: u64,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Role {
    Registry,
    Chunk { world: usize, x: u32, y: u32 },
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
fn private_dir(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    Ok(builder.create(path)?)
}
fn sync_dir(path: &Path) -> Result<()> {
    Ok(File::open(path)?.sync_all()?)
}
fn read_regular(path: &Path) -> Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(invalid("nonregular generation file"));
    }
    Ok(fs::read(path)?)
}
fn number(name: &str, prefix: &str) -> Option<u64> {
    let digits = name.strip_prefix(prefix)?;
    let n: u64 = digits.parse().ok()?;
    (n != 0 && format!("{prefix}{n:020}") == name).then_some(n)
}
impl Store {
    /// Only creates a NEW directory. Existing/Python roots are never adopted.
    pub fn create(root: &Path) -> Result<Self> {
        private_dir(root)?;
        let mut marker = private_file(&root.join(MARKER))?;
        marker.write_all(MAGIC)?;
        marker.sync_all()?;
        sync_dir(root)?;
        sync_dir(
            root.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        Self::open(root)
    }
    /// Reopens only a marked root, and takes an OS lock released on crash/exit.
    pub fn open(root: &Path) -> Result<Self> {
        if !fs::symlink_metadata(root)?.file_type().is_dir() {
            return Err(invalid("root must be a real directory"));
        }
        if read_regular(&root.join(MARKER))? != MAGIC {
            return Err(invalid("not an experimental Rust root"));
        }
        for python in ["savegame.json", "savegame.players.json", "savegame_chunks"] {
            if root.join(python).try_exists()? {
                return Err(invalid("Python data in Rust root"));
            }
        }
        let path = root.join(".writer.lock");
        if let Ok(meta) = fs::symlink_metadata(&path)
            && !meta.file_type().is_file()
        {
            return Err(invalid("nonregular writer lock"));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(path)?;
        lock.try_lock().map_err(|e| match e {
            fs::TryLockError::WouldBlock => Error::Busy,
            fs::TryLockError::Error(e) => e.into(),
        })?;
        let result = Self {
            root: fs::canonicalize(root)?,
            _lock: lock,
        };
        result.generations()?;
        Ok(result)
    }
    fn generations(&self) -> Result<Vec<u64>> {
        let mut generations = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| invalid("root entry"))?;
            if name == MARKER || name == ".writer.lock" {
                continue;
            }
            if let Some(n) = number(name, "generation-") {
                if !entry.file_type()?.is_dir() {
                    return Err(invalid("non-directory generation"));
                }
                generations.push(n);
            } else if number(name, "pending-").is_none() {
                return Err(invalid("unexpected root entry"));
            }
        }
        generations.sort_unstable();
        Ok(generations)
    }
    fn next(&self) -> Result<u64> {
        let mut high = 0;
        for entry in fs::read_dir(&self.root)? {
            let name = entry?.file_name();
            let name = name.to_str().ok_or_else(|| invalid("root entry"))?;
            if let Some(n) = number(name, "generation-").or_else(|| number(name, "pending-")) {
                high = high.max(n);
            }
        }
        high.checked_add(1)
            .ok_or_else(|| invalid("generation counter exhausted"))
    }
    pub fn commit(&mut self, snapshot: RegistrySnapshot) -> Result<Generation> {
        self.commit_inner(snapshot, |_| Ok(()))
    }
    pub(super) fn commit_inner(
        &mut self,
        snapshot: RegistrySnapshot,
        mut checkpoint: impl FnMut(Checkpoint) -> Result<()>,
    ) -> Result<Generation> {
        let bundle = snapshot.encode()?;
        let n = self.next()?;
        let stage = self.root.join(format!("pending-{n:020}"));
        private_dir(&stage)?;
        checkpoint(Checkpoint::Stage)?;
        let mut entries = Vec::new();
        let files = std::iter::once((Role::Registry, bundle.registry)).chain(
            bundle.chunks.into_iter().map(|c| {
                (
                    Role::Chunk {
                        world: c.world,
                        x: c.key.x,
                        y: c.key.y,
                    },
                    c.bytes,
                )
            }),
        );
        for (index, (role, bytes)) in files.enumerate() {
            let mut file = private_file(&stage.join(format!("{index:08}.bin")))?;
            checkpoint(Checkpoint::Created(index))?;
            file.write_all(&bytes)?;
            checkpoint(Checkpoint::Written(index))?;
            file.sync_all()?;
            checkpoint(Checkpoint::Synced(index))?;
            entries.push(Entry {
                role,
                length: bytes.len() as u64,
                sha256: digest(&bytes),
            });
        }
        let manifest = Manifest {
            format: "lantern-generation".into(),
            version: 1,
            generation: n.to_string(),
            files: entries,
        };
        let bytes = serde_json::to_vec(&manifest).map_err(|_| invalid("manifest encoding"))?;
        let mut file = private_file(&stage.join("manifest.json"))?;
        file.write_all(&bytes)?;
        checkpoint(Checkpoint::ManifestWritten)?;
        file.sync_all()?;
        checkpoint(Checkpoint::ManifestSynced)?;
        sync_dir(&stage)?;
        checkpoint(Checkpoint::StageSynced)?;
        fs::rename(&stage, self.root.join(format!("generation-{n:020}")))?;
        checkpoint(Checkpoint::Published)?;
        sync_dir(&self.root)?;
        checkpoint(Checkpoint::RootSynced)?;
        Ok(Generation(n))
    }
    pub fn recover(&self) -> Result<Recovery> {
        let mut skipped = Vec::new();
        for n in self.generations()?.into_iter().rev() {
            match self.load(n) {
                Ok(registry) => {
                    return Ok(Recovery {
                        generation: Some(Generation(n)),
                        registry: Some(registry),
                        skipped,
                    });
                }
                Err(_) => skipped.push(Generation(n)),
            }
        }
        if skipped.is_empty() {
            Ok(Recovery {
                generation: None,
                registry: None,
                skipped,
            })
        } else {
            Err(invalid("no usable committed generation"))
        }
    }
    fn load(&self, n: u64) -> Result<RestoredRegistry> {
        let root = self.root.join(format!("generation-{n:020}"));
        let manifest: Manifest =
            serde_json::from_slice(&read_regular(&root.join("manifest.json"))?)
                .map_err(|_| invalid("manifest schema"))?;
        if manifest.format != "lantern-generation"
            || manifest.version != 1
            || manifest.generation != n.to_string()
        {
            return Err(invalid("manifest version or generation"));
        }
        let mut expected = BTreeSet::from(["manifest.json".to_owned()]);
        let mut registry = None;
        let mut chunks = Vec::new();
        for (i, e) in manifest.files.into_iter().enumerate() {
            let name = format!("{i:08}.bin");
            expected.insert(name.clone());
            let bytes = read_regular(&root.join(name))?;
            if bytes.len() as u64 != e.length || digest(&bytes) != e.sha256 {
                return Err(invalid("generation integrity"));
            }
            match e.role {
                Role::Registry if i == 0 && registry.is_none() => registry = Some(bytes),
                Role::Registry => return Err(invalid("duplicate or unordered registry")),
                Role::Chunk { world, x, y } => chunks.push(ChunkBlob {
                    world,
                    key: ChunkKey { x, y },
                    bytes,
                }),
            }
        }
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let name = entry.file_name();
            if !name.to_str().is_some_and(|name| expected.remove(name)) {
                return Err(invalid("unexpected generation file"));
            }
        }
        if !expected.is_empty() {
            return Err(invalid("missing generation file"));
        }
        Bundle {
            registry: registry.ok_or_else(|| invalid("missing registry"))?,
            chunks,
        }
        .restore()
    }
}
