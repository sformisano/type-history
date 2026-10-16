//! Resolve authored input paths while retaining every traversed filesystem identity.
use crate::Result;
use std::collections::{BTreeSet, VecDeque};
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use toml::Value;

pub(super) struct DeclaredSnapshotInput {
    pub(super) resolved: PathBuf,
    pub(super) resolution: BTreeSet<PathBuf>,
}

pub(super) fn resolve_snapshot_input(root: &Path, path: &Path) -> Result<DeclaredSnapshotInput> {
    let (resolved, resolution) = resolve_snapshot_route(root, path, false)?;
    Ok(DeclaredSnapshotInput {
        resolved: resolved.ok_or("snapshot input must exist")?,
        resolution,
    })
}

#[derive(Clone)]
enum RouteStep {
    Root,
    Parent,
    Normal(OsString),
}

fn route_steps(path: &Path) -> Result<VecDeque<RouteStep>> {
    path.components()
        .filter_map(|component| match component {
            Component::Prefix(_) => Some(Err("snapshot inputs require Unix paths".into())),
            Component::RootDir => Some(Ok(RouteStep::Root)),
            Component::CurDir => None,
            Component::ParentDir => Some(Ok(RouteStep::Parent)),
            Component::Normal(name) => Some(Ok(RouteStep::Normal(name.to_owned()))),
        })
        .collect()
}

pub(super) fn resolve_snapshot_route(
    root: &Path,
    path: &Path,
    allow_missing_final: bool,
) -> Result<(Option<PathBuf>, BTreeSet<PathBuf>)> {
    let authored = root.join(path);
    let mut pending = route_steps(&authored)?;
    let mut cursor = PathBuf::new();
    let mut resolution = BTreeSet::new();
    let mut followed_links = 0_u8;
    while let Some(step) = pending.pop_front() {
        match step {
            RouteStep::Root => cursor = PathBuf::from("/"),
            RouteStep::Parent => {
                if cursor != Path::new("/") {
                    if !fs::symlink_metadata(&cursor)?.is_dir() {
                        return Err(format!(
                            "snapshot input traverses through non-directory {}",
                            cursor.display()
                        )
                        .into());
                    }
                    cursor.pop();
                }
            }
            RouteStep::Normal(name) => {
                cursor.push(name);
                resolution.insert(cursor.clone());
                let metadata = match fs::symlink_metadata(&cursor) {
                    Ok(metadata) => metadata,
                    Err(error)
                        if error.kind() == ErrorKind::NotFound
                            && allow_missing_final
                            && pending.is_empty() =>
                    {
                        return Ok((None, resolution));
                    }
                    Err(error) => return Err(error.into()),
                };
                if metadata.file_type().is_symlink() {
                    followed_links = followed_links
                        .checked_add(1)
                        .ok_or("snapshot input symlink depth overflow")?;
                    if followed_links > 64 {
                        return Err("snapshot input has too many symlink hops".into());
                    }
                    let target = fs::read_link(&cursor)?;
                    cursor.pop();
                    let mut target_steps = route_steps(&target)?;
                    target_steps.append(&mut pending);
                    pending = target_steps;
                } else if !pending.is_empty() && !metadata.is_dir() {
                    return Err(format!(
                        "snapshot input traverses through non-directory {}",
                        cursor.display()
                    )
                    .into());
                } else if !metadata.is_dir() && !metadata.is_file() {
                    return Err(format!(
                        "snapshot input is not a regular file: {}",
                        cursor.display()
                    )
                    .into());
                }
            }
        }
    }
    let resolved = authored.canonicalize().map_err(|error| {
        format!(
            "cannot resolve snapshot input {}: {error}",
            authored.display()
        )
    })?;
    if cursor != resolved {
        return Err(format!(
            "snapshot input resolution disagrees with the filesystem for {}",
            authored.display()
        )
        .into());
    }
    Ok((Some(resolved), resolution))
}

pub(super) fn capture_optional_input(
    root: &Path,
    path: &Path,
    extra: &mut Vec<PathBuf>,
    resolution: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let logical = root.join(path);
    let (resolved, route) = resolve_snapshot_route(root, path, true)?;
    resolution.extend(route);
    if let Some(resolved) = resolved {
        extra.push(resolved);
    } else {
        extra.push(logical);
    }
    Ok(())
}

pub(super) fn declared_snapshot_inputs(
    root: &Path,
    document: &Value,
) -> Result<Vec<DeclaredSnapshotInput>> {
    let Some(value) = document
        .get("package")
        .and_then(|value| value.get("metadata"))
        .and_then(|value| value.get("type-history"))
        .and_then(|value| value.get("snapshot-inputs"))
    else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or(
        "package.metadata.type-history.snapshot-inputs must be an array of relative paths",
    )?;
    values
        .iter()
        .map(|value| {
            let value = value.as_str().ok_or(
                "package.metadata.type-history.snapshot-inputs must contain relative paths",
            )?;
            let path = Path::new(value);
            if path.is_absolute() {
                return Err(
                    "package.metadata.type-history.snapshot-inputs must contain relative paths"
                        .into(),
                );
            }
            resolve_snapshot_input(root, path).map_err(|error| {
                format!(
                    "cannot resolve declared Type History snapshot input {}: {error}",
                    root.join(path).display()
                )
                .into()
            })
        })
        .collect()
}

pub(super) fn lexical_absolute(path: &Path) -> Result<PathBuf> {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => return Err("snapshot inputs require Unix paths".into()),
            Component::RootDir => resolved = PathBuf::from("/"),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => resolved.push(name),
        }
    }
    if !resolved.is_absolute() {
        return Err("snapshot path must be absolute".into());
    }
    Ok(resolved)
}
pub(super) fn mirrored(mirror: &Path, path: &Path) -> Result<PathBuf> {
    Ok(mirror.join(
        path.strip_prefix(Path::new("/"))
            .map_err(|_| "snapshot requires absolute Unix paths")?,
    ))
}
