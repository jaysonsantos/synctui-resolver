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
}
