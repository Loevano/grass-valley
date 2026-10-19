//! Explicit offline paths only. Source roots remain locked and read-only.
pub mod python;
use super::{Generation, Result, Store, invalid, store::SourceProvenance};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyReport {
    pub source_generation: Generation,
    pub destination_generation: Generation,
    pub skipped: Vec<Generation>,
    pub source_manifest_sha256: String,
}
/// Resolve an absent destination leaf under an existing parent. Canonicalizing
/// its parent catches aliases without creating anything or following a final link.
fn destination(path: &Path, protected: &[&Path]) -> Result<PathBuf> {
    let leaf = path
        .file_name()
        .ok_or_else(|| invalid("destination must be a new directory"))?;
    if fs::symlink_metadata(path).is_ok() {
        return Err(invalid("destination already exists"));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent)?;
    if !fs::metadata(&parent)?.is_dir() {
        return Err(invalid("destination parent"));
    }
    let result = parent.join(leaf);
    for source in protected {
        let source = fs::canonicalize(source)?;
        if result.starts_with(&source) || source.starts_with(&result) {
            return Err(invalid("source and destination overlap"));
        }
    }
    Ok(result)
}
/// Copy/convert the newest native generation. Unlike recovery, conversion refuses
/// a damaged newest generation instead of silently discarding its progress.
pub fn convert_native(source: &Path, target: &Path) -> Result<CopyReport> {
    copy_native(source, target, None, false)
}
/// Recover an explicitly selected generation, or the newest complete fallback,
/// into a NEW root. Never switches/deletes/repairs the source in place.
pub fn recover_native(
    source: &Path,
    target: &Path,
    selected: Option<Generation>,
) -> Result<CopyReport> {
    copy_native(source, target, selected, true)
}
fn copy_native(
    source: &Path,
    target: &Path,
    selected: Option<Generation>,
    recover: bool,
) -> Result<CopyReport> {
    copy_with_checkpoints(source, target, selected, recover, |_| Ok(()))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyCheckpoint {
    Write(super::store::Checkpoint),
    Ready,
}
pub(super) fn copy_with_checkpoints(
    source: &Path,
    target: &Path,
    selected: Option<Generation>,
    recover: bool,
    mut checkpoint: impl FnMut(CopyCheckpoint) -> Result<()>,
) -> Result<CopyReport> {
    let store = Store::open_read_only(source)?;
    let target = destination(target, &[store.root()])?;
    let mut skipped = Vec::new();
    let generation = if let Some(selected) = selected {
        selected
    } else if recover {
        let recovery = store.recover()?;
        skipped = recovery.skipped;
        recovery
            .generation
            .ok_or_else(|| invalid("no committed source generation"))?
    } else {
        Generation(
            *store
                .generations()?
                .last()
                .ok_or_else(|| invalid("no committed source generation"))?,
        )
    };
    let bundle = store.read_bundle(generation.0)?;
    let hash = store.manifest_digest(generation)?;
    let mut output = Store::create_copy(&target)?;
    let destination_generation = output.commit_bundle_from(
        bundle,
        Some(SourceProvenance {
            operation: if recover {
                "recover_native"
            } else {
                "copy_native"
            }
            .into(),
            generation: generation.0,
            manifest_sha256: hash.clone(),
        }),
        |phase| checkpoint(CopyCheckpoint::Write(phase)),
    )?;
    output.finish_copy()?;
    checkpoint(CopyCheckpoint::Ready)?;
    Ok(CopyReport {
        source_generation: generation,
        destination_generation,
        skipped,
        source_manifest_sha256: hash,
    })
}
/// Explicit destructive action limited to uncommitted pending directories. It
/// requires the writable OS lock and never removes a committed generation.
pub fn cleanup_pending(source: &Path) -> Result<usize> {
    let store = Store::open(source)?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(store.root())? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_str().is_some_and(|s| s.starts_with("pending-")) {
            if !entry.file_type()?.is_dir() {
                return Err(invalid("non-directory pending work"));
            }
            regular_tree(&entry.path())?;
            paths.push(entry.path());
        }
    }
    let count = paths.len();
    for path in paths {
        fs::remove_dir_all(path)?;
    }
    fs::File::open(store.root())?.sync_all()?;
    Ok(count)
}
fn regular_tree(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            regular_tree(&entry.path())?;
        } else if !kind.is_file() {
            return Err(invalid("nonregular pending entry"));
        }
    }
    Ok(())
}
