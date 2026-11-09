use super::{
    editor::Editor,
    theme::{Palette, Theme},
};
use crate::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Block, Clear, List, ListState, Paragraph, Wrap},
};
#[derive(Clone)]
pub enum Action {
    Related,
    Back,
    Bulk,
    Merge,
    Select,
    Today,
    New,
    Edit,
    Delete,
    Undo,
    Trash,
    Import,
    NewType,
    Schema,
    Views,
    SaveView,
    Timeline,
    Note,
    FollowUp,
    Done,
    Theme,
    SetTheme(Theme),
    Help,
    Raw(String),
    Category(usize),
    Quit,
}
struct Entry {
    name: String,
    help: String,
    key: String,
    action: Action,
}
pub struct Commands {
    entries: Vec<Entry>,
    input: Editor,
    selected: usize,
}
impl Commands {
    pub fn new(types: &[catalog::RecordType]) -> Self {
        let mut entries = vec![];
        for (name, help, key, action) in [
            (
                "Related records",
                "Follow links to companies, contacts, deals, tasks and notes",
                "r",
                Action::Related,
            ),
            (
                "Back to previous record",
                "Return after following a relationship",
                "B",
                Action::Back,
            ),
            (
                "Bulk actions",
                "Update, trash or export selected records",
                "U",
                Action::Bulk,
            ),
            (
                "Merge duplicate",
                "Review duplicate fields and preserve links and history",
                "M",
                Action::Merge,
            ),
            (
                "Toggle record selection",
                "Select records for bulk actions",
                "Space",
                Action::Select,
            ),
            (
                "Today",
                "Overdue tasks and next actions",
                "g / M-0",
                Action::Today,
            ),
            (
                "New record",
                "Open a guided data entry form",
                "n",
                Action::New,
            ),
            ("Edit record", "Edit the selected record", "e", Action::Edit),
            (
                "Delete record",
                "Move selected record to trash",
                "d",
                Action::Delete,
            ),
            (
                "Undo deletion",
                "Restore the most recently deleted record",
                "u",
                Action::Undo,
            ),
            (
                "Trash",
                "Browse and restore deleted records",
                "b",
                Action::Trash,
            ),
            (
                "Import CSV",
                "Map columns, preview and resolve duplicates",
                "i",
                Action::Import,
            ),
            (
                "New record type",
                "Create a custom category and its schema",
                "c",
                Action::NewType,
            ),
            (
                "Edit schema",
                "Fields, required values, choices and defaults",
                "s",
                Action::Schema,
            ),
            (
                "Saved views",
                "Open or edit saved filters and sorting",
                "v",
                Action::Views,
            ),
            (
                "Save current view",
                "Remember this search and category",
                "V",
                Action::SaveView,
            ),
            (
                "Activity timeline",
                "Notes, tasks and change history",
                "l",
                Action::Timeline,
            ),
            (
                "Log activity note",
                "Create a note linked to this record",
                "a",
                Action::Note,
            ),
            (
                "Schedule follow-up",
                "Create a linked task",
                "f",
                Action::FollowUp,
            ),
            (
                "Complete task",
                "Mark selected task done",
                "x",
                Action::Done,
            ),
            (
                "Cycle theme",
                "Choose another color theme",
                "T",
                Action::Theme,
            ),
            ("Key guide", "Browse keyboard shortcuts", "?", Action::Help),
            (
                "Advanced command",
                "Type an advanced CRM command",
                ":",
                Action::Raw(String::new()),
            ),
            ("Quit", "Close CRiMe", "C-x C-c", Action::Quit),
        ] {
            entries.push(Entry {
                name: name.into(),
                help: help.into(),
                key: key.into(),
                action,
            });
        }
        for theme in Theme::ALL {
            entries.push(Entry {
                name: format!("Theme: {}", theme.name()),
                help: "Switch to this theme".into(),
                key: String::new(),
                action: Action::SetTheme(theme),
            });
        }
        entries.push(Entry {
            name: "Go to All records".into(),
            help: "All categories".into(),
            key: "M-1".into(),
            action: Action::Category(0),
        });
        for (i, t) in types.iter().enumerate() {
            entries.push(Entry {
                name: format!("Go to {}", t.label),
                help: format!("{} records", t.slug),
                key: if i < 8 {
                    format!("M-{}", i + 2)
                } else {
                    "Tab".into()
                },
                action: Action::Category(i + 1),
            });
        }
        Self {
            entries,
            input: Editor::default(),
            selected: 0,
        }
    }
    fn matches(&self) -> Vec<&Entry> {
        let text = self.input.text().to_lowercase();
        let terms: Vec<_> = text.split_whitespace().collect();
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .filter(|e| {
                let haystack = format!("{} {} {}", e.name, e.help, e.key).to_lowercase();
                terms.iter().all(|t| haystack.contains(t))
            })
            .collect();
        entries.sort_by_key(|e| {
            let name = e.name.to_lowercase();
            if name == text || e.key.to_lowercase() == text {
                0
            } else if name.starts_with(&text) {
                1
            } else if terms.iter().all(|t| name.contains(t)) {
                2
            } else {
                3
            }
        });
        entries
    }

