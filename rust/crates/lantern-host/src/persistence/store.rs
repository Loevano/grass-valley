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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationInfo {
    pub generation: Generation,
    pub valid: bool,
}
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
    read_only: bool,
    converting: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<SourceProvenance>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceProvenance {
    pub operation: String,
    pub generation: u64,
    pub manifest_sha256: String,
}
impl SourceProvenance {
    fn validate(&self) -> Result<()> {
        if !["copy_native", "recover_native"].contains(&self.operation.as_str())
            || self.generation == 0
            || self.manifest_sha256.len() != 64
            || !self
                .manifest_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("manifest provenance"));
        }
        Ok(())
    }
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
pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
pub(super) fn private_dir(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    Ok(builder.create(path)?)
}
pub(super) fn sync_dir(path: &Path) -> Result<()> {
    Ok(File::open(path)?.sync_all()?)
}
pub(super) fn read_regular(path: &Path) -> Result<Vec<u8>> {
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
        Self::create_inner(root, false)
    }
    pub(super) fn create_copy(root: &Path) -> Result<Self> {
        Self::create_inner(root, true)
    }
    fn create_inner(root: &Path, converting: bool) -> Result<Self> {
        private_dir(root)?;
        if converting {
            let mut gate = private_file(&root.join(".conversion-in-progress"))?;
            gate.write_all(b"lantern-copy-v1\n")?;
            gate.sync_all()?;
            sync_dir(root)?;
        }
        let mut marker = private_file(&root.join(MARKER))?;
        marker.write_all(MAGIC)?;
        marker.sync_all()?;
        sync_dir(root)?;
        sync_dir(
            root.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        Self::open_inner(root, false, converting)
    }
    /// Reopens only a marked root, and takes an OS lock released on crash/exit.
    pub fn open(root: &Path) -> Result<Self> {
        Self::open_inner(root, false, false)
    }
    /// Tooling source access: no file creation, truncation, or writable handles.
    /// The existing OS lock still excludes a live writer during inspection/copy.
    pub fn open_read_only(root: &Path) -> Result<Self> {
        Self::open_inner(root, true, false)
    }
    fn open_inner(root: &Path, read_only: bool, converting: bool) -> Result<Self> {
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
        if !converting && root.join(".conversion-in-progress").try_exists()? {
            return Err(invalid("incomplete conversion"));
        }
        let path = root.join(".writer.lock");
        if let Ok(meta) = fs::symlink_metadata(&path)
            && !meta.file_type().is_file()
        {
            return Err(invalid("nonregular writer lock"));
        }
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(!read_only)
            .create(!read_only)
            .truncate(false);
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
            read_only,
            converting,
        };
        result.generations()?;
        Ok(result)
    }
    pub(super) fn generations(&self) -> Result<Vec<u64>> {
        let mut generations = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| invalid("root entry"))?;
            if name == MARKER
                || name == ".writer.lock"
                || (self.converting && name == ".conversion-in-progress")
            {
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
        if self.read_only {
            return Err(invalid("read-only save source"));
        }
        let bundle = snapshot.encode()?;
        self.commit_bundle(bundle, &mut checkpoint)
    }
    pub(super) fn commit_bundle(
        &mut self,
        bundle: Bundle,
        mut checkpoint: impl FnMut(Checkpoint) -> Result<()>,
    ) -> Result<Generation> {
        if self.read_only {
            return Err(invalid("read-only save source"));
        }
        self.commit_bundle_from(bundle, None, &mut checkpoint)
    }
    pub(super) fn commit_bundle_from(
        &mut self,
        bundle: Bundle,
        source: Option<SourceProvenance>,
        mut checkpoint: impl FnMut(Checkpoint) -> Result<()>,
    ) -> Result<Generation> {
        if self.read_only {
            return Err(invalid("read-only save source"));
        }
        if let Some(origin) = &source {
            origin.validate()?;
        }
        bundle.restore()?;
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
            version: if source.is_some() { 2 } else { 1 },
            generation: n.to_string(),
            files: entries,
            source,
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
    pub(super) fn finish_copy(&mut self) -> Result<()> {
        if !self.converting || self.read_only {
            return Err(invalid("not a conversion destination"));
        }
        if self.recover()?.generation.is_none() {
            return Err(invalid("no committed conversion"));
        }
        fs::remove_file(self.root.join(".conversion-in-progress"))?;
        sync_dir(&self.root)?;
        self.converting = false;
        Ok(())
    }
    pub(super) fn manifest_digest(&self, generation: Generation) -> Result<String> {
        Ok(digest(&read_regular(&self.root.join(format!(
            "generation-{:020}/manifest.json",
            generation.0
        )))?))
    }
    pub(super) fn root(&self) -> &Path {
        &self.root
    }
    pub fn inspect(&self) -> Result<Vec<GenerationInfo>> {
        self.generations()?
            .into_iter()
            .map(|n| {
                Ok(GenerationInfo {
                    generation: Generation(n),
                    valid: self.load(n).is_ok(),
                })
            })
            .collect()
    }
    pub fn recover_generation(&self, generation: Generation) -> Result<RestoredRegistry> {
        if generation.0 == 0 {
            return Err(invalid("invalid generation"));
        }
        self.load(generation.0)
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
        self.read_bundle(n)?.restore()
    }
    pub(super) fn read_bundle(&self, n: u64) -> Result<Bundle> {
        let root = self.root.join(format!("generation-{n:020}"));
        let manifest: Manifest =
            serde_json::from_slice(&read_regular(&root.join("manifest.json"))?)
                .map_err(|_| invalid("manifest schema"))?;
        if manifest.format != "lantern-generation"
            || ![1, 2].contains(&manifest.version)
            || manifest.generation != n.to_string()
        {
            return Err(invalid("manifest version or generation"));
        }
        if (manifest.version == 1 && manifest.source.is_some())
            || (manifest.version == 2 && manifest.source.is_none())
        {
            return Err(invalid("manifest provenance version"));
        }
        if let Some(source) = &manifest.source {
            source.validate()?;
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
        let bundle = Bundle {
            registry: registry.ok_or_else(|| invalid("missing registry"))?,
            chunks,
        };
        bundle.restore()?;
        Ok(bundle)
    }
}
