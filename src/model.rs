use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct Candidate {
    pub path: PathBuf,
    pub exists: bool,
    pub is_original: bool,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct ConflictGroup {
    pub base_path: PathBuf,
    pub candidates: Vec<Candidate>,
    pub chosen: Option<usize>,
}

impl ConflictGroup {
    pub fn newest_idx(&self) -> Option<usize> {
        self.candidates
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.modified.map(|m| (i, m)))
            .max_by_key(|(_, m)| *m)
            .map(|(i, _)| i)
    }

    pub fn oldest_idx(&self) -> Option<usize> {
        self.candidates
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.modified.map(|m| (i, m)))
            .min_by_key(|(_, m)| *m)
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use std::time::UNIX_EPOCH;

    fn cand(label: &str, secs: u64) -> Candidate {
        Candidate {
            path: PathBuf::from(label),
            exists: true,
            is_original: false,
            size: None,
            modified: Some(UNIX_EPOCH + Duration::from_secs(secs)),
            label: label.to_string(),
        }
    }

    #[test]
    fn newest_and_oldest_idx_pick_by_modified_time() {
        let g = ConflictGroup {
            base_path: PathBuf::from("base"),
            candidates: vec![cand("a", 10), cand("b", 5), cand("c", 99)],
            chosen: None,
        };
        assert_eq!(g.oldest_idx(), Some(1));
        assert_eq!(g.newest_idx(), Some(2));
    }
}