    pub fn handle(&mut self, key: KeyEvent) -> Option<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let len = self.matches().len();
        match key.code {
            KeyCode::Down | KeyCode::Char('n') if key.code == KeyCode::Down || ctrl => {
                self.selected = (self.selected + 1).min(len.saturating_sub(1))
            }
            KeyCode::Up | KeyCode::Char('p') if key.code == KeyCode::Up || ctrl => {
                self.selected = self.selected.saturating_sub(1)
            }
            KeyCode::Enter => {
                if let Some(entry) = self.matches().get(self.selected) {
                    return Some(entry.action.clone());
                }
                if !self.input.text().trim().is_empty() {
                    return Some(Action::Raw(self.input.text()));
                }
            }
            _ => {
                self.input.handle(key);
                self.selected = 0;
            }
        }
        None
    }
    pub fn render(&self, frame: &mut Frame, p: Palette) {
        let screen = frame.area();
        let height = screen.height.div_ceil(4);
        let area = Rect::new(screen.x, screen.bottom() - height, screen.width, height);
        frame.render_widget(Clear, area);
        let bordered = height >= 5;
        let areas = Layout::vertical([
            Constraint::Length(if bordered { 3 } else { 1 }),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
        let col = super::Line::from(
            self.input.chars[..self.input.cursor]
                .iter()
                .collect::<String>(),
        )
        .width();
        let inset = u16::from(bordered);
        let prefix = if bordered { "" } else { "M-x: " };
        let start = inset + prefix.len() as u16;
        let offset = col.saturating_sub(areas[0].width.saturating_sub(start + inset + 1) as usize);
        if areas[0].width > start + inset && areas[0].height > 0 {
            frame.set_cursor_position((
                areas[0].x + start + (col - offset) as u16,
                areas[0].y + inset,
            ));
        }
        let input = Paragraph::new(format!("{prefix}{}", self.input.text()))
            .scroll((0, offset.min(u16::MAX as usize) as u16))
            .style(p.base());
        frame.render_widget(
            if bordered {
                input.block(
                    Block::bordered()
                        .title(" Command palette • type to find an action ")
                        .border_style(p.base().fg(p.accent)),
                )
            } else {
                input
            },
            areas[0],
        );
        let labels: Vec<_> = self
            .matches()
            .iter()
            .map(|e| format!("{}  [{}] — {}", e.name, e.key, e.help))
            .collect();
        let labels = if labels.is_empty() {
            vec!["No matching action. Enter opens the advanced command input.".into()]
        } else {
            labels
        };
        let mut state = ListState::default().with_selected(Some(self.selected));
        frame.render_stateful_widget(
            List::new(labels)
                .style(p.base())
                .highlight_symbol("› ")
                .highlight_style(p.base().bg(p.selection)),
            areas[1],
            &mut state,
        );
        frame.render_widget(
            Paragraph::new("↑/↓ or C-n/p: choose • Enter: run • Esc/C-g: cancel")
                .style(p.base().fg(p.muted)),
            areas[2],
        );
    }
}
pub enum Tool {
    Trash {
        rows: Vec<Value>,
        selected: usize,
        confirm: bool,
        error: String,
    },
    NewType {
        input: Editor,
        label: String,
        slug_step: bool,
        error: String,
    },
    Import(Box<ImportScreen>),
}
pub enum Outcome {
    Continue,
    Close,
    Restored(Value),
    TypeCreated(catalog::RecordType),
    Imported { kind: String, summary: Value },
}
impl Tool {
    pub fn trash(store: &Store) -> Result<Self> {
        Ok(Self::Trash {
            rows: store.trash()?,
            selected: 0,
            confirm: false,
            error: String::new(),
        })
    }
    pub fn new_type() -> Self {
        Self::NewType {
            input: Editor::default(),
            label: String::new(),
            slug_step: false,
            error: String::new(),
        }
    }
    pub fn import(store: &Store, kind: Option<String>) -> Result<Self> {
        Ok(Self::Import(Box::new(ImportScreen::new(store, kind)?)))
    }
    pub fn handle(&mut self, store: &Store, key: KeyEvent) -> Result<Outcome> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match self {
            Self::Trash {
                rows,
                selected,
                confirm,
                error,
            } => {
                if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('g')) {
                    if *confirm {
                        *confirm = false;
                    } else {
                        return Ok(Outcome::Close);
                    }
                    return Ok(Outcome::Continue);
                }
                if *confirm {
                    if key.code == KeyCode::Char('y') && key.modifiers.is_empty() {
                        if let Some(row) = rows.get(*selected) {
                            store.purge_trash(row["record"]["id"].as_str().unwrap_or(""))?;
                            *rows = store.trash()?;
                            *selected = (*selected).min(rows.len().saturating_sub(1));
                        }
                        *confirm = false;
                    }
                    return Ok(Outcome::Continue);
                }
                match key.code {
                    KeyCode::Down | KeyCode::Char('n') if key.code == KeyCode::Down || ctrl => {
                        *selected = (*selected + 1).min(rows.len().saturating_sub(1))
                    }
                    KeyCode::Up | KeyCode::Char('p') if key.code == KeyCode::Up || ctrl => {
                        *selected = selected.saturating_sub(1)
                    }
                    KeyCode::Enter | KeyCode::Char('r') => {
                        if let Some(row) = rows.get(*selected) {
                            match store.restore(row["record"]["id"].as_str().unwrap_or("")) {
                                Ok(record) => return Ok(Outcome::Restored(record)),
                                Err(e) => *error = e.to_string(),
                            }
                        }
                    }
                    KeyCode::Char('d') if key.modifiers.is_empty() && !rows.is_empty() => {
                        *confirm = true
                    }
                    _ => {}
                }
            }
            Self::NewType {
                input,
                label,
                slug_step,
                error,
            } => {
                if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('g')) {
                    return Ok(Outcome::Close);
                }
                if key.code == KeyCode::Enter {
                    if !*slug_step {
                        let text = input.text().trim().to_owned();
                        if text.is_empty() {
                            *error =
                                "Enter a category name, for example Projects or Vendors.".into();
                        } else {
                            *label = text.clone();
                            let slug: String = text
                                .to_lowercase()
                                .chars()
                                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                                .collect();
                            input.set(slug.trim_matches('_'));
                            *slug_step = true;
                            error.clear();
                        }
                    } else {
                        match store.add_type(input.text().trim(), label) {
                            Ok(kind) => return Ok(Outcome::TypeCreated(kind)),
                            Err(e) => *error = e.to_string(),
                        }
                    }
                } else {
                    input.handle(key);
                }
            }
            Self::Import(screen) => return screen.handle(store, key),
        }
        Ok(Outcome::Continue)
    }
    pub fn render(&mut self, frame: &mut Frame, p: Palette) {
        match self {
            Self::Import(screen) => screen.render(frame, p),
            Self::Trash {
                rows,
                selected,
                confirm,
                error,
            } => {
                let areas = Layout::vertical([Constraint::Min(1), Constraint::Length(3)])
                    .split(frame.area());
                let labels: Vec<_> = rows
                    .iter()
                    .map(|r| {
                        format!(
                            "{} • {} • deleted {}",
                            r["record"]["name"].as_str().unwrap_or("Record"),
                            r["record"]["kind"].as_str().unwrap_or(""),
                            r["deleted_at"].as_str().unwrap_or("")
                        )
                    })
                    .collect();
                let mut state = ListState::default().with_selected(Some(*selected));
                frame.render_stateful_widget(
                    List::new(labels)
                        .style(p.base())
                        .highlight_symbol("› ")
                        .highlight_style(p.base().bg(p.selection))
                        .block(
                            Block::bordered()
                                .title(
                                    " Trash • records can be restored with original IDs and links ",
                                )
                                .border_style(p.base().fg(p.accent)),
                        ),
                    areas[0],
                    &mut state,
                );
                frame.render_widget(
                    Paragraph::new(if *confirm {
                        "Remove this recovery copy? y confirms; Esc cancels. History remains."
                            .into()
                    } else {
                        format!(
                            "Enter / r: restore • d: remove recovery copy • Esc/C-g: back\n{}",
                            if rows.is_empty() {
                                "Trash is empty."
                            } else {
                                error
                            }
                        )
                    })
                    .style(p.base())
                    .wrap(Wrap { trim: false }),
                    areas[1],
                );
            }
            Self::NewType {
                input,
                slug_step,
                error,
                ..
            } => {
                let help = if *slug_step {
                    "Stable type ID: lowercase letters, digits, _ or -. Enter creates the category and opens its schema editor."
                } else {
                    "Give the category a display name, for example Projects, Vendors or Books. Enter continues."
                };
                input_screen(
                    frame,
                    p,
                    input,
                    if *slug_step {
                        " New record type • ID "
                    } else {
                        " New record type • category name "
                    },
                    &format!("{help}\n{error}\nEsc/C-g: cancel"),
                );
            }
        }
    }
}
#[derive(PartialEq)]
enum ImportStep {
    Path,
    Kind,
    Map,
    Target,
    Policy,
    Match,
    Preview,
}
pub struct ImportScreen {
    step: ImportStep,
    input: Editor,
    kinds: Vec<catalog::RecordType>,
    kind: Option<String>,
    file: Option<importer::CsvFile>,
    config: Option<importer::Config>,
    prepared: Option<importer::Prepared>,
    selected: usize,
    choice: usize,
    options: Vec<(String, String)>,
    column: usize,
    report: String,
    scroll: u16,
    error: String,
}
impl ImportScreen {
    fn new(store: &Store, kind: Option<String>) -> Result<Self> {
        Ok(Self {
            step: ImportStep::Path,
            input: Editor::default(),
            kinds: store.types()?,
            kind,
            file: None,
            config: None,
            prepared: None,
            selected: 0,
            choice: 0,
            options: vec![],
            column: 0,
            report: String::new(),
            scroll: 0,
            error: String::new(),
        })
    }
    fn mapping(&mut self, store: &Store) -> Result<()> {
        let kind = self.kind.as_deref().context("Choose a record type")?;
        self.config = Some(importer::auto_config(
            store,
            kind,
            self.file.as_ref().context("Choose a CSV file")?,
        )?);
        self.step = ImportStep::Map;
        self.selected = 0;
        Ok(())
    }
    fn prepare(&mut self, store: &Store) -> Result<()> {
        let prepared = importer::prepare(
            store,
            self.file.as_ref().unwrap(),
            self.config.as_ref().unwrap(),
        )?;
        let mut lines = vec![
            format!(
                "Add {} • Merge {} • Skip {} • Errors {}",
                prepared.preview["add"],
                prepared.preview["merge"],
                prepared.preview["skip"],
                prepared.preview["errors"]
                    .as_array()
                    .map(Vec::len)
                    .unwrap_or(0)
            ),
            "Nothing has been imported. F10 commits only when there are no errors.".into(),
        ];
        if let Some(errors) = prepared.preview["errors"].as_array() {
            for e in errors.iter().take(50) {
                lines.push(format!(
                    "ERROR line {}: {}",
                    e["line"],
                    e["error"].as_str().unwrap_or("")
                ));
            }
        }
        if let Some(rows) = prepared.preview["rows"].as_array() {
            lines.push(format!(
                "Sample of first {} of {} planned rows:",
                rows.len().min(50),
                rows.len()
            ));
            for row in rows.iter().take(50) {
                lines.push(format!(
                    "Line {} • {} • {}",
                    row["line"],
                    row["name"].as_str().unwrap_or(""),
                    row["action"].as_str().unwrap_or("")
                ));
                if let Some(data) = row["data"].as_object() {
                    for (k, v) in data.iter().filter(|(_, v)| !v.is_null()).take(8) {
                        lines.push(format!("  {k}: {}", workflow::friendly(v, &[])));
                    }
                }
            }
        }
        self.report = lines.join("\n");
        self.prepared = Some(prepared);
        self.step = ImportStep::Preview;
        self.scroll = 0;
        Ok(())
    }
    fn handle(&mut self, store: &Store, key: KeyEvent) -> Result<Outcome> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        self.error.clear();
        if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('g')) {
            if self.step == ImportStep::Path {
                return Ok(Outcome::Close);
            }
            if matches!(
                self.step,
                ImportStep::Preview | ImportStep::Target | ImportStep::Match | ImportStep::Policy
            ) {
                self.step = ImportStep::Map;
            } else {
                return Ok(Outcome::Close);
            }
            return Ok(Outcome::Continue);
        }
        if self.step == ImportStep::Path {
            if key.code == KeyCode::Enter {
                match importer::read(&self.input.text()) {
                    Ok(file) => {
                        self.file = Some(file);
                        if self.kind.is_some() {
                            self.mapping(store)?;
                        } else {
                            self.choice = 0;
                            self.step = ImportStep::Kind;
                        }
                    }
                    Err(e) => self.error = e.to_string(),
                }
            } else {
                self.input.handle(key);
            }
            return Ok(Outcome::Continue);
        }
        if self.step == ImportStep::Preview {
            match key.code {
                KeyCode::PageDown | KeyCode::Down => self.scroll = self.scroll.saturating_add(5),
                KeyCode::PageUp | KeyCode::Up => self.scroll = self.scroll.saturating_sub(5),
                KeyCode::F(10) => match self.prepared.as_ref().unwrap().apply(store) {
                    Ok(summary) => {
                        return Ok(Outcome::Imported {
                            kind: self.config.as_ref().unwrap().kind.clone(),
                            summary,
                        });
                    }
                    Err(e) => self.error = e.to_string(),
                },
                _ => {}
            }
            return Ok(Outcome::Continue);
        }
        if key.code == KeyCode::F(10) && self.step == ImportStep::Map {
            if let Err(e) = self.prepare(store) {
                self.error = e.to_string();
            }
            return Ok(Outcome::Continue);
        }
        let next = key.code == KeyCode::Down
            || key.code == KeyCode::Tab
            || (ctrl && key.code == KeyCode::Char('n'));
        let prev = key.code == KeyCode::Up
            || key.code == KeyCode::BackTab
            || (ctrl && key.code == KeyCode::Char('p'));
        if self.step == ImportStep::Map {
            let count = self.file.as_ref().unwrap().headers.len() + 3;
            if next {
                self.selected = (self.selected + 1).min(count - 1);
            } else if prev {
                self.selected = self.selected.saturating_sub(1);
            } else if key.code == KeyCode::Enter {
                self.choice = 0;
                match self.selected {
                    0 => self.step = ImportStep::Kind,
                    1 => {
                        let config = self.config.as_ref().unwrap();
                        self.options = vec![("Record name / title".into(), "name".into())];
                        for target in config.mapping.values().filter(|p| *p != "name") {
                            if !self.options.iter().any(|(_, p)| p == target) {
                                self.options.push((importer::label(target), target.clone()));
                            }
                        }
                        self.choice = self
                            .options
                            .iter()
                            .position(|(_, p)| *p == config.duplicate_field)
                            .unwrap_or(0);
                        self.step = ImportStep::Match;
                    }
                    2 => {
                        self.choice = match self.config.as_ref().unwrap().duplicates {
                            importer::Duplicates::Skip => 0,
                            importer::Duplicates::Merge => 1,
                            importer::Duplicates::KeepBoth => 2,
                        };
                        self.step = ImportStep::Policy;
                    }
                    _ => {
                        self.column = self.selected - 3;
                        self.options = vec![("Ignore this column".into(), String::new())];
                        self.options.extend(importer::targets(
                            store,
                            &self.config.as_ref().unwrap().kind,
                        )?);
                        if let Some(target) = self
                            .config
                            .as_ref()
                            .unwrap()
                            .mapping
                            .get(&self.column)
                            .filter(|p| !self.options.iter().any(|(_, t)| t == *p))
                        {
                            self.options.push((
                                format!("CSV field: {}", importer::label(target)),
                                target.clone(),
                            ));
                        }
                        self.choice = self
                            .config
                            .as_ref()
                            .unwrap()
                            .mapping
                            .get(&self.column)
                            .and_then(|p| self.options.iter().position(|(_, t)| t == p))
                            .unwrap_or(0);
                        self.step = ImportStep::Target;
                    }
                }
            }
            return Ok(Outcome::Continue);
        }
        let count = match self.step {
            ImportStep::Kind => self.kinds.len(),
            ImportStep::Policy => 3,
            _ => self.options.len(),
        };
        if next {
            self.choice = (self.choice + 1) % count;
        } else if prev {
            self.choice = (self.choice + count - 1) % count;
        } else if key.code == KeyCode::Enter {
            match self.step {
                ImportStep::Kind => {
                    self.kind = Some(self.kinds[self.choice].slug.clone());
                    self.mapping(store)?;
                }
                ImportStep::Policy => {
                    self.config.as_mut().unwrap().duplicates = [
                        importer::Duplicates::Skip,
                        importer::Duplicates::Merge,
                        importer::Duplicates::KeepBoth,
                    ][self.choice]
                        .clone();
                    self.step = ImportStep::Map;
                }
                ImportStep::Match => {
                    self.config.as_mut().unwrap().duplicate_field =
                        self.options[self.choice].1.clone();
                    self.step = ImportStep::Map;
                }
                ImportStep::Target => {
                    let target = self.options[self.choice].1.clone();
                    let config = self.config.as_mut().unwrap();
                    config.mapping.remove(&self.column);
                    if !target.is_empty() {
                        config.mapping.retain(|_, p| *p != target);
                        config.mapping.insert(self.column, target);
                    }
                    self.step = ImportStep::Map;
                }
                _ => {}
            }
        }
        Ok(Outcome::Continue)
    }
    fn render(&mut self, frame: &mut Frame, p: Palette) {
        if self.step == ImportStep::Path {
            input_screen(
                frame,
                p,
                &self.input,
                " Import CSV • file path ",
                &format!(
                    "Enter a CSV file path (~/ is supported), then Enter.\nYou will map columns and preview before anything is written.\n{}\nEsc/C-g: cancel",
                    self.error
                ),
            );
            return;
        }
        let areas =
            Layout::vertical([Constraint::Min(1), Constraint::Length(4)]).split(frame.area());
        if self.step == ImportStep::Preview {
            let paragraph = Paragraph::new(self.report.as_str())
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
                        .title(" CSV import preview • F10 imports ")
                        .border_style(p.base().fg(p.accent)),
                ),
                areas[0],
            );
        } else {
            let (title, labels, selected) = match self.step {
                ImportStep::Map => {
                    let config = self.config.as_ref().unwrap();
                    let mut labels = vec![
                        format!("Record type: {}", config.kind),
                        format!(
                            "Duplicate match field: {}",
                            importer::label(&config.duplicate_field)
                        ),
                        format!("Duplicate action: {}", policy(&config.duplicates)),
                    ];
                    for (i, header) in self.file.as_ref().unwrap().headers.iter().enumerate() {
                        labels.push(format!(
                            "Column {} — {} → {}",
                            i + 1,
                            header,
                            config
                                .mapping
                                .get(&i)
                                .map(|p| importer::label(p))
                                .unwrap_or("Ignore".into())
                        ));
                    }
                    (
                        " Map columns • Enter changes a mapping ",
                        labels,
                        self.selected,
                    )
                }
                ImportStep::Kind => (
                    " Choose a record type ",
                    self.kinds.iter().map(|t| t.label.clone()).collect(),
                    self.choice,
                ),
                ImportStep::Policy => (
                    " Resolve duplicates ",
                    [
                        importer::Duplicates::Skip,
                        importer::Duplicates::Merge,
                        importer::Duplicates::KeepBoth,
                    ]
                    .iter()
                    .map(|p| policy(p).into())
                    .collect(),
                    self.choice,
                ),
                _ => (
                    " Choose a field ",
                    self.options
                        .iter()
                        .map(|(label, _)| label.clone())
                        .collect(),
                    self.choice,
                ),
            };
            let mut state = ListState::default().with_selected(Some(selected));
            frame.render_stateful_widget(
                List::new(labels)
                    .style(p.base())
                    .highlight_symbol("› ")
                    .highlight_style(p.base().bg(p.selection))
                    .block(
                        Block::bordered()
                            .title(title)
                            .border_style(p.base().fg(p.accent)),
                    ),
                areas[0],
                &mut state,
            );
        }
        frame.render_widget(Paragraph::new(format!("↑/↓ or C-n/p: choose • Enter: edit/apply • Esc: back\n{}\nBlank mapped cells preserve existing values; list cells use ;\n{}",if self.step==ImportStep::Preview{"PgUp/PgDn: review • F10: import • Esc: edit mappings"}else{"F10: preview (does not import yet)"},self.error)).style(p.base()).wrap(Wrap{trim:false}),areas[1]);
    }
}
fn policy(p: &importer::Duplicates) -> &'static str {
    match p {
        importer::Duplicates::Skip => "Skip duplicates (keep existing)",
        importer::Duplicates::Merge => "Merge mapped nonblank values",
        importer::Duplicates::KeepBoth => "Keep both (create another record)",
    }
}
fn input_screen(frame: &mut Frame, p: Palette, input: &Editor, title: &str, help: &str) {
    let areas = Layout::vertical([Constraint::Min(1), Constraint::Length(5)]).split(frame.area());
    let col = super::Line::from(input.chars[..input.cursor].iter().collect::<String>()).width();
    let start = col.saturating_sub(areas[0].width.saturating_sub(3) as usize);
    frame.render_widget(
        Paragraph::new(input.text())
            .style(p.base())
            .scroll((0, start.min(u16::MAX as usize) as u16))
            .block(
                Block::bordered()
                    .title(title)
                    .border_style(p.base().fg(p.accent)),
            ),
        areas[0],
    );
    if areas[0].width > 2 && areas[0].height > 2 {
        frame.set_cursor_position((areas[0].x + 1 + (col - start) as u16, areas[0].y + 1));
    }
    frame.render_widget(
        Paragraph::new(help)
            .style(p.base())
            .wrap(Wrap { trim: false }),
        areas[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    #[test]
    fn palette_prioritizes_named_actions_and_finds_custom_categories_and_themes() {
        let mut types = catalog::builtins();
        types.push(catalog::RecordType {
            slug: "project".into(),
            label: "Projects".into(),
            builtin: false,
        });
        let mut palette = Commands::new(&types);
        for (query, expected) in [
            ("trash", "Trash"),
            ("schema", "Edit schema"),
            ("import", "Import CSV"),
            ("dracula", "Theme: Dracula"),
            ("projects", "Go to Projects"),
        ] {
            palette.input.set(query);
            assert_eq!(palette.matches()[0].name, expected);
        }
        palette.input.set("trash");
        assert!(matches!(
            palette.handle(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Action::Trash)
        ));
    }
    #[test]
    fn palette_overlays_only_bottom_quarter_and_keeps_selection_visible() {
        for height in [12, 24, 43] {
            let mut terminal = Terminal::new(TestBackend::new(100, height)).unwrap();
            let mut palette = Commands::new(&catalog::builtins());
            palette.input.set("trash");
            let top = height - height.div_ceil(4);
            terminal
                .draw(|frame| {
                    frame.render_widget(Paragraph::new("Records remain visible"), frame.area());
                    palette.render(frame, super::super::theme::Theme::Dracula.palette());
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            let row = |y| {
                (0..100)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            };
            assert!(row(0).contains("Records remain visible"));
            for y in 1..top {
                assert!(row(y).trim().is_empty());
            }
            assert!((top..height).any(|y| row(y).contains("trash")));
            assert!((top..height).any(|y| row(y).contains("› Trash")));
            let cursor = terminal.get_cursor_position().unwrap();
            assert!(cursor.y >= top && cursor.y < height);
            assert!(matches!(
                palette.handle(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                Some(Action::Trash)
            ));
        }
    }
    #[test]
    fn csv_wizard_renders_preview_and_writes_only_after_explicit_commit() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("people.csv");
        std::fs::write(&path, "Name,Email\nAda,ada@example.com\n").unwrap();
        let mut screen = ImportScreen::new(&store, Some("contact".into())).unwrap();
        screen.input.set(path.to_str().unwrap());
        screen
            .handle(&store, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert!(screen.step == ImportStep::Map);
        screen
            .handle(&store, KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))
            .unwrap();
        assert!(screen.step == ImportStep::Preview);
        assert!(store.all_records().unwrap().is_empty());
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|frame| screen.render(frame, super::super::theme::Theme::Dracula.palette()))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("CSV import preview"));
        assert!(text.contains("ada@example.com"));
        assert!(matches!(
            screen
                .handle(&store, KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))
                .unwrap(),
            Outcome::Imported { .. }
        ));
        assert_eq!(store.all_records().unwrap().len(), 1);
    }
}
