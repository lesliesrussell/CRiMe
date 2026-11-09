use crate::*;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Modifier,
    text::{Line, Span, Text},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};
use tachyonfx::{EffectRenderer, Interpolation, fx};
mod actions;
mod details;
mod editor;
pub(crate) mod form;
pub mod theme;
mod tools;
mod views;
use editor::Editor;
use theme::Theme;
const PREFIX_HINT: &str = "C-x → b category • o focus\nC-c quit • C-g cancel • Esc cancel";
#[cfg(test)]
const CATEGORIES: [&str; 6] = ["All", "Contacts", "Companies", "Deals", "Tasks", "Notes"];
const HELP: &str = "KEY GUIDE\n\nBrowse\n  C-n / C-p       Next / previous record (or ↑ / ↓)\n  M-< / M->       First / last record\n  Tab / Shift-Tab Next / previous category\n  M-1 … M-6       All, Contacts, Companies, Deals, Tasks, Notes\n  C-x b           Next category\n  C-x o           Switch focus: records / details\n  C-v / M-v       Scroll details down / up\n  C-s or /        Search names and fields as you type\n  C-g             Cancel / clear search / close help\n\nWorkflows\n  r / B           Related records / back to previous location\n  Space / A / C   Toggle selection / select visible / clear selection\n  U               Bulk update, trash or export selected records\n  M               Preview and merge a duplicate into current record\n  g / M-0         Today: overdue tasks, next actions, follow-ups\n  v / V           Open saved views / save current filters as a view\n  l               Unified timeline for selected record\n  a / f           Add linked activity note / follow-up task\n  x               Mark selected task done\n\nActions\n  n               New record form (all types)\n  e               Edit all fields in a record form\n  s               Define fields/defaults in a saved record schema\n  d               Move selected record to trash (records focus)\n  M-x             Searchable command palette\n  :               Advanced CRM command input\n  u / b           Undo deletion / browse trash\n  i / c           Import CSV / create a record type\n  T               Cycle theme (saved for this database)\n  ? / F1 / C-h    Show this guide\n  C-x C-c or q    Quit\n  C-c             Reserved for future application bindings\n\nRecord forms\n  Tab / C-n/p     Select a field\n  Enter           Edit or apply the selected field\n  F2              Add a named field\n  F3              Remove a field or item\n  F4              Add an item to a selected list\n  F5              Change field type (clears its draft value)\n  F6              Field required/choices/description (schema editor)\n  F7 / F9         Move schema field up / down\n  F8              Preview migration to current schema (edit forms)\n  F10 / C-x C-s   Save the record\n  C-j             New line while editing text\n  Esc / C-g       Back or cancel; unsaved changes stay in a draft\n\nMinibuffer editing\n  C-a / C-e       Start / end of line\n  C-b / C-f       Back / forward character\n  M-b / M-f       Back / forward word\n  C-d             Delete character\n  C-k / C-u       Kill to end / beginning\n  C-w / M-d       Kill previous / next word\n  C-y             Yank last killed text\n  C-p / C-n       Previous / next command (or ↑ / ↓)\n  Enter           Accept   C-g / Esc: cancel\n\nAdvanced commands (:)\n  add contact 'Ada' '{\"email\":\"ada@example.com\"}'\n  update RECORD_ID '{\"phone\":\"123\"}'\n  delete RECORD_ID\n  list contact Ada\n  theme dracula | solarized-dark | solarized-light | tokyo-night\n  refresh   help\n\nC- means Ctrl; M- means Alt/Option. Arrows and : work\nif your terminal reserves an Emacs shortcut.";
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Browse,
    Command,
    Search,
}
struct App {
    marked: std::collections::BTreeSet<String>,
    action_panel: Option<actions::Panel>,
    navigation: Vec<actions::Navigation>,
    records: Vec<Value>,
    kinds: Vec<String>,
    category_labels: Vec<String>,
    tab: usize,
    selected: usize,
    scroll: u16,
    query: String,
    previous_query: String,
    editor: Editor,
    mode: Mode,
    theme: Theme,
    help: bool,
    details_focus: bool,
    prefix: bool,
    message: String,
    form: Option<form::Form>,
    view_manager: Option<views::Manager>,
    active_view: Option<workflow::View>,
    today: bool,
    agenda: Vec<workflow::AgendaItem>,
    timeline: Option<Value>,
    related_to: Option<String>,
    agenda_date: chrono::NaiveDate,
    palette: Option<tools::Commands>,
    tool: Option<tools::Tool>,
}
impl App {
    fn new(store: &Store, theme: Option<Theme>) -> Result<Self> {
        store.conn.execute_batch("CREATE TABLE IF NOT EXISTS preferences (key VARCHAR PRIMARY KEY, value VARCHAR NOT NULL)")?;
        let saved = store
            .conn
            .query_row(
                "SELECT value FROM preferences WHERE key='theme'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| Theme::parse(&s));
        Ok(Self {
            kinds: std::iter::once(String::new())
                .chain(store.types()?.into_iter().map(|t| t.slug))
                .collect(),
            category_labels: std::iter::once("All".into())
                .chain(store.types()?.into_iter().map(|t| t.label))
                .collect(),
            palette: None,
            tool: None,
            form: None,
            view_manager: None,
            active_view: None,
            today: false,
            agenda: workflow::agenda(&load(store)?, workflow::date()),
            timeline: None,
            related_to: None,
            agenda_date: workflow::date(),
            marked: actions::empty_selection(),
            action_panel: None,
            navigation: vec![],
            records: load(store)?,
            tab: 0,
            selected: 0,
            scroll: 0,
            query: String::new(),
            previous_query: String::new(),
            editor: Editor::default(),
            mode: Mode::Browse,
            theme: theme.or(saved).unwrap_or_default(),
            help: false,
            details_focus: false,
            prefix: false,
            message: "M-x: Actions • n: New • i: Import • u: Undo deletion • ?: Keys".into(),
        })
    }
    fn visible(&self) -> Vec<&Value> {
        let query = self.query.to_lowercase();
        if self.today {
            return self
                .agenda
                .iter()
                .map(|item| &item.record)
                .filter(|r| r.to_string().to_lowercase().contains(&query))
                .collect();
        }
        let records = if let Some(view) = &self.active_view {
            view.select(&self.records, workflow::date())
        } else {
            self.records
                .iter()
                .filter(|r| self.tab == 0 || r["kind"] == self.kinds[self.tab].as_str())
                .collect()
        };
        records
            .into_iter()
            .filter(|r| r.to_string().to_lowercase().contains(&query))
            .collect()
    }
    fn restored(&mut self, store: &Store, record: &Value) -> Result<()> {
        self.refresh(store)?;
        self.query.clear();
        self.category(
            self.kinds
                .iter()
                .position(|k| record["kind"] == *k)
                .unwrap_or(0),
        );
        self.selected = self
            .visible()
            .iter()
            .position(|r| r["id"] == record["id"])
            .unwrap_or(0);
        self.message = format!("Restored {}", record["name"].as_str().unwrap_or("record"));
        Ok(())
    }
    fn dispatch(&mut self, store: &Store, action: tools::Action) -> Result<bool> {
        use tools::Action;
        match action {
            Action::Category(tab) => {
                self.category(tab);
                Ok(false)
            }
            Action::Quit => Ok(true),
            Action::SetTheme(theme) => {
                self.set_theme(store, theme)?;
                Ok(false)
            }
            Action::Raw(text) => {
                self.begin(Mode::Command);
                self.editor.set(&text);
                Ok(false)
            }
            action => {
                let code = match action {
                    Action::Related => 'r',
                    Action::Back => 'B',
                    Action::Bulk => 'U',
                    Action::Merge => 'M',
                    Action::Select => ' ',
                    Action::Today => 'g',
                    Action::New => 'n',
                    Action::Edit => 'e',
                    Action::Delete => {
                        self.details_focus = false;
                        self.help = false;
                        'd'
                    }
                    Action::Undo => 'u',
                    Action::Trash => 'b',
                    Action::Import => 'i',
                    Action::NewType => 'c',
                    Action::Schema => 's',
                    Action::Views => 'v',
                    Action::SaveView => 'V',
                    Action::Timeline => 'l',
                    Action::Note => 'a',
                    Action::FollowUp => 'f',
                    Action::Done => 'x',
                    Action::Theme => 'T',
                    Action::Help => '?',
                    _ => unreachable!(),
                };
                self.handle(
                    store,
                    KeyEvent::new(KeyCode::Char(code), KeyModifiers::NONE),
                )
            }
        }
    }
    fn refresh_catalog(&mut self, store: &Store) -> Result<()> {
        let types = store.types()?;
        self.kinds = std::iter::once(String::new())
            .chain(types.iter().map(|t| t.slug.clone()))
            .collect();
        self.category_labels = std::iter::once("All".into())
            .chain(types.into_iter().map(|t| t.label))
            .collect();
        Ok(())
    }
    fn refresh(&mut self, store: &Store) -> Result<()> {
        self.refresh_catalog(store)?;
        self.records = load(store)?;
        self.marked.retain(|id| {
            self.records
                .iter()
                .any(|r| r["id"].as_str() == Some(id.as_str()))
        });
        self.agenda_date = workflow::date();
        self.agenda = workflow::agenda(&self.records, self.agenda_date);
        self.selected = self.selected.min(self.visible().len().saturating_sub(1));
        Ok(())
    }
    fn category(&mut self, tab: usize) {
        self.tab = tab;
        self.today = false;
        self.active_view = None;
        self.timeline = None;
        self.reset();
    }
    fn related_form(&mut self, store: &Store, kind: &str, record: &Value) -> Result<()> {
        let mut entry = form::Form::new(Some(kind), None, &self.records);
        entry.restore_schema(store)?;
        entry.preset_link(record);
        self.form = Some(entry);
        self.related_to = record["id"].as_str().map(str::to_owned);
        self.timeline = None;
        Ok(())
    }

    fn current(&self) -> Option<Value> {
        self.visible().get(self.selected).map(|r| (*r).clone())
    }
    fn reset(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }
    fn set_theme(&mut self, store: &Store, theme: Theme) -> Result<()> {
        store.conn.execute("INSERT INTO preferences VALUES ('theme',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![theme.slug()])?;
        self.theme = theme;
        self.message = format!("Theme: {} • saved", theme.name());
        Ok(())
    }
    fn begin(&mut self, mode: Mode) {
        self.mode = mode;
        self.editor.clear();
        self.prefix = false;
        if mode == Mode::Search {
            self.previous_query = self.query.clone();
            self.editor.set(&self.query);
        }
    }
    fn cancel(&mut self) {
        if self.mode == Mode::Search {
            self.query = self.previous_query.clone();
            self.reset();
        }
        self.mode = Mode::Browse;
        self.editor.clear();
        self.prefix = false;
        self.help = false;
        self.message = "Cancelled".into();
    }
    fn command(&mut self, store: &Store, text: &str) -> Result<()> {
        let args = shlex::split(text).context("Unmatched quote")?;
        match args.first().map(String::as_str) {
            Some("theme") => {
                let name = args
                    .get(1)
                    .context("Use theme dracula, solarized-dark, solarized-light or tokyo-night")?;
                self.set_theme(store, Theme::parse(name).context("Unknown theme")?)?;
            }
            Some("help") => {
                self.help = true;
                self.scroll = 0;
            }
            Some("refresh") => {
                self.refresh(store)?;
                self.query.clear();
                self.reset();
                self.message = "Records refreshed".into();
            }
            _ => {
                let value = store.execute(&args)?;
                self.refresh(store)?;
                if args.first().is_some_and(|a| a == "list") {
                    self.today = false;
                    self.active_view = None;
                    self.tab = args
                        .get(1)
                        .and_then(|kind| self.kinds.iter().position(|k| k == kind))
                        .unwrap_or(0);
                    self.query = args.get(2).cloned().unwrap_or_default();
                }
                self.reset();
                if let Some(id) = value["id"].as_str() {
                    self.today = false;
                    self.active_view = None;
                    self.query.clear();
                    self.tab = 0;
                    self.selected = self
                        .visible()
                        .iter()
                        .position(|r| r["id"] == id)
                        .unwrap_or(0);
                }
                self.message = "Command completed".into();
            }
        }
        Ok(())
    }
    fn accept(&mut self, store: &Store) -> Result<()> {
        let mode = self.mode;
        let text = if mode == Mode::Command {
            self.editor.submit()
        } else {
            let s = self.editor.text();
            self.editor.clear();
            s
        };
        self.mode = Mode::Browse;
        match mode {
            Mode::Command => self.command(store, &text),
            Mode::Search => {
                self.message = format!("Search: {} • C-g clears", self.query);
                Ok(())
            }
            Mode::Browse => Ok(()),
        }
    }
    fn handle(&mut self, store: &Store, key: KeyEvent) -> Result<bool> {
        if let Some(palette) = self.palette.as_mut() {
            if key.code == KeyCode::Esc
                || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('g'))
            {
                self.palette = None;
                return Ok(false);
            }
            if let Some(action) = palette.handle(key) {
                self.palette = None;
                return self.dispatch(store, action);
            }
            return Ok(false);
        }
        if self.action_key(store, key)? {
            return Ok(false);
        }
        if let Some(tool) = self.tool.as_mut() {
            match tool.handle(store, key)? {
                tools::Outcome::Continue => {}
                tools::Outcome::Close => self.tool = None,
                tools::Outcome::Restored(record) => {
                    self.tool = None;
                    self.restored(store, &record)?;
                }
                tools::Outcome::TypeCreated(kind) => {
                    self.tool = None;
                    self.refresh_catalog(store)?;
                    self.category(self.kinds.iter().position(|k| *k == kind.slug).unwrap_or(0));
                    let mut entry = form::Form::schema_editor(Some(&kind.slug), &self.records);
                    entry.restore_schema(store)?;
                    self.form = Some(entry);
                }
                tools::Outcome::Imported { kind, summary } => {
                    self.tool = None;
                    self.refresh(store)?;
                    self.query.clear();
                    self.category(self.kinds.iter().position(|k| *k == kind).unwrap_or(0));
                    self.message = format!(
                        "Import finished: {} added, {} merged, {} skipped",
                        summary["added"], summary["merged"], summary["skipped"]
                    );
                }
            }
            return Ok(false);
        }
        if let Some(manager) = self.view_manager.as_mut() {
            match manager.handle(store, key)? {
                views::Outcome::Continue => {}
                views::Outcome::Close => self.view_manager = None,
                views::Outcome::Open(view) => {
                    self.view_manager = None;
                    self.today = false;
                    self.tab = self.kinds.iter().position(|k| *k == view.kind).unwrap_or(0);
                    self.active_view = Some(view);
                    self.query.clear();
                    self.reset();
                }
            }
            return Ok(false);
        }
        if self.timeline.is_some() && self.form.is_none() {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            match key.code {
                KeyCode::Esc | KeyCode::Char('l') => {
                    self.timeline = None;
                    self.scroll = 0;
                }
                KeyCode::Char('g') if ctrl => {
                    self.timeline = None;
                    self.scroll = 0;
                }
                KeyCode::Down | KeyCode::PageDown => self.scroll = self.scroll.saturating_add(5),
                KeyCode::Up | KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(5),
                KeyCode::Char('n' | 'v') if ctrl => self.scroll = self.scroll.saturating_add(5),
                KeyCode::Char('p') if ctrl => self.scroll = self.scroll.saturating_sub(5),
                KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::ALT) => {
                    self.scroll = self.scroll.saturating_sub(5)
                }
                KeyCode::Char('a' | 'f') if key.modifiers.is_empty() => {
                    let record = self.timeline.as_ref().unwrap()["record"].clone();
                    self.related_form(
                        store,
                        if key.code == KeyCode::Char('a') {
                            "note"
                        } else {
                            "task"
                        },
                        &record,
                    )?;
                }
                _ => {}
            }
            return Ok(false);
        }
        if let Some(form) = self.form.as_mut() {
            match form.handle(store, &self.records, key) {
                form::Outcome::Continue => {}
                form::Outcome::Cancel => {
                    self.form = None;
                    if let Some(id) = self.related_to.take() {
                        self.timeline = Some(store.timeline(&id)?);
                    }
                    self.message = "Changes discarded; saved records are unchanged.".into();
                }
                form::Outcome::SchemaSaved(kind, version) => {
                    self.form = None;
                    self.message = format!(
                        "{} schema v{version} saved. New records use it; edit + F8 migrates older records.",
                        details::label(&kind)
                    );
                }
                form::Outcome::Saved(record) => {
                    self.form = None;
                    self.refresh(store)?;
                    self.help = false;
                    self.scroll = 0;
                    if let Some(id) = self.related_to.take() {
                        self.timeline = Some(store.timeline(&id)?);
                    } else if !self.today && self.active_view.is_none() {
                        self.query.clear();
                        self.category(
                            self.kinds
                                .iter()
                                .position(|k| record["kind"] == *k)
                                .unwrap_or(0),
                        );
                        self.selected = self
                            .visible()
                            .iter()
                            .position(|r| r["id"] == record["id"])
                            .unwrap_or(0);
                    }
                    self.message = "Record saved • n: New • e: Edit • C-s: Search".into();
                }
            }
            return Ok(false);
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if ctrl && key.code == KeyCode::Char('g') {
            if self.mode == Mode::Browse {
                self.query.clear();
                self.reset();
            }
            self.cancel();
            return Ok(false);
        }
        if self.prefix {
            self.prefix = false;
            self.message.clear();
            match key.code {
                KeyCode::Char('c') if ctrl => return Ok(true),
                KeyCode::Char('b') => {
                    self.category((self.tab + 1) % self.kinds.len());
                }
                KeyCode::Char('o') => self.details_focus = !self.details_focus,
                KeyCode::Esc => {}
                _ => self.message = "Unknown C-x key. Press C-x for choices or ? for help.".into(),
            }
            return Ok(false);
        }
        if ctrl && key.code == KeyCode::Char('c') {
            // Reserved for the application keymap; never cancel input or exit.
            if self.mode == Mode::Browse {
                self.message = "C-c is reserved for future bindings. Use C-x C-c to quit.".into();
            }
            return Ok(false);
        }
        if self.mode != Mode::Browse {
            match key.code {
                KeyCode::Esc => self.cancel(),
                KeyCode::Enter => {
                    if let Err(e) = self.accept(store) {
                        self.message = format!("Error: {e}");
                    }
                }
                _ => self.editor.handle(key),
            }
            if self.mode == Mode::Search {
                self.query = self.editor.text();
                self.reset();
            }
            return Ok(false);
        }
        if ctrl && key.code == KeyCode::Char('x') {
            self.prefix = true;
            self.message.clear();
            return Ok(false);
        }
        match key.code {
            KeyCode::Char('0') if alt => {
                self.today = true;
                self.active_view = None;
                self.query.clear();
                self.help = false;
                self.refresh(store)?;
                self.reset();
            }
            KeyCode::Char('g') if key.modifiers.is_empty() => {
                self.today = true;
                self.active_view = None;
                self.query.clear();
                self.help = false;
                self.refresh(store)?;
                self.reset();
            }
            KeyCode::Char('v') if key.modifiers.is_empty() => {
                self.view_manager = Some(views::Manager::new(store, None)?)
            }
            KeyCode::Char('V') => {
                let mut view = self
                    .active_view
                    .clone()
                    .unwrap_or_else(workflow::View::blank);
                view.id.clear();
                view.name.clear();
                if self.active_view.is_none() {
                    view.kind = self.kinds[self.tab].as_str().into();
                    view.query = self.query.clone();
                } else if !self.query.is_empty() {
                    view.filters.push(workflow::Filter {
                        field: "*".into(),
                        op: workflow::Op::Contains,
                        value: self.query.clone(),
                    });
                }
                self.view_manager = Some(views::Manager::new(store, Some(view))?);
            }
            KeyCode::Char('l') if key.modifiers.is_empty() => {
                if let Some(record) = self.current() {
                    self.timeline = Some(store.timeline(record["id"].as_str().unwrap_or(""))?);
                    self.scroll = 0;
                }
            }
            KeyCode::Char('a' | 'f') if key.modifiers.is_empty() => {
                if let Some(record) = self.current() {
                    self.related_form(
                        store,
                        if key.code == KeyCode::Char('a') {
                            "note"
                        } else {
                            "task"
                        },
                        &record,
                    )?;
                }
            }
            KeyCode::Char('x') if key.modifiers.is_empty() => {
                if let Some(record) = self.current() {
                    if record["kind"] == "task" {
                        store.execute(&[
                            "update".into(),
                            record["id"].as_str().unwrap_or("").into(),
                            json!({"status":"done","completed_at":workflow::date().to_string()})
                                .to_string(),
                        ])?;
                        self.refresh(store)?;
                        self.message = "Task completed. Today and saved views updated.".into();
                    } else {
                        self.message = "Select a task to mark it done.".into();
                    }
                }
            }
            KeyCode::Char('u') if key.modifiers.is_empty() => {
                match store.undo_delete() {
                    Ok(record) => self.restored(store, &record)?,
                    Err(e) => self.message = e.to_string(),
                };
            }
            KeyCode::Char('b') if key.modifiers.is_empty() => {
                self.tool = Some(tools::Tool::trash(store)?)
            }
            KeyCode::Char('c') if key.modifiers.is_empty() => {
                self.tool = Some(tools::Tool::new_type())
            }
            KeyCode::Char('i') if key.modifiers.is_empty() => {
                self.tool = Some(tools::Tool::import(
                    store,
                    if self.tab == 0 {
                        None
                    } else {
                        Some(self.kinds[self.tab].clone())
                    },
                )?)
            }
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('x') if alt => {
                self.help = false;
                self.palette = Some(tools::Commands::new(&store.types()?));
            }
            KeyCode::Char(':') => {
                self.help = false;
                self.begin(Mode::Command);
            }
            KeyCode::Char('s') if ctrl => {
                self.help = false;
                self.begin(Mode::Search);
            }
            KeyCode::Char('/') => {
                self.help = false;
                self.begin(Mode::Search);
            }
            KeyCode::Char('h') if ctrl => {
                self.help = !self.help;
                self.scroll = 0;
            }
            KeyCode::Char('?') | KeyCode::F(1) => {
                self.help = !self.help;
                self.scroll = 0;
            }
            KeyCode::Esc => self.cancel(),
            KeyCode::Char('T') => self.set_theme(store, self.theme.next())?,
            KeyCode::Char('s') if key.modifiers.is_empty() => {
                let mut entry = form::Form::schema_editor(
                    if self.tab == 0 {
                        None
                    } else {
                        Some(self.kinds[self.tab].as_str())
                    },
                    &self.records,
                );
                entry.restore_schema(store)?;
                self.form = Some(entry);
                self.help = false;
            }
            KeyCode::Char('n') if !ctrl && !alt => {
                self.help = false;
                let mut entry = form::Form::new(
                    if self.tab == 0 {
                        None
                    } else {
                        Some(self.kinds[self.tab].as_str())
                    },
                    None,
                    &self.records,
                );
                entry.restore_schema(store)?;
                self.form = Some(entry);
            }
            KeyCode::Char('d') if key.modifiers.is_empty() && !self.details_focus && !self.help => {
                if let Some(record) = self.current() {
                    let id = record["id"].as_str().context("Selected record has no ID")?;
                    match store.execute(&["delete".into(), id.into()]) {
                        Ok(_) => {
                            self.refresh(store)?;
                            self.selected =
                                self.selected.min(self.visible().len().saturating_sub(1));
                            self.scroll = 0;
                            self.message = format!(
                                "Moved {} to trash • u restores • b opens trash",
                                record["name"].as_str().unwrap_or("record")
                            );
                        }
                        Err(error) => self.message = format!("Could not delete record: {error}"),
                    }
                } else {
                    self.message = "No record selected to delete.".into();
                }
            }
            KeyCode::Char('e') | KeyCode::Enter if !ctrl && !alt => {
                if let Some(r) = self.current() {
                    let mut entry = form::Form::new(None, Some(&r), &self.records);
                    entry.restore_schema(store)?;
                    self.form = Some(entry);
                }
            }
            KeyCode::Char(digit @ '1'..='9')
                if alt && !ctrl && (digit as u8 - b'1') < self.kinds.len() as u8 =>
            {
                self.category((digit as u8 - b'1') as usize);
                self.help = false;
                self.message.clear();
                self.reset();
            }
            KeyCode::Tab => {
                self.category((self.tab + 1) % self.kinds.len());
                self.help = false;
                self.reset();
            }
            KeyCode::BackTab => {
                self.category((self.tab + self.kinds.len() - 1) % self.kinds.len());
                self.help = false;
                self.reset();
            }
            KeyCode::Char('<') if alt => {
                self.selected = 0;
                self.scroll = 0;
            }
            KeyCode::Char('>') if alt => {
                self.selected = self.visible().len().saturating_sub(1);
                self.scroll = 0;
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Char('v') if ctrl => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Char('v') if alt => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Down | KeyCode::Char('n') if key.code == KeyCode::Down || ctrl => {
                if self.details_focus || self.help {
                    self.scroll = self.scroll.saturating_add(1);
                } else {
                    self.selected = (self.selected + 1).min(self.visible().len().saturating_sub(1));
                    self.scroll = 0;
                }
            }
            KeyCode::Up | KeyCode::Char('p') if key.code == KeyCode::Up || ctrl => {
                if self.details_focus || self.help {
                    self.scroll = self.scroll.saturating_sub(1);
                } else {
                    self.selected = self.selected.saturating_sub(1);
                    self.scroll = 0;
                }
            }
            _ => {}
        }
        Ok(false)
    }
    fn context_help(&self) -> &'static str {
        if self.prefix {
            " Next key: b category  o focus  C-c quit  C-g/Esc cancel"
        } else if self.mode != Mode::Browse {
            " C-a/e start/end  C-b/f move  C-k kill  C-y yank  C-g cancel  C-p/n history"
        } else if self.help || self.details_focus {
            " n New  e Edit  s Schema  C-n/p scroll  C-v/M-v page  C-x o focus  ? help"
        } else {
            " M-x Actions  g Today  v Views  l Timeline  n New  e Edit  u Undo  ?"
        }
    }
    fn render(&mut self, frame: &mut Frame) {
        self.render_content(frame);
        self.render_actions(frame);
        if let Some(palette) = self.palette.as_ref() {
            palette.render(frame, self.theme.palette());
        }
    }
    fn render_content(&mut self, frame: &mut Frame) {
        let p = self.theme.palette();
        if (self.palette.is_some() || self.tool.is_some())
            && (frame.area().width < 45 || frame.area().height < 12)
        {
            frame.render_widget(
                Paragraph::new("Enlarge the terminal to at least 45 × 12. Esc goes back.")
                    .style(p.base()),
                frame.area(),
            );
            return;
        }
        if let Some(tool) = self.tool.as_mut() {
            tool.render(frame, p);
            return;
        }
        if self.today && self.agenda_date != workflow::date() {
            self.agenda_date = workflow::date();
            self.agenda = workflow::agenda(&self.records, self.agenda_date);
            self.selected = self.selected.min(self.visible().len().saturating_sub(1));
        }
        if let Some(manager) = self.view_manager.as_mut() {
            manager.render(frame, p);
            return;
        }
        if let Some(timeline) = self.timeline.as_ref() {
            let areas =
                Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(frame.area());
            let text = timeline_text(timeline, &self.records);
            let paragraph = Paragraph::new(text)
                .style(p.base())
                .wrap(Wrap { trim: false });
            let max = paragraph
                .line_count(areas[0].width.saturating_sub(2))
                .saturating_sub(areas[0].height.saturating_sub(2) as usize)
                .min(u16::MAX as usize) as u16;
            self.scroll = self.scroll.min(max);
            frame.render_widget(
                paragraph.scroll((self.scroll, 0)).block(
                    Block::bordered()
                        .title(" Unified activity timeline ")
                        .border_style(p.base().fg(p.accent)),
                ),
                areas[0],
            );
            frame.render_widget(Paragraph::new("C-n/p / PgUp/PgDn: scroll • a: log note • f: follow-up task\nEsc / C-g / l: back").style(p.base().fg(p.muted)),areas[1]);
            return;
        }
        if let Some(form) = self.form.as_mut() {
            form.render(frame, p, &self.records);
            return;
        }
        frame.render_widget(Block::default().style(p.base()), frame.area());
        if frame.area().width < 45 || frame.area().height < 10 {
            frame.render_widget(
                Paragraph::new(
                    "Please enlarge the terminal to at least 45 × 10.\nC-x C-c or q to quit.",
                )
                .style(p.base()),
                frame.area(),
            );
            return;
        }
        let areas = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(if self.prefix { 4 } else { 3 }),
            Constraint::Length(1),
        ])
        .split(frame.area());
        let tabs: Vec<Span> = self
            .category_labels
            .iter()
            .enumerate()
            .map(|(i, name)| {
                Span::styled(
                    if i < 9 {
                        format!(" M-{} {name} ", i + 1)
                    } else {
                        format!(" {name} [Tab] ")
                    },
                    if i == self.tab {
                        p.base().fg(p.accent).add_modifier(Modifier::BOLD)
                    } else {
                        p.base().fg(p.muted)
                    },
                )
            })
            .collect();
        let tab_end: usize = self
            .category_labels
            .iter()
            .enumerate()
            .take(self.tab + 1)
            .map(|(i, label)| {
                Line::from(if i < 9 {
                    format!(" M-{} {label} ", i + 1)
                } else {
                    format!(" {label} [Tab] ")
                })
                .width()
            })
            .sum();
        let tab_scroll = tab_end
            .saturating_sub(areas[0].width.saturating_sub(2) as usize)
            .min(u16::MAX as usize) as u16;
        frame.render_widget(
            Paragraph::new(Line::from(tabs))
                .scroll((0, tab_scroll))
                .block(
                    Block::bordered()
                        .title(format!(
                            " CRiMe • {} • {} ",
                            self.theme.name(),
                            if self.today {
                                format!("Today {}", workflow::date())
                            } else if let Some(view) = &self.active_view {
                                view.name.clone()
                            } else {
                                "Records".into()
                            }
                        ))
                        .border_style(p.base().fg(p.muted)),
                )
                .style(p.base()),
            areas[0],
        );
        if self.help {
            let help = Paragraph::new(HELP)
                .style(p.base())
                .wrap(Wrap { trim: false });
            let max_scroll = help
                .line_count(areas[1].width.saturating_sub(2))
                .saturating_sub(areas[1].height.saturating_sub(2) as usize)
                .min(u16::MAX as usize) as u16;
            self.scroll = self.scroll.min(max_scroll);
            frame.render_widget(
                help.scroll((self.scroll, 0)).block(
                    Block::bordered()
                        .title(" Key guide • C-v / M-v scroll • C-g close ")
                        .border_style(p.base().fg(p.accent)),
                ),
                areas[1],
            );
        } else {
            let visible = self.visible();
            let panes =
                Layout::horizontal([Constraint::Percentage(34), Constraint::Percentage(66)])
                    .split(areas[1]);
            let items: Vec<ListItem> = visible
                .iter()
                .map(|r| {
                    let mut lines = vec![Line::from(Span::styled(
                        format!(
                            "{}{}",
                            if self.marked.contains(r["id"].as_str().unwrap_or("")) {
                                "[✓] "
                            } else {
                                ""
                            },
                            r["name"].as_str().unwrap_or("Untitled")
                        ),
                        p.base().add_modifier(Modifier::BOLD),
                    ))];
                    if self.today {
                        if let Some(item) =
                            self.agenda.iter().find(|item| item.record["id"] == r["id"])
                        {
                            lines.push(Line::from(Span::styled(
                                format!("  {}", item.section),
                                p.base().fg(p.accent),
                            )));
                            lines.push(Line::from(Span::styled(
                                format!("  {}", item.reason),
                                p.base().fg(p.muted),
                            )));
                        }
                    } else if let Some(view) = &self.active_view {
                        for field in &view.columns {
                            let label = details::label(
                                &field.trim_start_matches("/data/").replace('/', " › "),
                            );
                            let value = workflow::friendly(
                                &workflow::field(r, field, &self.records),
                                &self.records,
                            );
                            lines.push(Line::from(Span::styled(
                                format!("  {label}: {value}"),
                                p.base().fg(p.muted),
                            )));
                        }
                    } else {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", details::label(r["kind"].as_str().unwrap_or(""))),
                            p.base().fg(p.muted),
                        )));
                    }
                    ListItem::new(lines)
                })
                .collect();
            let mut state = ListState::default().with_selected(if visible.is_empty() {
                None
            } else {
                Some(self.selected.min(visible.len() - 1))
            });
            frame.render_stateful_widget(
                List::new(items)
                    .style(p.base())
                    .block(
                        Block::bordered()
                            .title(format!(
                                " {} ({}) ",
                                if self.today {
                                    "Today actions"
                                } else {
                                    "Records"
                                },
                                visible.len()
                            ))
                            .border_style(p.base().fg(if self.details_focus {
                                p.muted
                            } else {
                                p.accent
                            })),
                    )
                    .highlight_style(p.base().bg(p.selection))
                    .highlight_symbol("› "),
                panes[0],
                &mut state,
            );
            let mut text=visible.get(self.selected).map(|r|details::record(r,&self.records,p)).unwrap_or_else(||Text::from(if self.today{"You're up to date.\nNo matching actions need attention.\ng: Refresh Today • v: Saved views • M-1: All records\nn: Create a task or contact"}else{"No matching records.\n\nn: Create a record with a guided form.\ne: Edit a selected record.\nC-g: Clear the search.\n?: Show the key guide."}));
            if self.today
                && let Some(record) = visible.get(self.selected)
                && let Some(item) = self
                    .agenda
                    .iter()
                    .find(|item| item.record["id"] == record["id"])
            {
                text.lines.splice(
                    0..0,
                    [
                        Line::from(Span::styled(item.section.clone(), p.base().fg(p.accent))),
                        Line::from(item.reason.clone()),
                        Line::from(""),
                    ],
                );
            }
            let paragraph = Paragraph::new(text)
                .style(p.base())
                .wrap(Wrap { trim: false });
            let max_scroll = paragraph
                .line_count(panes[1].width.saturating_sub(2))
                .saturating_sub(panes[1].height.saturating_sub(2) as usize)
                .min(u16::MAX as usize) as u16;
            self.scroll = self.scroll.min(max_scroll);
            frame.render_widget(
                paragraph.scroll((self.scroll, 0)).block(
                    Block::bordered()
                        .title(" Details ")
                        .border_style(p.base().fg(if self.details_focus {
                            p.accent
                        } else {
                            p.muted
                        })),
                ),
                panes[1],
            );
        }
        let title = match self.mode {
            Mode::Browse => " Status ",
            Mode::Command => " M-x command • Enter accepts • C-g cancels ",
            Mode::Search => " Search • Enter keeps • C-g cancels ",
        };
        let text = if self.mode == Mode::Browse {
            if self.prefix {
                PREFIX_HINT.into()
            } else if !self.query.is_empty() {
                format!("Filter: {} • {}", self.query, self.message)
            } else {
                self.message.clone()
            }
        } else {
            self.editor.text()
        };
        let inner_width = areas[2].width.saturating_sub(2) as usize;
        let mut start = 0;
        let mut cursor_width = 0;
        if self.mode != Mode::Browse {
            while start < self.editor.cursor
                && Line::from(
                    self.editor.chars[start..self.editor.cursor]
                        .iter()
                        .collect::<String>(),
                )
                .width()
                    >= inner_width
            {
                start += 1;
            }
            cursor_width = Line::from(
                self.editor.chars[start..self.editor.cursor]
                    .iter()
                    .collect::<String>(),
            )
            .width();
        }
        let shown = if self.mode == Mode::Browse {
            text
        } else {
            self.editor.chars[start..].iter().collect()
        };
        let status = Paragraph::new(shown).style(p.base());
        let status = if self.prefix {
            status.wrap(Wrap { trim: false })
        } else {
            status
        };
        frame.render_widget(
            status.block(
                Block::bordered()
                    .title(title)
                    .border_style(p.base().fg(p.accent)),
            ),
            areas[2],
        );
        if self.mode != Mode::Browse {
            frame.set_cursor_position((areas[2].x + 1 + cursor_width as u16, areas[2].y + 1));
        }
        frame.render_widget(
            Paragraph::new(self.context_help()).style(p.base().fg(p.muted)),
            areas[3],
        );
    }
}
fn timeline_text(timeline: &Value, records: &[Value]) -> String {
    let mut lines = vec![
        timeline["record"]["name"]
            .as_str()
            .unwrap_or("Record")
            .to_owned(),
        "".into(),
        "OPEN FOLLOW-UPS".into(),
    ];
    if let Some(tasks) = timeline["open_tasks"].as_array() {
        if tasks.is_empty() {
            lines.push("No open tasks. Press f to schedule the next action.".into());
        }
        for task in tasks {
            lines.push(format!(
                "• {} • due {}",
                task["name"].as_str().unwrap_or("Task"),
                workflow::friendly(&task["data"]["due"], records)
            ));
        }
    }
    lines.extend(["".into(), "HISTORY • newest first".into()]);
    if let Some(events) = timeline["events"].as_array() {
        for event in events {
            lines.push(format!(
                "{} • {} {} • {}",
                event["at"].as_str().unwrap_or(""),
                details::label(event["kind"].as_str().unwrap_or("")),
                event["action"].as_str().unwrap_or(""),
                event["name"].as_str().unwrap_or("")
            ));
            if let Some(changes) = event["changes"].as_array() {
                for field in changes.iter().filter_map(Value::as_str) {
                    let value = if field == "name" {
                        event["record"]["name"].clone()
                    } else {
                        event["record"]["data"][field].clone()
                    };
                    lines.push(format!(
                        "  {}: {}",
                        details::label(field),
                        workflow::friendly(&value, records)
                    ));
                }
            }
            if let Some(body) = event["record"]["data"]["body"].as_str() {
                for line in body.lines() {
                    lines.push(format!("  {line}"));
                }
            }
            if event["kind"] == "task" {
                lines.push(format!(
                    "  Status: {} • Due: {}",
                    workflow::friendly(&event["record"]["data"]["status"], records),
                    workflow::friendly(&event["record"]["data"]["due"], records)
                ));
            }
            lines.push("".into());
        }
    }
    lines.join("\n")
}
pub fn run(store: &Store, theme: Option<Theme>) -> Result<()> {
    let mut app = App::new(store, theme)?;
    app.today = true;
    let mut terminal = ratatui::init();
    let result = (|| -> Result<()> {
        let mut effect = fx::fade_from_fg(app.theme.palette().bg, (250, Interpolation::SineOut));
        let mut last = std::time::Instant::now();
        loop {
            let elapsed = last.elapsed();
            last = std::time::Instant::now();
            terminal.draw(|frame| {
                app.render(frame);
                frame.render_effect(&mut effect, frame.area(), elapsed.into());
            })?;
            if event::poll(std::time::Duration::from_millis(33))?
                && let Event::Key(key) = event::read()?
                && key.kind != KeyEventKind::Release
                && app.handle(store, key)?
            {
                break;
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result
}
fn load(store: &Store) -> Result<Vec<Value>> {
    Ok(store
        .execute(&["list".into()])?
        .as_array()
        .context("Expected record list")?
        .clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    fn key(app: &mut App, store: &Store, c: char, m: KeyModifiers) -> bool {
        app.handle(store, KeyEvent::new(KeyCode::Char(c), m))
            .unwrap()
    }
    fn enter(app: &mut App, store: &Store) {
        app.handle(store, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
    }
    fn status(app: &mut App, width: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, 20)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buffer = terminal.backend().buffer();
        (15..19)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn prefix_choices_render_before_action_and_clear_afterward() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let mut app = App::new(&store, None).unwrap();
        key(&mut app, &store, 'x', KeyModifiers::CONTROL);
        assert_eq!(app.tab, 0);
        let before = status(&mut app, 80);
        assert!(
            before.contains("b category"),
            "missing choices before action: {before}"
        );
        assert!(before.contains("o focus"));
        key(&mut app, &store, 'b', KeyModifiers::NONE);
        assert_eq!(app.tab, 1);
        let after = status(&mut app, 80);
        assert!(!after.contains("b category"), "stale prefix help: {after}");
        assert!(!after.contains("C-x …"));
        assert!(app.message.is_empty());
    }
    #[test]
    fn pending_help_survives_filter_and_narrow_width_then_cancels_cleanly() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let mut app = App::new(&store, None).unwrap();
        app.query = "a long active search that must not hide pending choices".into();
        for cancel in [KeyCode::Esc, KeyCode::Char('g')] {
            key(&mut app, &store, 'x', KeyModifiers::CONTROL);
            let pending = status(&mut app, 45);
            assert!(pending.contains("b category"));
            assert!(pending.contains("o focus"));
            assert!(pending.contains("C-c quit"));
            assert!(pending.contains("C-g cancel"));
            app.handle(
                &store,
                KeyEvent::new(
                    cancel,
                    if cancel == KeyCode::Esc {
                        KeyModifiers::NONE
                    } else {
                        KeyModifiers::CONTROL
                    },
                ),
            )
            .unwrap();
            assert!(!app.prefix);
            assert_eq!(app.tab, 0);
            assert!(!status(&mut app, 45).contains("b category"));
        }
        key(&mut app, &store, 'x', KeyModifiers::CONTROL);
        key(&mut app, &store, 'o', KeyModifiers::NONE);
        assert!(app.details_focus);
        assert!(app.message.is_empty());
        assert!(app.context_help().contains("C-n/p scroll"));
        key(&mut app, &store, 's', KeyModifiers::CONTROL);
        assert!(app.context_help().contains("C-a/e start/end"));
    }
    #[test]
    fn meta_numbers_jump_to_every_category_and_reset_navigation() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let mut app = App::new(&store, None).unwrap();
        for (digit, category) in [('6', 5), ('2', 1), ('5', 4), ('3', 2), ('1', 0), ('4', 3)] {
            app.selected = 10;
            app.scroll = 10;
            app.help = true;
            key(&mut app, &store, digit, KeyModifiers::ALT);
            assert_eq!(app.tab, category);
            assert_eq!(app.selected, 0);
            assert_eq!(app.scroll, 0);
            assert!(!app.help);
        }
        for (digit, modifiers) in [
            ('1', KeyModifiers::NONE),
            ('0', KeyModifiers::ALT),
            ('7', KeyModifiers::ALT),
            ('9', KeyModifiers::ALT),
        ] {
            key(&mut app, &store, digit, modifiers);
            assert_eq!(app.tab, 3);
        }
        app.begin(Mode::Command);
        app.editor.set("unfinished command");
        key(&mut app, &store, '2', KeyModifiers::ALT);
        assert_eq!(app.tab, 3);
        assert_eq!(app.editor.text(), "unfinished command");
        app.cancel();
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buffer = terminal.backend().buffer();
        let tabs = (0..100)
            .map(|x| buffer[(x, 1)].symbol())
            .collect::<String>();
        for (index, name) in CATEGORIES.iter().enumerate() {
            assert!(tabs.contains(&format!("M-{} {name}", index + 1)));
        }
    }
    #[test]
    fn standalone_ctrl_c_is_reserved_in_every_mode_but_quit_chord_still_works() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let mut app = App::new(&store, None).unwrap();
        for mode in [Mode::Browse, Mode::Command, Mode::Search] {
            app.mode = mode;
            app.editor.set("unfinished text");
            app.editor.cursor = 4;
            app.query = "active filter".into();
            assert!(!key(&mut app, &store, 'c', KeyModifiers::CONTROL));
            assert!(app.mode == mode);
            assert_eq!(app.editor.text(), "unfinished text");
            assert_eq!(app.editor.cursor, 4);
            assert_eq!(app.query, "active filter");
        }
        app.cancel();
        key(&mut app, &store, 'x', KeyModifiers::CONTROL);
        assert!(key(&mut app, &store, 'c', KeyModifiers::CONTROL));
        app.prefix = false;
        key(&mut app, &store, 'x', KeyModifiers::CONTROL);
        assert!(!key(&mut app, &store, 'c', KeyModifiers::NONE));
        key(&mut app, &store, 'x', KeyModifiers::CONTROL);
        key(&mut app, &store, 'g', KeyModifiers::CONTROL);
        assert!(!key(&mut app, &store, 'c', KeyModifiers::CONTROL));
        assert!(key(&mut app, &store, 'q', KeyModifiers::NONE));
    }
    #[test]
    fn d_deletes_filtered_selection_and_handles_last_and_empty_records() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        for (kind, name) in [
            ("contact", "Target first"),
            ("contact", "Target second"),
            ("contact", "Other"),
            ("company", "Target company"),
        ] {
            store
                .execute(&["add".into(), kind.into(), name.into()])
                .unwrap();
        }
        let mut app = App::new(&store, None).unwrap();
        app.tab = 1;
        app.query = "Target".into();
        app.selected = 1;
        let id = app.current().unwrap()["id"].as_str().unwrap().to_owned();
        key(&mut app, &store, 'd', KeyModifiers::NONE);
        assert!(store.execute(&["get".into(), id]).is_err());
        assert_eq!(app.tab, 1);
        assert_eq!(app.query, "Target");
        assert_eq!(app.visible().len(), 1);
        assert_eq!(app.selected, 0);
        key(&mut app, &store, 'd', KeyModifiers::NONE);
        assert!(app.current().is_none());
        key(&mut app, &store, 'd', KeyModifiers::NONE);
        assert!(app.message.contains("No record"));
        assert_eq!(
            store
                .execute(&["export".into()])
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn d_does_not_delete_outside_records_focus_or_when_modified() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        store
            .execute(&["add".into(), "contact".into(), "Ada".into()])
            .unwrap();
        let mut app = App::new(&store, None).unwrap();
        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SUPER,
        ] {
            key(&mut app, &store, 'd', modifiers);
        }
        app.details_focus = true;
        key(&mut app, &store, 'd', KeyModifiers::NONE);
        app.details_focus = false;
        app.help = true;
        key(&mut app, &store, 'd', KeyModifiers::NONE);
        app.help = false;
        key(&mut app, &store, 'x', KeyModifiers::CONTROL);
        key(&mut app, &store, 'd', KeyModifiers::NONE);
        for mode in [Mode::Command, Mode::Search] {
            app.begin(mode);
            key(&mut app, &store, 'd', KeyModifiers::NONE);
            assert_eq!(app.editor.text(), "d");
            app.cancel();
        }
        key(&mut app, &store, 'e', KeyModifiers::NONE);
        enter(&mut app, &store);
        key(&mut app, &store, 'd', KeyModifiers::NONE);
        assert!(app.form.is_some());
        assert_eq!(
            store
                .execute(&["export".into()])
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn today_completion_updates_views_and_timeline_actions_link_to_the_contact() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let contact = store
            .execute(&["add".into(), "contact".into(), "Ada".into()])
            .unwrap();
        let task = store
            .execute(&[
                "add".into(),
                "task".into(),
                "Overdue follow-up".into(),
                json!({"contact_id":contact["id"],"due":"2000-01-01"}).to_string(),
            ])
            .unwrap();
        let mut app = App::new(&store, None).unwrap();
        app.today = true;
        assert_eq!(app.current().unwrap()["id"], task["id"]);
        key(&mut app, &store, 'x', KeyModifiers::NONE);
        assert!(app.today);
        assert!(workflow::closed(
            &store.get(task["id"].as_str().unwrap()).unwrap()
        ));
        assert!(
            !app.agenda
                .iter()
                .any(|item| item.record["id"] == task["id"])
        );
        app.category(1);
        app.selected = 0;
        key(&mut app, &store, 'l', KeyModifiers::NONE);
        assert!(app.timeline.is_some());
        key(&mut app, &store, 'f', KeyModifiers::NONE);
        assert!(app.form.is_some());
        enter(&mut app, &store);
        for c in "Next call".chars() {
            key(&mut app, &store, c, KeyModifiers::NONE);
        }
        enter(&mut app, &store);
        app.handle(&store, KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))
            .unwrap();
        let timeline = app
            .timeline
            .as_ref()
            .expect("Return to the contact timeline after saving a follow-up");
        assert_eq!(timeline["record"]["id"], contact["id"]);
        assert!(
            timeline["open_tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["name"] == "Next call" && r["data"]["contact_id"] == contact["id"])
        );
        let rendered = timeline_text(timeline, &app.records);
        assert!(rendered.contains("Next call"));
        assert!(rendered.contains("done"));
        assert!(!rendered.contains("\"data\""));
    }
    #[test]
    fn new_type_flow_creates_a_category_schema_and_palette_navigation() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let mut app = App::new(&store, None).unwrap();
        key(&mut app, &store, 'c', KeyModifiers::NONE);
        for c in "Projects".chars() {
            key(&mut app, &store, c, KeyModifiers::NONE);
        }
        enter(&mut app, &store);
        enter(&mut app, &store);
        assert!(app.form.is_some());
        assert_eq!(app.kinds.last().unwrap(), "projects");
        app.handle(&store, KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))
            .unwrap();
        key(&mut app, &store, 'n', KeyModifiers::NONE);
        enter(&mut app, &store);
        for c in "Launch".chars() {
            key(&mut app, &store, c, KeyModifiers::NONE);
        }
        enter(&mut app, &store);
        app.handle(&store, KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.current().unwrap()["kind"], "projects");
        key(&mut app, &store, '1', KeyModifiers::ALT);
        key(&mut app, &store, 'x', KeyModifiers::ALT);
        for c in "projects".chars() {
            key(&mut app, &store, c, KeyModifiers::NONE);
        }
        enter(&mut app, &store);
        assert_eq!(app.tab, 6);
        assert!(app.palette.is_none());
        key(&mut app, &store, 'd', KeyModifiers::NONE);
        assert!(app.current().is_none());
        key(&mut app, &store, 'u', KeyModifiers::NONE);
        assert_eq!(app.current().unwrap()["name"], "Launch");
    }
    #[test]
    fn emacs_navigation_search_create_rename_and_theme_persistence() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        for name in ["Ada", "Grace"] {
            store
                .execute(&["add".into(), "contact".into(), name.into()])
                .unwrap();
        }
        let mut app = App::new(&store, Some(Theme::TokyoNight)).unwrap();
        key(&mut app, &store, 'n', KeyModifiers::CONTROL);
        assert_eq!(app.selected, 1);
        key(&mut app, &store, 'p', KeyModifiers::CONTROL);
        assert_eq!(app.selected, 0);
        key(&mut app, &store, 's', KeyModifiers::CONTROL);
        app.editor.set("Grace");
        key(&mut app, &store, 'e', KeyModifiers::CONTROL);
        assert_eq!(app.visible().len(), 1);
        enter(&mut app, &store);
        assert_eq!(app.current().unwrap()["name"], "Grace");
        key(&mut app, &store, 'g', KeyModifiers::CONTROL);
        assert_eq!(app.visible().len(), 2);
        key(&mut app, &store, 'n', KeyModifiers::NONE);
        enter(&mut app, &store); // Choose Contact in the type picker.
        enter(&mut app, &store); // Edit required name.
        for c in "東京".chars() {
            key(&mut app, &store, c, KeyModifiers::NONE);
        }
        enter(&mut app, &store);
        app.handle(&store, KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.current().unwrap()["name"], "東京");
        key(&mut app, &store, 'e', KeyModifiers::NONE);
        enter(&mut app, &store);
        key(&mut app, &store, 'a', KeyModifiers::CONTROL);
        key(&mut app, &store, 'k', KeyModifiers::CONTROL);
        for c in "Tokyo".chars() {
            key(&mut app, &store, c, KeyModifiers::NONE);
        }
        enter(&mut app, &store);
        app.handle(&store, KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.current().unwrap()["name"], "Tokyo");
        key(&mut app, &store, 'T', KeyModifiers::SHIFT);
        assert_eq!(app.theme, Theme::Dracula);
        assert_eq!(App::new(&store, None).unwrap().theme, Theme::Dracula);
        key(&mut app, &store, 'x', KeyModifiers::CONTROL);
        key(&mut app, &store, 'o', KeyModifiers::NONE);
        assert!(app.details_focus);
        key(&mut app, &store, 'x', KeyModifiers::CONTROL);
        assert!(key(&mut app, &store, 'c', KeyModifiers::CONTROL));
    }
    #[test]
    fn all_themes_render_readable_details_and_small_terminals() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        store
            .execute(&[
                "add".into(),
                "contact".into(),
                "Ada Lovelace".into(),
                r#"{"email":"ada@example.com","tags":["VIP","customer"]}"#.into(),
            ])
            .unwrap();
        let mut app = App::new(&store, None).unwrap();
        for theme in Theme::ALL {
            app.theme = theme;
            let mut terminal = Terminal::new(TestBackend::new(100, 36)).unwrap();
            terminal.draw(|f| app.render(f)).unwrap();
            let buffer = terminal.backend().buffer();
            let text = buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("Ada Lovelace"));
            assert!(text.contains("Email"));
            assert!(text.contains("ada@example.com"));
            assert!(text.contains("VIP · customer"));
            assert!(!text.contains("\"data\""));
            assert_eq!(buffer[(0, 0)].bg, theme.palette().bg);
        }
        let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("Please enlarge"));
    }
}
