use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("create dir {path:?}"))
}

pub fn move_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        ensure_dir(parent)?;
    }
    match fs::rename(from, to) {
        Ok(_) => Ok(()),
        Err(_) => {
            fs::copy(from, to).with_context(|| format!("copy {:?} -> {:?}", from, to))?;
            fs::remove_file(from).with_context(|| format!("remove {:?}", from))?;
            Ok(())
        }
    }
}

pub fn unique_suffix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn unique_name(base: &str) -> String {
    format!("{base}.{}", unique_suffix_millis())
}

pub fn archive_dir_for(base_path: &Path) -> Result<PathBuf> {
    let parent = base_path.parent().ok_or_else(|| anyhow!("no parent"))?;
    Ok(parent.join(".stconflict-archive"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn unique_name_prefix() {
        let n = unique_name("file.txt");
        assert!(n.starts_with("file.txt."));
    }

    #[test]
    fn ensure_dir_creates() {
        let td = tempdir().unwrap();
        let p = td.path().join("a/b/c");
        ensure_dir(&p).unwrap();
        assert!(p.is_dir());
    }

    #[test]
    fn move_file_rename_or_copy_delete() {
        let td = tempdir().unwrap();
        let from = td.path().join("from.txt");
        let mut f = fs::File::create(&from).unwrap();
        writeln!(f, "hello").unwrap();

        let to = td.path().join("subdir").join("to.txt");
        move_file(&from, &to).unwrap();
        assert!(!from.exists());
        assert!(to.exists());
        let s = fs::read_to_string(&to).unwrap();
        assert!(s.contains("hello"));
    }

    #[test]
    fn archive_dir_for_places_in_parent() {
        let td = tempdir().unwrap();
        let base = td.path().join("x").join("file.txt");
        ensure_dir(base.parent().unwrap()).unwrap();
        let a = archive_dir_for(&base).unwrap();
        assert_eq!(a, td.path().join("x").join(".stconflict-archive"));
    }
}
