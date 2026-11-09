use super::{details, editor::Editor, theme::Palette};
use crate::{
    workflow::{Filter, Op, Rule, View},
    *,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    widgets::{Block, List, ListState, Paragraph, Wrap},
};
use std::collections::BTreeMap;
pub enum Outcome {
    Continue,
    Close,
    Open(View),
}
pub struct Manager {
    views: Vec<View>,
    selected: usize,
    editor: Option<ViewEditor>,
    error: String,
}
impl Manager {
    pub fn new(store: &Store, initial: Option<View>) -> Result<Self> {
        Ok(Self {
            views: store.views()?,
            selected: 0,
            editor: initial.map(|v| ViewEditor::new(v, store)),
            error: String::new(),
        })
    }
    pub fn handle(&mut self, store: &Store, key: KeyEvent) -> Result<Outcome> {
        if let Some(editor) = self.editor.as_mut() {
            match editor.handle(store, key)? {
                EditOutcome::Continue => {}
                EditOutcome::Cancel => self.editor = None,
                EditOutcome::Saved(view) => {
                    self.editor = None;
                    self.views = store.views()?;
                    self.selected = self.views.iter().position(|v| v.id == view.id).unwrap_or(0);
                    self.error = "View saved. Enter opens it.".into();
                }
            }
            return Ok(Outcome::Continue);
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('g')) {
            return Ok(Outcome::Close);
        }
        match key.code {
            KeyCode::Down | KeyCode::Char('n') if key.code == KeyCode::Down || ctrl => {
                self.selected = (self.selected + 1).min(self.views.len().saturating_sub(1))
            }
            KeyCode::Up | KeyCode::Char('p') if key.code == KeyCode::Up || ctrl => {
                self.selected = self.selected.saturating_sub(1)
            }
            KeyCode::Enter => {
                if let Some(view) = self.views.get(self.selected) {
                    return Ok(Outcome::Open(view.clone()));
                }
            }
            KeyCode::Char('n') if key.modifiers.is_empty() => {
                self.editor = Some(ViewEditor::new(View::blank(), store))
            }
            KeyCode::Char('e') if key.modifiers.is_empty() => {
                if let Some(view) = self.views.get(self.selected) {
                    self.editor = Some(ViewEditor::new(view.clone(), store));
                }
            }
            KeyCode::Char('d') if key.modifiers.is_empty() => {
                if let Some(view) = self.views.get(self.selected) {
                    store.delete_view(&view.id)?;
                    self.views = store.views()?;
                    self.selected = self.selected.min(self.views.len().saturating_sub(1));
                }
            }
            _ => {}
        }
        Ok(Outcome::Continue)
    }
    pub fn render(&mut self, frame: &mut Frame, p: Palette) {
        if frame.area().width < 45 || frame.area().height < 10 {
            frame.render_widget(
                Paragraph::new("Enlarge the terminal to at least 45 × 10. Esc goes back.")
                    .style(p.base()),
                frame.area(),
            );
            return;
        }
        if let Some(editor) = self.editor.as_mut() {
            editor.render(frame, p);
            return;
        }
        let areas =
            Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(frame.area());
        let labels: Vec<_> = self
            .views
            .iter()
            .map(|v| {
                format!(
                    "{} • {} • {} conditions",
                    v.name,
                    if v.kind.is_empty() {
                        "All records"
                    } else {
                        &v.kind
                    },
                    v.filters.len()
                )
            })
            .collect();
        let mut state = ListState::default().with_selected(Some(self.selected));
        frame.render_stateful_widget(
            List::new(labels)
                .style(p.base())
                .highlight_symbol("› ")
                .highlight_style(p.base().bg(p.selection))
                .block(
                    Block::bordered()
                        .title(" Saved views ")
                        .border_style(p.base().fg(p.accent)),
                ),
            areas[0],
            &mut state,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "Enter open • n New • e Edit • d Delete view • Esc/C-g back\n{}",
                self.error
            ))
            .style(p.base())
            .wrap(Wrap { trim: false }),
            areas[1],
        );
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Step {
    Fields,
    Name,
    Query,
    Value,
    Kind,
    Rule,
    Sort,
    Direction,
    Columns,
    FilterField,
    FilterOp,
    Discard,
}
struct ViewEditor {
    view: View,
    original: Value,
    step: Step,
    selected: usize,
    choice: usize,
    input: Editor,
    options: Vec<(String, String)>,
    columns: Vec<String>,
    filter: Option<Filter>,
    filter_index: Option<usize>,
    error: String,
    kinds: Vec<(String, String)>,
}
enum EditOutcome {
    Continue,
    Cancel,
    Saved(View),
}
fn fields(store: &Store) -> Vec<(String, String)> {
    let mut options: BTreeMap<String, String> = BTreeMap::new();
    for key in [
        "name",
        "kind",
        "created",
        "updated",
        "last_activity",
        "email",
        "phone",
        "stage",
        "status",
        "due",
        "value",
        "currency",
        "contact_id",
        "company_id",
    ] {
        options.insert(key.into(), details::label(key));
    }
    fn walk(v: &Value, path: &str, label: &str, options: &mut BTreeMap<String, String>) {
        if let Some(map) = v.as_object() {
            for (key, v) in map {
                let path = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                let name = format!("{label}{}", details::label(key));
                options.insert(path.clone(), name.clone());
                walk(v, &path, &format!("{name} › "), options);
            }
        }
    }
    if let Ok(records) = store.all_records() {
        for r in records {
            walk(&r["data"], "/data", "Field: ", &mut options);
        }
    }
    if let Ok(mut stmt)=store.conn.prepare("SELECT template FROM record_schemas QUALIFY row_number() OVER (PARTITION BY kind ORDER BY version DESC)=1") && let Ok(rows)=stmt.query_map([],|r|r.get::<_,String>(0)) {for value in rows.flatten(){if let Ok(v)=serde_json::from_str::<Value>(&value){walk(&v,"/data","Field: ",&mut options);}}}
    let mut result: Vec<_> = options
        .into_iter()
        .map(|(path, label)| (label, path))
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}
impl ViewEditor {
    fn new(view: View, store: &Store) -> Self {
        let mut options = fields(store);
        for path in view
            .columns
            .iter()
            .chain(std::iter::once(&view.sort))
            .chain(view.filters.iter().map(|f| &f.field))
        {
            if !options.iter().any(|(_, p)| p == path) {
                options.push((path.clone(), path.clone()));
            }
        }
        Self {
            kinds: std::iter::once(("All records".into(), String::new()))
                .chain(
                    store
                        .types()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|t| (t.label, t.slug)),
                )
                .collect(),
            original: serde_json::to_value(&view).unwrap_or(Value::Null),
            columns: view.columns.clone(),
            view,
            step: Step::Fields,
            selected: 0,
            choice: 0,
            input: Editor::default(),
            options,
            filter: None,
            filter_index: None,
            error: String::new(),
        }
    }
    fn label(&self, path: &str) -> String {
        self.options
            .iter()
            .find(|(_, p)| p == path)
            .map(|v| v.0.clone())
            .unwrap_or(path.into())
    }
    fn rows(&self) -> Vec<String> {
        let mut rows = vec![
            format!("Name: {}", self.view.name),
            format!(
                "Record type: {}",
                if self.view.kind.is_empty() {
                    "All"
                } else {
                    &self.view.kind
                }
            ),
            format!("Base filter: {}", rule_label(&self.view.rule)),
            format!("Search text: {}", self.view.query),
            format!("Sort by: {}", self.label(&self.view.sort)),
            format!(
                "Direction: {}",
                if self.view.descending {
                    "Descending"
                } else {
                    "Ascending"
                }
            ),
            format!(
                "Visible fields: {}",
                self.view
                    .columns
                    .iter()
                    .map(|p| self.label(p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ];
        rows.extend(
            self.view
                .filters
                .iter()
                .map(|f| format!("{} {} {}", self.label(&f.field), f.op.label(), f.value)),
        );
        rows
    }
    fn start_filter(&mut self, index: Option<usize>) {
        self.filter_index = index;
        self.filter = Some(
            index
                .map(|i| self.view.filters[i].clone())
                .unwrap_or(Filter {
                    field: "name".into(),
                    op: Op::Equals,
                    value: String::new(),
                }),
        );
        self.choice = self
            .options
            .iter()
            .position(|(_, p)| *p == self.filter.as_ref().unwrap().field)
            .unwrap_or(0);
        self.step = Step::FilterField;
    }
    fn finish_filter(&mut self) {
        if let Some(mut filter) = self.filter.take() {
            if matches!(filter.op, Op::Empty | Op::NotEmpty) {
                filter.value.clear();
            }
            if let Some(i) = self.filter_index {
                self.view.filters[i] = filter;
            } else {
                self.view.filters.push(filter);
                self.selected = 6 + self.view.filters.len();
            }
        }
        self.step = Step::Fields;
    }
    fn handle(&mut self, store: &Store, key: KeyEvent) -> Result<EditOutcome> {
        self.error.clear();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('g')) {
            if self.step != Step::Fields {
                self.step = Step::Fields;
            } else if serde_json::to_value(&self.view)? != self.original {
                self.step = Step::Discard;
                self.choice = 0;
            } else {
                return Ok(EditOutcome::Cancel);
            }
            return Ok(EditOutcome::Continue);
        }
        if key.code == KeyCode::F(10) {
            if self.step != Step::Fields {
                self.error = "Finish the current step with Enter before saving.".into();
                return Ok(EditOutcome::Continue);
            }
            return match store.save_view(&self.view) {
                Ok(view) => Ok(EditOutcome::Saved(view)),
                Err(e) => {
                    self.error = e.to_string();
                    Ok(EditOutcome::Continue)
                }
            };
        }
        let next = key.code == KeyCode::Down
            || (ctrl && key.code == KeyCode::Char('n'))
            || key.code == KeyCode::Tab;
        let prev = key.code == KeyCode::Up
            || (ctrl && key.code == KeyCode::Char('p'))
            || key.code == KeyCode::BackTab;
        if self.step == Step::Fields {
            if next {
                self.selected = (self.selected + 1).min(self.rows().len() - 1);
            } else if prev {
                self.selected = self.selected.saturating_sub(1);
            } else if key.code == KeyCode::F(2) {
                self.start_filter(None);
            } else if key.code == KeyCode::F(3) && self.selected >= 7 {
                self.view.filters.remove(self.selected - 7);
                self.selected = self.selected.min(self.rows().len() - 1);
            } else if key.code == KeyCode::Enter {
                self.choice = 0;
                match self.selected {
                    0 => {
                        self.input.set(&self.view.name);
                        self.step = Step::Name;
                    }
                    1 => {
                        self.step = Step::Kind;
                        self.choice = self
                            .kinds
                            .iter()
                            .position(|(_, s)| *s == self.view.kind)
                            .unwrap_or(0);
                    }
                    2 => {
                        self.step = Step::Rule;
                        self.choice = rule_index(&self.view.rule);
                    }
                    3 => {
                        self.input.set(&self.view.query);
                        self.step = Step::Query;
                    }
                    4 => {
                        self.step = Step::Sort;
                        self.choice = self
                            .options
                            .iter()
                            .position(|(_, p)| *p == self.view.sort)
                            .unwrap_or(0);
                    }
                    5 => {
                        self.step = Step::Direction;
                        self.choice = usize::from(self.view.descending);
                    }
                    6 => {
                        self.step = Step::Columns;
                        self.columns = self.view.columns.clone();
                    }
                    _ => self.start_filter(Some(self.selected - 7)),
                }
            }
            return Ok(EditOutcome::Continue);
        }
        if matches!(self.step, Step::Name | Step::Query | Step::Value) {
            if key.code == KeyCode::Enter {
                let text = self.input.text();
                match self.step {
                    Step::Name => self.view.name = text,
                    Step::Query => self.view.query = text,
                    Step::Value => {
                        self.filter.as_mut().unwrap().value = text;
                        self.finish_filter();
                    }
                    _ => {}
                }
                self.step = Step::Fields;
            } else {
                self.input.handle(key);
            }
            return Ok(EditOutcome::Continue);
        }
        let labels = self.labels();
        if next {
            self.choice = (self.choice + 1) % labels.len();
        } else if prev {
            self.choice = (self.choice + labels.len() - 1) % labels.len();
        } else if self.step == Step::Columns && key.code == KeyCode::Char(' ') {
            let path = self.options[self.choice].1.clone();
            if self.columns.contains(&path) {
                self.columns.retain(|p| *p != path);
            } else {
                self.columns.push(path);
            }
        } else if key.code == KeyCode::Enter {
            match self.step {
                Step::Kind => {
                    self.view.kind = self.kinds[self.choice].1.clone();
                    self.step = Step::Fields;
                }
                Step::Rule => {
                    self.view.rule = [
                        Rule::All,
                        Rule::OverdueTasks,
                        Rule::ActiveDeals,
                        Rule::FollowUpContacts,
                    ][self.choice]
                        .clone();
                    self.step = Step::Fields;
                }
                Step::Sort => {
                    self.view.sort = self.options[self.choice].1.clone();
                    self.step = Step::Fields;
                }
                Step::Direction => {
                    self.view.descending = self.choice == 1;
                    self.step = Step::Fields;
                }
                Step::Columns => {
                    self.view.columns = self.columns.clone();
                    self.step = Step::Fields;
                }
                Step::FilterField => {
                    self.filter.as_mut().unwrap().field = self.options[self.choice].1.clone();
                    self.step = Step::FilterOp;
                    self.choice = 0;
                }
                Step::FilterOp => {
                    self.filter.as_mut().unwrap().op = Op::ALL[self.choice].clone();
                    if matches!(self.filter.as_ref().unwrap().op, Op::Empty | Op::NotEmpty) {
                        self.finish_filter();
                    } else {
                        self.input.set(&self.filter.as_ref().unwrap().value);
                        self.step = Step::Value;
                    }
                }
                Step::Discard => {
                    if self.choice == 1 {
                        return Ok(EditOutcome::Cancel);
                    } else {
                        self.step = Step::Fields;
                    }
                }
                _ => {}
            }
        }
        Ok(EditOutcome::Continue)
    }
    fn labels(&self) -> Vec<String> {
        match self.step {
            Step::Fields => self.rows(),
            Step::Kind => self.kinds.iter().map(|(label, _)| label.clone()).collect(),
            Step::Rule => [
                Rule::All,
                Rule::OverdueTasks,
                Rule::ActiveDeals,
                Rule::FollowUpContacts,
            ]
            .iter()
            .map(|r| rule_label(r).into())
            .collect(),
            Step::Sort | Step::FilterField => self.options.iter().map(|p| p.0.clone()).collect(),
            Step::Columns => self
                .options
                .iter()
                .map(|(label, path)| {
                    format!(
                        "[{}] {label}",
                        if self.columns.contains(path) {
                            "x"
                        } else {
                            " "
                        }
                    )
                })
                .collect(),
            Step::FilterOp => Op::ALL.iter().map(|op| op.label().into()).collect(),
            Step::Direction => vec!["Ascending".into(), "Descending".into()],
            Step::Discard => vec!["Keep editing".into(), "Discard view draft".into()],
            _ => vec![],
        }
    }
    fn render(&mut self, frame: &mut Frame, p: Palette) {
        let areas =
            Layout::vertical([Constraint::Min(1), Constraint::Length(5)]).split(frame.area());
        if matches!(self.step, Step::Name | Step::Query | Step::Value) {
            let col = super::Line::from(
                self.input.chars[..self.input.cursor]
                    .iter()
                    .collect::<String>(),
            )
            .width();
            let start = col.saturating_sub(areas[0].width.saturating_sub(3) as usize);
            frame.set_cursor_position((areas[0].x + 1 + (col - start) as u16, areas[0].y + 1));
            frame.render_widget(
                Paragraph::new(self.input.text())
                    .scroll((0, start.min(u16::MAX as usize) as u16))
                    .style(p.base())
                    .wrap(Wrap { trim: false })
                    .block(
                        Block::bordered()
                            .title(match self.step {
                                Step::Name => " View name ",
                                Step::Query => " Search text (optional) ",
                                _ => " Filter value • dates YYYY-MM-DD or $today; links by name ",
                            })
                            .border_style(p.base().fg(p.accent)),
                    ),
                areas[0],
            );
        } else {
            let mut state =
                ListState::default().with_selected(Some(if self.step == Step::Fields {
                    self.selected
                } else {
                    self.choice
                }));
            frame.render_stateful_widget(
                List::new(self.labels())
                    .style(p.base())
                    .highlight_symbol("› ")
                    .highlight_style(p.base().bg(p.selection))
                    .block(
                        Block::bordered()
                            .title(if self.step == Step::Fields {
                                " Configure saved view "
                            } else if self.step == Step::Columns {
                                " Visible fields • Space toggles • Enter applies "
                            } else {
                                " Choose an option • Enter selects "
                            })
                            .border_style(p.base().fg(p.accent)),
                    ),
                areas[0],
                &mut state,
            );
        }
        frame.render_widget(Paragraph::new(format!("Tab / C-n/p: move • Enter: edit/apply • Esc/C-g: back\nF2: add filter • F3: remove selected filter • F10: Save view\nFilters combine with AND. $today dates update automatically.\n{}",self.error)).style(p.base()).wrap(Wrap{trim:false}),areas[1]);
    }
}
fn rule_index(rule: &Rule) -> usize {
    match rule {
        Rule::All => 0,
        Rule::OverdueTasks => 1,
        Rule::ActiveDeals => 2,
        Rule::FollowUpContacts => 3,
    }
}
fn rule_label(rule: &Rule) -> &'static str {
    match rule {
        Rule::All => "All matching records",
        Rule::OverdueTasks => "Overdue open tasks",
        Rule::ActiveDeals => "Active deals (not won/lost)",
        Rule::FollowUpContacts => "Contacts needing follow-up",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(editor: &mut ViewEditor, store: &Store, code: KeyCode) -> EditOutcome {
        editor
            .handle(store, KeyEvent::new(code, KeyModifiers::NONE))
            .unwrap()
    }
    #[test]
    fn guided_view_editor_saves_conditions_sort_and_visible_columns() {
        let s = Store::open(std::path::Path::new(":memory:")).unwrap();
        for (name, value, stage) in [
            ("Small", 2, "qualified"),
            ("Large", 10, "qualified"),
            ("Won", 99, "won"),
        ] {
            s.execute(&[
                "add".into(),
                "deal".into(),
                name.into(),
                json!({"value":value,"stage":stage}).to_string(),
            ])
            .unwrap();
        }
        let mut editor = ViewEditor::new(View::blank(), &s);
        key(&mut editor, &s, KeyCode::Enter);
        for c in "Qualified opportunities".chars() {
            key(&mut editor, &s, KeyCode::Char(c));
        }
        key(&mut editor, &s, KeyCode::Enter);
        editor.selected = 1;
        key(&mut editor, &s, KeyCode::Enter);
        editor.choice = 3;
        key(&mut editor, &s, KeyCode::Enter);
        editor.selected = 2;
        key(&mut editor, &s, KeyCode::Enter);
        editor.choice = 2;
        key(&mut editor, &s, KeyCode::Enter);
        key(&mut editor, &s, KeyCode::F(2));
        editor.choice = editor
            .options
            .iter()
            .position(|(_, p)| p == "stage")
            .unwrap();
        key(&mut editor, &s, KeyCode::Enter);
        key(&mut editor, &s, KeyCode::Enter);
        for c in "qualified".chars() {
            key(&mut editor, &s, KeyCode::Char(c));
        }
        key(&mut editor, &s, KeyCode::Enter);
        editor.selected = 4;
        key(&mut editor, &s, KeyCode::Enter);
        editor.choice = editor
            .options
            .iter()
            .position(|(_, p)| p == "value")
            .unwrap();
        key(&mut editor, &s, KeyCode::Enter);
        editor.selected = 5;
        key(&mut editor, &s, KeyCode::Enter);
        key(&mut editor, &s, KeyCode::Down);
        key(&mut editor, &s, KeyCode::Enter);
        editor.selected = 6;
        key(&mut editor, &s, KeyCode::Enter);
        editor.choice = editor
            .options
            .iter()
            .position(|(_, p)| p == "value")
            .unwrap();
        key(&mut editor, &s, KeyCode::Char(' '));
        key(&mut editor, &s, KeyCode::Enter);
        let EditOutcome::Saved(view) = key(&mut editor, &s, KeyCode::F(10)) else {
            panic!("View did not save: {}", editor.error)
        };
        let saved = s.find_view(&view.id).unwrap();
        assert!(saved.columns.contains(&"value".into()));
        let records = s.all_records().unwrap();
        let matched = saved.select(&records, workflow::date());
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0]["name"], "Large");
        let mut manager = Manager::new(&s, None).unwrap();
        manager.selected = manager.views.iter().position(|v| v.id == saved.id).unwrap();
        assert!(
            matches!(manager.handle(&s,KeyEvent::new(KeyCode::Enter,KeyModifiers::NONE)).unwrap(),Outcome::Open(v) if v.id==saved.id)
        );
    }
}
