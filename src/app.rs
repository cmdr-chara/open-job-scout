use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::model::{ApplicationStatus, Job, demo_jobs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Recommended,
    Applied,
    Interviews,
    Pipeline,
}

impl Tab {
    pub const ALL: [Self; 4] = [
        Self::Recommended,
        Self::Applied,
        Self::Interviews,
        Self::Pipeline,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Recommended => "Recommended",
            Self::Applied => "Applied",
            Self::Interviews => "Interviews",
            Self::Pipeline => "Pipeline",
        }
    }

    pub fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|tab| *tab == self)
            .unwrap_or_default();
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|tab| *tab == self)
            .unwrap_or_default();
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Browse,
    Search,
}

pub struct App {
    pub jobs: Vec<Job>,
    pub active_tab: Tab,
    pub selected: usize,
    pub search_query: String,
    pub input_mode: InputMode,
    pub show_help: bool,
    pub should_quit: bool,
    pub notice: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            jobs: demo_jobs(),
            active_tab: Tab::Recommended,
            selected: 0,
            search_query: String::new(),
            input_mode: InputMode::Browse,
            show_help: false,
            should_quit: false,
            notice: Some("Rust v2 preview · demo data".into()),
        }
    }
}

impl App {
    pub fn visible_indices(&self) -> Vec<usize> {
        let query = self.search_query.trim().to_lowercase();
        self.jobs
            .iter()
            .enumerate()
            .filter(|(_, job)| self.matches_tab(job))
            .filter(|(_, job)| query.is_empty() || job.search_blob().contains(&query))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn selected_job(&self) -> Option<&Job> {
        let indices = self.visible_indices();
        indices
            .get(self.selected)
            .and_then(|index| self.jobs.get(*index))
    }

    pub fn tab_count(&self, tab: Tab) -> usize {
        self.jobs
            .iter()
            .filter(|job| Self::job_matches_tab(tab, job))
            .count()
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        if self.show_help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => self.show_help = false,
                _ => {}
            }
            return;
        }

        match self.input_mode {
            InputMode::Search => self.handle_search_key(key),
            InputMode::Browse => self.handle_browse_key(key),
        }
        self.clamp_selection();
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search_query.clear();
                self.input_mode = InputMode::Browse;
                self.selected = 0;
            }
            KeyCode::Enter => {
                self.input_mode = InputMode::Browse;
                self.selected = 0;
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.selected = 0;
            }
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.search_query.push(character);
                self.selected = 0;
            }
            _ => {}
        }
    }

    fn handle_browse_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('/') => {
                self.input_mode = InputMode::Search;
                self.notice = None;
            }
            KeyCode::Esc if !self.search_query.is_empty() => {
                self.search_query.clear();
                self.selected = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => {
                self.selected = self.visible_indices().len().saturating_sub(1)
            }
            KeyCode::Left | KeyCode::Char('h') => self.switch_tab(self.active_tab.previous()),
            KeyCode::Right | KeyCode::Char('l') => self.switch_tab(self.active_tab.next()),
            KeyCode::Char('1') => self.switch_tab(Tab::Recommended),
            KeyCode::Char('2') => self.switch_tab(Tab::Applied),
            KeyCode::Char('3') => self.switch_tab(Tab::Interviews),
            KeyCode::Char('4') => self.switch_tab(Tab::Pipeline),
            KeyCode::Char('r') => self.mark_selected(ApplicationStatus::Reviewed),
            KeyCode::Char('a') => self.mark_selected(ApplicationStatus::Applied),
            KeyCode::Char('i') => self.mark_selected(ApplicationStatus::Interview),
            KeyCode::Char('x') => self.mark_selected(ApplicationStatus::Rejected),
            KeyCode::Char('o') => self.mark_selected(ApplicationStatus::Offer),
            KeyCode::Char('c') => self.mark_selected(ApplicationStatus::Closed),
            _ => {}
        }
    }

    fn matches_tab(&self, job: &Job) -> bool {
        Self::job_matches_tab(self.active_tab, job)
    }

    fn job_matches_tab(tab: Tab, job: &Job) -> bool {
        match tab {
            Tab::Recommended => matches!(
                job.status,
                ApplicationStatus::New | ApplicationStatus::Reviewed
            ),
            Tab::Applied => job.status == ApplicationStatus::Applied,
            Tab::Interviews => matches!(
                job.status,
                ApplicationStatus::Interview | ApplicationStatus::Offer
            ),
            Tab::Pipeline => true,
        }
    }

    fn switch_tab(&mut self, tab: Tab) {
        self.active_tab = tab;
        self.selected = 0;
        self.notice = None;
    }

    fn move_selection(&mut self, amount: isize) {
        let count = self.visible_indices().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = if amount.is_negative() {
            self.selected.saturating_sub(amount.unsigned_abs())
        } else {
            (self.selected + amount as usize).min(count - 1)
        };
    }

    fn mark_selected(&mut self, status: ApplicationStatus) {
        let indices = self.visible_indices();
        let Some(job_index) = indices.get(self.selected).copied() else {
            return;
        };
        let title = self.jobs[job_index].title.clone();
        self.jobs[job_index].status = status;
        self.notice = Some(format!("{title} → {}", status.label()));
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        let count = self.visible_indices().len();
        if count == 0 {
            self.selected = 0;
        } else if self.selected >= count {
            self.selected = count - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommended_tab_excludes_finished_pipeline_states() {
        let app = App::default();
        let visible = app.visible_indices();
        assert!(!visible.is_empty());
        assert!(visible.iter().all(|index| matches!(
            app.jobs[*index].status,
            ApplicationStatus::New | ApplicationStatus::Reviewed
        )));
    }

    #[test]
    fn search_filters_visible_jobs_case_insensitively() {
        let app = App {
            search_query: "FASTAPI".into(),
            ..Default::default()
        };
        let visible = app.visible_indices();
        assert_eq!(visible.len(), 1);
        assert_eq!(app.jobs[visible[0]].company, "Northstar Labs");
    }

    #[test]
    fn status_action_moves_job_out_of_recommended_tab() {
        let mut app = App::default();
        let id = app.selected_job().unwrap().id.clone();
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(
            app.jobs.iter().find(|job| job.id == id).unwrap().status,
            ApplicationStatus::Applied
        );
        assert!(
            app.visible_indices()
                .iter()
                .all(|index| app.jobs[*index].id != id)
        );
    }

    #[test]
    fn tab_counts_are_independent_from_search() {
        let mut app = App::default();
        let applied = app.tab_count(Tab::Applied);
        app.search_query = "nonexistent".into();
        assert_eq!(app.tab_count(Tab::Applied), applied);
    }
}
