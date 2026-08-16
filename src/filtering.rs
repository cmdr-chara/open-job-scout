use crate::{model::{Job, WorkMode}, ranking::normalize_text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Score,
    Newest,
}

impl SortMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Score => "Best match",
            Self::Newest => "Newest",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Score => Self::Newest,
            Self::Newest => Self::Score,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueueFilters {
    pub min_score: u8,
    pub work_mode: Option<WorkMode>,
    pub source: Option<String>,
    pub sort: SortMode,
}

impl Default for QueueFilters {
    fn default() -> Self {
        Self {
            min_score: 0,
            work_mode: None,
            source: None,
            sort: SortMode::Score,
        }
    }
}

impl QueueFilters {
    pub fn is_active(&self) -> bool {
        self.min_score > 0 || self.work_mode.is_some() || self.source.is_some()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn matches(&self, job: &Job) -> bool {
        if job.score < f64::from(self.min_score) {
            return false;
        }
        if let Some(work_mode) = self.work_mode
            && job.work_mode != work_mode
        {
            return false;
        }
        if let Some(source) = self.source.as_deref()
            && !normalize_text(&job.source).eq(&normalize_text(source))
        {
            return false;
        }
        true
    }

    pub fn sort_indices(&self, jobs: &[Job], indices: &mut [usize]) {
        match self.sort {
            SortMode::Score => indices.sort_by(|left, right| {
                jobs[*right]
                    .score
                    .total_cmp(&jobs[*left].score)
                    .then_with(|| jobs[*right].last_seen.cmp(&jobs[*left].last_seen))
                    .then_with(|| jobs[*left].id.cmp(&jobs[*right].id))
            }),
            SortMode::Newest => indices.sort_by(|left, right| {
                jobs[*right]
                    .posted
                    .cmp(&jobs[*left].posted)
                    .then_with(|| jobs[*right].last_seen.cmp(&jobs[*left].last_seen))
                    .then_with(|| jobs[*right].score.total_cmp(&jobs[*left].score))
            }),
        }
    }

    pub fn adjust_min_score(&mut self, delta: i8) {
        let next = i16::from(self.min_score) + i16::from(delta) * 5;
        self.min_score = next.clamp(0, 100) as u8;
    }

    pub fn cycle_work_mode(&mut self, direction: i8) {
        const VALUES: [Option<WorkMode>; 5] = [
            None,
            Some(WorkMode::Remote),
            Some(WorkMode::Hybrid),
            Some(WorkMode::Onsite),
            Some(WorkMode::Unknown),
        ];
        let current = VALUES
            .iter()
            .position(|value| *value == self.work_mode)
            .unwrap_or_default();
        let len = VALUES.len() as i16;
        let next = (current as i16 + i16::from(direction)).rem_euclid(len) as usize;
        self.work_mode = VALUES[next];
    }

    pub fn cycle_source(&mut self, jobs: &[Job], direction: i8) {
        let mut sources = jobs
            .iter()
            .map(|job| job.source.trim())
            .filter(|source| !source.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        sources.sort_by_key(|source| normalize_text(source));
        sources.dedup_by(|left, right| normalize_text(left) == normalize_text(right));
        if sources.is_empty() {
            self.source = None;
            return;
        }
        let mut values: Vec<Option<String>> = Vec::with_capacity(sources.len() + 1);
        values.push(None);
        values.extend(sources.into_iter().map(Some));
        let current = values
            .iter()
            .position(|value| value.as_deref() == self.source.as_deref())
            .unwrap_or_default();
        let len = values.len() as i16;
        let next = (current as i16 + i16::from(direction)).rem_euclid(len) as usize;
        self.source = values[next].clone();
    }

    pub fn work_mode_label(&self) -> &'static str {
        self.work_mode.map_or("Any", WorkMode::label)
    }

    pub fn source_label(&self) -> &str {
        self.source.as_deref().unwrap_or("Any")
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.min_score > 0 {
            parts.push(format!("score ≥{}", self.min_score));
        }
        if let Some(mode) = self.work_mode {
            parts.push(mode.label().to_string());
        }
        if let Some(source) = &self.source {
            parts.push(source.clone());
        }
        if parts.is_empty() {
            "No filters".into()
        } else {
            parts.join(" · ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::demo_jobs;

    #[test]
    fn filters_match_score_and_work_mode() {
        let jobs = demo_jobs();
        let mut filters = QueueFilters {
            min_score: 90,
            work_mode: Some(WorkMode::Remote),
            ..Default::default()
        };
        assert!(filters.matches(&jobs[0]));
        filters.work_mode = Some(WorkMode::Onsite);
        assert!(!filters.matches(&jobs[0]));
    }

    #[test]
    fn score_adjustment_is_bounded() {
        let mut filters = QueueFilters::default();
        filters.adjust_min_score(-1);
        assert_eq!(filters.min_score, 0);
        for _ in 0..30 {
            filters.adjust_min_score(1);
        }
        assert_eq!(filters.min_score, 100);
    }

    #[test]
    fn source_cycle_includes_any() {
        let jobs = demo_jobs();
        let mut filters = QueueFilters::default();
        filters.cycle_source(&jobs, 1);
        assert!(filters.source.is_some());
        for _ in 0..20 {
            filters.cycle_source(&jobs, 1);
            if filters.source.is_none() {
                return;
            }
        }
        panic!("source cycle did not return to Any");
    }
}
