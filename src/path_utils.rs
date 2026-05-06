use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn normalize_path_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

pub fn resolve_candidate_preserving_existing_symlinks(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return fs::canonicalize(path)
            .map_err(|error| format!("path_invalid path={} error={error}", path.display()));
    }

    let mut existing = path.to_path_buf();
    let mut missing = Vec::new();
    while !existing.exists() {
        let Some(file_name) = existing.file_name() else {
            break;
        };
        missing.push(file_name.to_owned());
        if !existing.pop() {
            break;
        }
    }

    let mut resolved = if existing.exists() {
        fs::canonicalize(&existing).map_err(|error| {
            format!(
                "path_existing_parent_invalid path={} error={error}",
                existing.display()
            )
        })?
    } else {
        normalize_path_lexically(&existing)
    };

    for component in missing.iter().rev() {
        resolved.push(component);
    }

    Ok(normalize_path_lexically(&resolved))
}
