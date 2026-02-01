use crate::model::{Candidate, ConflictGroup};
use anyhow::Result;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn is_conflict_name(file_name: &str) -> Option<&str> {
    // Syncthing conflict files usually look like:
    //   <base>.sync-conflict-YYYYMMDD-HHMMSS-DEVICE
    // We treat anything containing ".sync-conflict-" as a conflict file.
    file_name
        .find(".sync-conflict-")
        .map(|idx| &file_name[..idx])
}

fn stat_candidate(path: PathBuf, is_original: bool, label: String) -> Candidate {
    let meta = fs::metadata(&path).ok();
    Candidate {
        exists: meta.is_some(),
        size: meta.as_ref().map(|m| m.len()),
        modified: meta.and_then(|m| m.modified().ok()),
        path,
        is_original,
        label,
    }
}

pub fn scan_conflicts(root: &Path, include_hidden: bool) -> Result<Vec<ConflictGroup>> {
    let mut by_base: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();

    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy();
        if !include_hidden {
            if file_name.starts_with('.') {
                continue;
            }
            // Also ignore dot-directories by checking any component
            if entry
                .path()
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
            {
                continue;
            }
        }

        let Some(base_name) = is_conflict_name(&file_name) else {
            continue;
        };

        let base_path = entry.path().parent().unwrap_or(root).join(base_name);
        by_base
            .entry(base_path)
            .or_default()
            .push(entry.path().to_path_buf());
    }

    let mut groups = Vec::new();
    for (base_path, conflict_paths) in by_base {
        let mut candidates = Vec::new();
        candidates.push(stat_candidate(
            base_path.clone(),
            true,
            "Original".to_string(),
        ));

        for (i, p) in conflict_paths.into_iter().enumerate() {
            let label = format!("Conflict {}", i + 1);
            candidates.push(stat_candidate(p, false, label));
        }

        // Ensure deterministic order: original first, then conflicts sorted by path.
        let (orig, mut rest): (Vec<_>, Vec<_>) =
            candidates.into_iter().partition(|c| c.is_original);
        rest.sort_by(|a, b| a.path.cmp(&b.path));
        let mut candidates = orig;
        candidates.extend(rest);

        groups.push(ConflictGroup {
            base_path,
            candidates,
            chosen: None,
        });
    }

    Ok(groups)
}

pub fn rel_path<'a>(root: &'a Path, p: &'a Path) -> &'a Path {
    p.strip_prefix(root).unwrap_or(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_file(path: &Path, content: &str) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn rel_path_strips_prefix() {
        let root = Path::new("/a/b");
        let p = Path::new("/a/b/c/d.txt");
        assert_eq!(rel_path(root, p), Path::new("c/d.txt"));
    }

    #[test]
    fn scan_finds_groups_and_original_candidate() {
        let td = tempdir().unwrap();
        let root = td.path();

        let base = root.join("notes.txt");
        let c1 = root.join("notes.txt.sync-conflict-20240101-010101-DEV");
        let c2 = root.join("notes.txt.sync-conflict-20240102-020202-DEV");

        write_file(&base, "orig");
        write_file(&c1, "c1");
        write_file(&c2, "c2");

        let groups = scan_conflicts(root, true).unwrap();
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.base_path, base);
        assert!(g.candidates.len() >= 3);
        assert!(g.candidates[0].is_original);
        assert_eq!(g.candidates[0].path, g.base_path);
    }

    #[test]
    fn scan_ignores_hidden_dirs_by_default() {
        let td = tempdir().unwrap();
        let root = td.path();

        let base = root.join(".hidden").join("file.txt");
        let c1 = root
            .join(".hidden")
            .join("file.txt.sync-conflict-20240101-010101-DEV");
        write_file(&c1, "c");
        write_file(&base, "o");

        let groups = scan_conflicts(root, false).unwrap();
        assert_eq!(groups.len(), 0);

        let groups = scan_conflicts(root, true).unwrap();
        assert_eq!(groups.len(), 1);
    }
}
