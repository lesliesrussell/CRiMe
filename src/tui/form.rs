use super::{details, editor::Editor, theme::Palette};
use crate::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Modifier,
    text::{Line, Span, Text},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};
use std::collections::BTreeMap;
pub(crate) mod schema;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Kind {
    Text,
    Number,
    Bool,
    Date,
    Object,
    Array,
    Link(String),
}
impl Kind {
    pub(crate) fn title(&self) -> String {
        match self {
            Self::Text => "Text".into(),
            Self::Number => "Number".into(),
            Self::Bool => "Yes / No".into(),
            Self::Date => "Date".into(),
            Self::Object => "Field group".into(),
            Self::Array => "List".into(),
            Self::Link(k) => format!("{} link", details::label(k)),
        }
    }
    fn blank(&self) -> Value {
        match self {
            Self::Object => json!({}),
            Self::Array => json!([]),
            _ => Value::Null,
        }
    }
}
#[derive(Clone)]
struct Row {
    path: String,
    label: String,
    depth: usize,
    kind: Kind,
}
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Fields,
    Edit,
    AddName,
    Type,
    Choice,
    Discard,
    RecordType,
    ChangeType,
    Migration,
    Control,
    PropertyEdit,
}
pub enum Outcome {
    Continue,
    Cancel,
    Saved(Value),
    SchemaSaved(String, i64),
}
pub struct Form {
    pub kind: String,
    id: Option<String>,
    draft: Value,
    original: Value,
    hints: BTreeMap<String, (Kind, String)>,
    selected: usize,
    mode: Mode,
    editor: Editor,
    path: String,
    parent: String,
    edit_kind: Kind,
    choices: Vec<(String, Value)>,
    choice: usize,
    error: String,
    prefix: bool,
    schema_edit: bool,
    current_schema: Option<schema::Schema>,
    record_version: i64,
    original_version: i64,
    original_hints: BTreeMap<String, (Kind, String)>,
    migration: Option<schema::Migration>,
    migration_scroll: u16,
    available_types: Vec<catalog::RecordType>,
    controls: BTreeMap<String, schema::FieldControl>,
    original_controls: BTreeMap<String, schema::FieldControl>,
    property: usize,
    control_path: String,
}
fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
fn child(parent: &str, key: &str) -> String {
    format!("{parent}/{}", escape(key))
}
fn infer(key: &str, value: &Value) -> Kind {
    if let Some(kind) = key.strip_suffix("_id").filter(|k| validate_kind(k).is_ok()) {
        return Kind::Link(kind.into());
    }
    if value.as_str().is_some_and(valid_date) {
        return Kind::Date;
    }
    match value {
        Value::Number(_) => Kind::Number,
        Value::Bool(_) => Kind::Bool,
        Value::Object(_) => Kind::Object,
        Value::Array(_) => Kind::Array,
        _ => Kind::Text,
    }
}
fn blank_shape(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            Value::Object(m.iter().map(|(k, v)| (k.clone(), blank_shape(v))).collect())
        }
        Value::Array(_) => json!([]),
        _ => Value::Null,
    }
}
fn merge_missing(target: &mut Value, example: &Value) {
    if let (Some(dst), Some(src)) = (target.as_object_mut(), example.as_object()) {
        for (k, v) in src {
            if let Some(existing) = dst.get_mut(k) {
                merge_missing(existing, v);
            } else {
                dst.insert(k.clone(), blank_shape(v));
            }
        }
    }
}
fn collect_hints(value: &Value, path: &str, hints: &mut BTreeMap<String, (Kind, String)>) {
    match value {
        Value::Object(m) => {
            for (key, v) in m {
                let p = child(path, key);
                hints.entry(p.clone()).or_insert((
                    infer(key, v),
                    "Custom field discovered in saved records.".into(),
                ));
                collect_hints(v, &p, hints);
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                collect_hints(v, &child(path, &i.to_string()), hints);
            }
        }
        _ => {}
    }
}
fn defaults(kind: &str) -> Vec<(&'static str, Kind, &'static str)> {
    use Kind::*;
    match kind {
        "contact" => vec![
            ("email", Text, "Email address, for example ada@example.com."),
            (
                "phone",
                Text,
                "Telephone number, including country code if useful.",
            ),
            (
                "company_id",
                Link("company".into()),
                "Choose the company by name; no record ID required.",
            ),
            ("job_title", Text, "Role or job title."),
            (
                "last_contacted",
                Date,
                "Last interaction date (YYYY-MM-DD). Notes and completed follow-ups are also counted.",
            ),
            (
                "next_contact",
                Date,
                "Plan the next contact (YYYY-MM-DD). Today shows it when due.",
            ),
            (
                "follow_up_days",
                Number,
                "Days between contacts; Today uses 30 when this is not set.",
            ),
            (
                "tags",
                Array,
                "A list of labels. Enter or F4 adds an item; F2 adds a named field.",
            ),
            (
                "notes",
                Text,
                "Free-form notes. C-j inserts a new line while editing.",
            ),
        ],
        "company" => vec![
            (
                "website",
                Text,
                "Company website, for example https://example.com.",
            ),
            ("industry", Text, "Industry or line of business."),
            ("phone", Text, "Main telephone number."),
            ("address", Text, "Postal address. C-j inserts a new line."),
            ("notes", Text, "Useful background or account notes."),
        ],
        "deal" => vec![
            (
                "contact_id",
                Link("contact".into()),
                "Choose the person involved in this deal.",
            ),
            (
                "company_id",
                Link("company".into()),
                "Choose the associated company.",
            ),
            ("stage", Text, "Choose a stage or enter your own."),
            (
                "value",
                Number,
                "Numeric deal amount, for example 1250.50; leave empty if unknown.",
            ),
            (
                "currency",
                Text,
                "Currency code, for example USD, EUR or GBP.",
            ),
            (
                "expected_close",
                Date,
                "Expected close date: YYYY-MM-DD, for example 2027-01-31.",
            ),
            ("notes", Text, "Deal background and next steps."),
        ],
        "task" => vec![
            (
                "deal_id",
                Link("deal".into()),
                "Choose the deal this next action moves forward.",
            ),
            (
                "company_id",
                Link("company".into()),
                "Choose the related company if needed.",
            ),
            (
                "contact_id",
                Link("contact".into()),
                "Choose the person this follow-up concerns.",
            ),
            ("due", Date, "Due date: YYYY-MM-DD, for example 2027-01-31."),
            ("status", Text, "Choose a task status or enter your own."),
            ("priority", Text, "Choose a priority or enter your own."),
            (
                "notes",
                Text,
                "What needs to be done. C-j inserts a new line.",
            ),
        ],
        "note" => vec![
            (
                "occurred_on",
                Date,
                "When the interaction happened (YYYY-MM-DD); empty uses the note creation date.",
            ),
            (
                "deal_id",
                Link("deal".into()),
                "Choose a related deal if needed.",
            ),
            (
                "contact_id",
                Link("contact".into()),
                "Choose the person this activity concerns.",
            ),
            (
                "company_id",
                Link("company".into()),
                "Choose a related company if needed.",
            ),
            (
                "body",
                Text,
                "Activity or note text. C-j adds a new line; Enter finishes editing.",
            ),
            (
                "tags",
                Array,
                "Optional labels. Enter or F4 adds an item; F2 adds a named field.",
            ),
        ],
        _ => vec![],
    }
}
impl Form {
    pub fn new(kind: Option<&str>, record: Option<&Value>, records: &[Value]) -> Self {
        let chosen = record
            .and_then(|r| r["kind"].as_str())
            .or(kind)
            .unwrap_or("contact");
        let draft = json!({"name":record.and_then(|r|r["name"].as_str()).unwrap_or(""),"data":record.map(|r|r["data"].clone()).unwrap_or(json!({}))});
        let mut f = Self {
            kind: chosen.into(),
            id: record.and_then(|r| r["id"].as_str()).map(str::to_owned),
            original: draft.clone(),
            draft,
            hints: BTreeMap::new(),
            selected: 0,
            mode: if kind.is_none() && record.is_none() {
                Mode::RecordType
            } else {
                Mode::Fields
            },
            editor: Editor::default(),
            path: String::new(),
            parent: String::new(),
            edit_kind: Kind::Text,
            choices: vec![],
            choice: 0,
            error: String::new(),
            prefix: false,
            schema_edit: false,
            current_schema: None,
            record_version: record
                .and_then(|r| r["schema_version"].as_i64())
                .unwrap_or(0),
            original_version: record
                .and_then(|r| r["schema_version"].as_i64())
                .unwrap_or(0),
            original_hints: BTreeMap::new(),
            migration: None,
            migration_scroll: 0,
            available_types: catalog::builtins(),
            controls: BTreeMap::new(),
            original_controls: BTreeMap::new(),
            property: 0,
            control_path: String::new(),
        };
        f.populate(records);
        f
    }
    pub(crate) fn schema_for(store: &Store, kind: &str) -> Result<schema::Schema> {
        if let Some(schema) = schema::load(store, kind)? {
            return Ok(schema);
        }
        let form = Self::new(Some(kind), None, &store.all_records()?);
        Ok(schema::Schema {
            version: 0,
            template: form.draft["data"].clone(),
            hints: form.hints,
            controls: BTreeMap::new(),
        })
    }
    pub fn preset_link(&mut self, record: &Value) {
        if let (Some(kind), Some(id)) = (record["kind"].as_str(), record["id"].as_str()) {
            let key = format!("{kind}_id");
            self.draft["data"][&key] = json!(id);
            for key in ["contact_id", "company_id"] {
                if let Some(id) = record["data"][key].as_str() {
                    self.draft["data"][key] = json!(id);
                }
            }
            self.hints.insert(
                child("/data", &key),
                (
                    Kind::Link(kind.into()),
                    "Linked to the record you started from.".into(),
                ),
            );
        }
    }
    pub fn restore_schema(&mut self, store: &Store) -> Result<()> {
        self.available_types = store.types()?;
        self.controls.clear();
        let key = format!("form-schema:{}", self.kind);
        if let Ok(schema) = store.conn.query_row(
            "SELECT value FROM preferences WHERE key=?",
            params![key],
            |row| row.get::<_, String>(0),
        ) {
            let saved: BTreeMap<String, (Kind, String)> =
                serde_json::from_str(&schema).context("Cannot read saved field types")?;
            self.hints.extend(saved);
        }
        self.current_schema = schema::load(store, &self.kind)?;
        if self.id.is_none() {
            self.record_version = self.current_schema.as_ref().map(|s| s.version).unwrap_or(0);
        }
        if let Some(schema) = &self.current_schema {
            if self.id.is_none() {
                self.draft["data"] = schema.template.clone();
                self.hints = schema.hints.clone();
                self.record_version = schema.version;
                self.controls = schema.controls.clone();
            } else {
                self.hints.extend(schema.hints.clone());
            }
        }
        if self.id.is_some()
            && self.record_version > 0
            && let Some(applied) = schema::load_version(store, &self.kind, self.record_version)?
        {
            self.controls = applied.controls;
        }
        self.original = self.draft.clone();
        self.original_hints = self.hints.clone();
        self.original_controls = self.controls.clone();
        self.original_version = self.record_version;
        Ok(())
    }
    pub fn schema_editor(kind: Option<&str>, records: &[Value]) -> Self {
        let mut form = Self::new(kind, None, records);
        form.schema_edit = true;
        form
    }
    fn dirty(&self) -> bool {
        self.draft != self.original
            || self.hints != self.original_hints
            || self.record_version != self.original_version
            || self.controls != self.original_controls
    }
    fn populate(&mut self, records: &[Value]) {
        for (key, kind, help) in defaults(&self.kind) {
            let path = child("/data", key);
            self.hints.insert(path, (kind.clone(), help.into()));
            if self.id.is_none() && self.draft["data"].get(key).is_none() {
                self.draft["data"][key] = kind.blank();
            }
        }
        for r in records.iter().filter(|r| r["kind"] == self.kind) {
            collect_hints(&r["data"], "/data", &mut self.hints);
            if self.id.is_none() {
                merge_missing(&mut self.draft["data"], &r["data"]);
            }
        }
        collect_hints(&self.draft["data"], "/data", &mut self.hints);
        self.original = self.draft.clone();
        self.original_hints = self.hints.clone();
    }
    fn field_kind(&self, path: &str, key: &str, value: &Value) -> Kind {
        if !value.is_null() {
            let kind = infer(key, value);
            if kind == Kind::Text
                && self
                    .hints
                    .get(path)
                    .is_some_and(|h| matches!(&h.0, Kind::Date | Kind::Link(_)))
            {
                return self.hints[path].0.clone();
            }
            return kind;
        }
        self.hints
            .get(path)
            .map(|h| h.0.clone())
            .unwrap_or_else(|| infer(key, value))
    }
    fn rows(&self) -> Vec<Row> {
        fn walk(f: &Form, v: &Value, path: &str, depth: usize, out: &mut Vec<Row>) {
            match v {
                Value::Object(m) => {
                    let mut entries: Vec<_> = m.iter().collect();
                    entries.sort_by_key(|(key, _)| {
                        (
                            f.controls
                                .get(&child(path, key))
                                .and_then(|c| c.order)
                                .unwrap_or(usize::MAX),
                            *key,
                        )
                    });
                    for (key, value) in entries {
                        let p = child(path, key);
                        out.push(Row {
                            path: p.clone(),
                            label: format!(
                                "{}{}",
                                details::label(key),
                                if f.controls.get(&p).is_some_and(|c| c.required) {
                                    " *"
                                } else {
                                    ""
                                }
                            ),
                            depth,
                            kind: f.field_kind(&p, key, value),
                        });
                        walk(f, value, &p, depth + 1, out);
                    }
                }
                Value::Array(a) => {
                    for (i, value) in a.iter().enumerate() {
                        let p = child(path, &i.to_string());
                        out.push(Row {
                            path: p.clone(),
                            label: format!("Item {}", i + 1),
                            depth,
                            kind: f.field_kind(&p, "", value),
                        });
                        walk(f, value, &p, depth + 1, out);
                    }
                }
                _ => {}
            }
        }
        let mut rows = vec![
            Row {
                path: "/name".into(),
                label: "Name / title *".into(),
                depth: 0,
                kind: Kind::Text,
            },
            Row {
                path: "/data".into(),
                label: "Fields".into(),
                depth: 0,
                kind: Kind::Object,
            },
        ];
        if self.schema_edit {
            rows.remove(0);
        }
        walk(self, &self.draft["data"], "/data", 0, &mut rows);
        rows
    }
    fn current(&self) -> Row {
        let rows = self.rows();
        rows[self.selected.min(rows.len() - 1)].clone()
    }
    fn set(&mut self, path: &str, value: Value) -> Result<()> {
        *self
            .draft
            .pointer_mut(path)
            .context("This field no longer exists")? = value;
        Ok(())
    }
    fn options(&self, path: &str) -> Option<Vec<&'static str>> {
        match (self.kind.as_str(), path) {
            ("deal", "/data/stage") => Some(vec![
                "lead",
                "qualified",
                "proposal",
                "negotiation",
                "won",
                "lost",
            ]),
            ("task", "/data/status") => Some(vec!["open", "in progress", "done", "cancelled"]),
            ("task", "/data/priority") => Some(vec!["low", "normal", "high"]),
            _ => None,
        }
    }
    fn start_edit(&mut self, records: &[Value]) {
        let row = self.current();
        self.path = row.path.clone();
        self.edit_kind = row.kind.clone();
        self.error.clear();
        let value = self
            .draft
            .pointer(&self.path)
            .cloned()
            .unwrap_or(Value::Null);
        if matches!(row.kind, Kind::Object | Kind::Array) {
            if value.is_null() {
                let _ = self.set(&row.path, row.kind.blank());
            }
            self.start_add();
            return;
        }
        self.choices.clear();
        match &row.kind {
            Kind::Bool => {
                self.choices = vec![
                    ("Not set".into(), Value::Null),
                    ("Yes".into(), json!(true)),
                    ("No".into(), json!(false)),
                ]
            }
            Kind::Link(kind) => {
                self.choices.push(("No linked record".into(), Value::Null));
                for r in records.iter().filter(|r| r["kind"] == *kind) {
                    self.choices.push((
                        format!(
                            "{}  [{}]",
                            r["name"].as_str().unwrap_or("Untitled"),
                            r["id"]
                                .as_str()
                                .unwrap_or("")
                                .chars()
                                .take(8)
                                .collect::<String>()
                        ),
                        r["id"].clone(),
                    ));
                }
                if !value.is_null() && !self.choices.iter().any(|(_, v)| *v == value) {
                    self.choices.push((
                        "Keep existing link (record unavailable)".into(),
                        value.clone(),
                    ));
                }
            }
            _ => {
                if let Some(control) = self
                    .controls
                    .get(&self.path)
                    .filter(|c| !c.choices.is_empty())
                {
                    self.choices = control
                        .choices
                        .iter()
                        .map(|s| (s.clone(), json!(s)))
                        .collect();
                } else if let Some(options) = self.options(&self.path) {
                    self.choices = options
                        .into_iter()
                        .map(|s| (details::label(s), json!(s)))
                        .collect();
                    self.choices
                        .push(("Enter a custom value…".into(), json!({"custom":true})));
                }
            }
        }
        if self.choices.is_empty() {
            self.editor.set(
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        if value.is_null() {
                            String::new()
                        } else {
                            value.to_string()
                        }
                    })
                    .as_str(),
            );
            self.mode = Mode::Edit;
        } else {
            self.choice = self
                .choices
                .iter()
                .position(|(_, v)| *v == value)
                .unwrap_or(0);
            self.mode = Mode::Choice;
        }
    }
    fn start_add(&mut self) {
        let row = self.current();
        let value = self.draft.pointer(&row.path).unwrap_or(&Value::Null);
        self.parent = if value.is_object() || value.is_array() {
            row.path
        } else {
            row.path
                .rsplit_once('/')
                .map(|p| p.0.to_owned())
                .filter(|p| !p.is_empty())
                .unwrap_or("/data".into())
        };
        if self.parent.is_empty() {
            self.parent = "/data".into();
        }
        self.error.clear();
        self.editor.clear();
        self.choice = 0;
        if self
            .draft
            .pointer(&self.parent)
            .is_some_and(Value::is_array)
        {
            self.mode = Mode::Type;
        } else {
            self.mode = Mode::AddName;
        }
    }
    fn start_named_field(&mut self) {
        self.start_add();
        // A field belongs to an object. A selected list/item must not turn F2
        // into an unnamed array insertion; use its enclosing object instead.
        while !self
            .draft
            .pointer(&self.parent)
            .is_some_and(Value::is_object)
        {
            self.parent = self
                .parent
                .rsplit_once('/')
                .map(|p| p.0.to_owned())
                .filter(|p| !p.is_empty())
                .unwrap_or("/data".into());
        }
        self.mode = Mode::AddName;
    }
    fn start_list_item(&mut self) -> Result<()> {
        let mut path = self.current().path;
        loop {
            if self.draft.pointer(&path).is_some_and(Value::is_array) {
                self.parent = path;
                self.editor.clear();
                self.choice = 0;
                self.error.clear();
                self.mode = Mode::Type;
                return Ok(());
            }
            path = path
                .rsplit_once('/')
                .map(|p| p.0.to_owned())
                .filter(|p| !p.is_empty())
                .context("Select a list or one of its items before adding a list item")?;
        }
    }
    fn types(&self) -> Vec<Kind> {
        let mut types = vec![
            Kind::Text,
            Kind::Number,
            Kind::Bool,
            Kind::Date,
            Kind::Object,
            Kind::Array,
        ];
        types.extend(
            self.available_types
                .iter()
                .map(|t| Kind::Link(t.slug.clone())),
        );
        types
    }
    fn insert_field(&mut self, kind: Kind) -> Result<()> {
        let parent = self
            .draft
            .pointer_mut(&self.parent)
            .context("Cannot add here")?;
        if let Some(a) = parent.as_array_mut() {
            self.path = child(&self.parent, &a.len().to_string());
            a.push(kind.blank());
        } else {
            let name = self.editor.text().trim().to_owned();
            let object = parent
                .as_object_mut()
                .context("Select a group to add a field")?;
            if name.is_empty() {
                bail!("Enter a field name");
            }
            if object.contains_key(&name) {
                bail!("A field with that name already exists");
            }
            self.path = child(&self.parent, &name);
            object.insert(name, kind.blank());
        }
        self.hints.insert(
            self.path.clone(),
            (
                kind,
                "Custom field. Enter edits its value; F3 removes it from this draft.".into(),
            ),
        );
        self.selected = self
            .rows()
            .iter()
            .position(|r| r.path == self.path)
            .unwrap_or(0);
        self.mode = Mode::Fields;
        Ok(())
    }
    fn move_field(&mut self, delta: isize) -> Result<()> {
        let row = self.current();
        let parent = row.path.rsplit_once('/').map(|p| p.0).unwrap_or("");
        if !self.draft.pointer(parent).is_some_and(Value::is_object) {
            bail!("Select a field inside a group to change its order");
        }
        let siblings: Vec<_> = self
            .rows()
            .into_iter()
            .filter(|r| r.path.rsplit_once('/').map(|p| p.0) == Some(parent))
            .collect();
        let index = siblings
            .iter()
            .position(|r| r.path == row.path)
            .context("Select a field")?;
        let next = (index as isize + delta).clamp(0, siblings.len() as isize - 1) as usize;
        for (i, r) in siblings.iter().enumerate() {
            self.controls.entry(r.path.clone()).or_default().order = Some(i);
        }
        self.controls
            .entry(siblings[index].path.clone())
            .or_default()
            .order = Some(next);
        self.controls
            .entry(siblings[next].path.clone())
            .or_default()
            .order = Some(index);
        self.selected = self
            .rows()
            .iter()
            .position(|r| r.path == row.path)
            .unwrap_or(0);
        Ok(())
    }
    fn commit_edit(&mut self) -> Result<()> {
        let text = self.editor.text();
        let value = match self.edit_kind {
            Kind::Number => {
                if text.trim().is_empty() {
                    Value::Null
                } else {
                    Value::Number(
                        text.trim()
                            .parse::<serde_json::Number>()
                            .context("Enter a number, for example 1250.50")?,
                    )
                }
            }
            Kind::Date => {
                if text.trim().is_empty() {
                    Value::Null
                } else {
                    if !valid_date(text.trim()) {
                        bail!("Enter a real date as YYYY-MM-DD, for example 2027-01-31");
                    }
                    json!(text.trim())
                }
            }
            _ => json!(text),
        };
        if self.path == "/name" && text.trim().is_empty() {
            bail!("Name / title is required");
        }
        self.set(&self.path.clone(), value)?;
        self.mode = Mode::Fields;
        self.error.clear();
        Ok(())
    }
    fn remove(&mut self) -> Result<()> {
        let row = self.current();
        if row.path == "/name" || row.path == "/data" {
            bail!("Name and the Fields group cannot be removed");
        }
        let (parent, key) = row.path.rsplit_once('/').context("Invalid field")?;
        let value = self
            .draft
            .pointer_mut(parent)
            .context("Invalid field group")?;
        if let Some(a) = value.as_array_mut() {
            a.remove(key.parse::<usize>()?);
        } else if let Some(m) = value.as_object_mut() {
            m.remove(&key.replace("~1", "/").replace("~0", "~"));
        }
        self.selected = self.selected.min(self.rows().len() - 1);
        Ok(())
    }
    fn save(&mut self, store: &Store) -> Result<Value> {
        let name = self.draft["name"].as_str().unwrap_or("").trim();
        if name.is_empty() {
            self.selected = 0;
            bail!("Name / title is required. Select it and press Enter.");
        }
        store.validate_record(&self.kind, &self.draft["data"], self.record_version)?;
        store.atomic(|| -> Result<Value> {
            let before=self.id.as_ref().map(|id|store.get(id)).transpose()?;
            let record = if let Some(id) = &self.id {
                store.replace_record_version(id,name,&self.draft["data"],self.record_version)?
            } else {
                store.execute(&[
                    "add".into(),
                    self.kind.clone(),
                    name.into(),
                    self.draft["data"].to_string(),
                ])?
            };
            store.conn.execute(
                "UPDATE records SET schema_version=? WHERE id=?",
                params![self.record_version, record["id"].as_str().unwrap_or("")],
            )?;
            let record = store.get(record["id"].as_str().unwrap_or(""))?;
            if let Some(before)=before && before["schema_version"]!=record["schema_version"]{store.log_change(Some(&before),Some(&record),"schema migrated")?;}
            store.conn.execute_batch("CREATE TABLE IF NOT EXISTS preferences (key VARCHAR PRIMARY KEY, value VARCHAR NOT NULL)")?;
            store.conn.execute("INSERT INTO preferences VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("form-schema:{}",self.kind),serde_json::to_string(&self.hints)?])?;
            Ok(record)
        })
    }

    pub fn handle(&mut self, store: &Store, records: &[Value], key: KeyEvent) -> Outcome {
        self.error.clear();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if self.mode == Mode::Migration {
            match key.code {
                KeyCode::PageDown => {
                    self.migration_scroll = self.migration_scroll.saturating_add(5);
                    return Outcome::Continue;
                }
                KeyCode::PageUp => {
                    self.migration_scroll = self.migration_scroll.saturating_sub(5);
                    return Outcome::Continue;
                }
                _ => {}
            }
        }
        if ctrl && key.code == KeyCode::Char('c') {
            return Outcome::Continue;
        }
        if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('g')) {
            if self.prefix {
                self.prefix = false;
                return Outcome::Continue;
            }
            if self.mode == Mode::PropertyEdit {
                self.mode = Mode::Control;
                return Outcome::Continue;
            }
            if self.mode != Mode::Fields && self.mode != Mode::RecordType {
                self.mode = Mode::Fields;
                return Outcome::Continue;
            }
            if self.dirty() {
                self.mode = Mode::Discard;
                self.choice = 0;
            } else {
                return Outcome::Cancel;
            }
            return Outcome::Continue;
        }
        let save =
            key.code == KeyCode::F(10) || (self.prefix && ctrl && key.code == KeyCode::Char('s'));
        if self.prefix {
            self.prefix = false;
            if !save {
                self.error = "C-x C-s saves. C-g cancels the prefix.".into();
                return Outcome::Continue;
            }
        }
        if save {
            if self.mode == Mode::Edit
                && let Err(e) = self.commit_edit()
            {
                self.error = e.to_string();
                return Outcome::Continue;
            }
            if self.mode != Mode::Fields {
                self.error = "Finish this step with Enter before saving.".into();
                return Outcome::Continue;
            }
            if self.schema_edit {
                let expected = self.current_schema.as_ref().map(|s| s.version).unwrap_or(0);
                return match schema::save_with_controls(
                    store,
                    &self.kind,
                    expected,
                    &self.draft["data"],
                    &self.hints,
                    &self.controls,
                ) {
                    Ok(version) => Outcome::SchemaSaved(self.kind.clone(), version),
                    Err(e) => {
                        self.error = e.to_string();
                        Outcome::Continue
                    }
                };
            }
            return match self.save(store) {
                Ok(record) => Outcome::Saved(record),
                Err(e) => {
                    self.error = e.to_string();
                    Outcome::Continue
                }
            };
        }
        if ctrl && key.code == KeyCode::Char('x') {
            self.prefix = true;
            return Outcome::Continue;
        }
        let next = key.code == KeyCode::Down || (ctrl && key.code == KeyCode::Char('n'));
        let prev = key.code == KeyCode::Up || (ctrl && key.code == KeyCode::Char('p'));
        let result = (|| -> Result<()> {
            match self.mode {
                Mode::Fields => {
                    if next || key.code == KeyCode::Tab {
                        self.selected = (self.selected + 1).min(self.rows().len() - 1);
                    } else if prev || key.code == KeyCode::BackTab {
                        self.selected = self.selected.saturating_sub(1);
                    } else {
                        match key.code {
                            KeyCode::Enter => self.start_edit(records),
                            KeyCode::F(2) => self.start_named_field(),
                            KeyCode::F(4) => self.start_list_item()?,
                            KeyCode::F(3) => self.remove()?,
                            KeyCode::F(6) if self.schema_edit => {
                                let row = self.current();
                                if row.path == "/data" {
                                    bail!("Select a field, not the Fields group");
                                }
                                self.control_path = row.path;
                                self.choice = 0;
                                self.mode = Mode::Control;
                            }
                            KeyCode::F(7) if self.schema_edit => self.move_field(-1)?,
                            KeyCode::F(9) if self.schema_edit => self.move_field(1)?,
                            KeyCode::F(5) => {
                                if matches!(self.current().path.as_str(), "/name" | "/data") {
                                    bail!("Select a custom or schema field to change its type");
                                }
                                self.choice = self
                                    .types()
                                    .iter()
                                    .position(|k| *k == self.current().kind)
                                    .unwrap_or(0);
                                self.mode = Mode::ChangeType;
                            }
                            KeyCode::F(8) if !self.schema_edit && self.id.is_some() => {
                                let current=schema::load(store,&self.kind)?.context("No saved schema yet. Close this form and press s to define one.")?;
                                self.migration =
                                    Some(schema::preview(&self.draft["data"], current));
                                self.choice = 0;
                                self.migration_scroll = 0;
                                self.mode = Mode::Migration;
                            }
                            _ => {}
                        }
                    }
                }
                Mode::Edit => {
                    if key.code == KeyCode::Enter || key.code == KeyCode::Tab {
                        self.commit_edit()?;
                        if key.code == KeyCode::Tab {
                            self.selected = (self.selected + 1).min(self.rows().len() - 1);
                        }
                    } else if ctrl && key.code == KeyCode::Char('j') {
                        self.editor
                            .handle(KeyEvent::new(KeyCode::Char('\n'), KeyModifiers::NONE));
                    } else {
                        self.editor.handle(key);
                    }
                }
                Mode::Control => {
                    if next {
                        self.choice = (self.choice + 1) % 3;
                    } else if prev {
                        self.choice = (self.choice + 2) % 3;
                    } else if key.code == KeyCode::Enter {
                        let control = self.controls.entry(self.control_path.clone()).or_default();
                        if self.choice == 0 {
                            control.required = !control.required;
                        } else {
                            self.property = self.choice;
                            let text = if self.property == 1 {
                                control.choices.join("\n")
                            } else {
                                control.description.clone()
                            };
                            self.editor.set(&text);
                            self.mode = Mode::PropertyEdit;
                        }
                    }
                }
                Mode::PropertyEdit => {
                    if key.code == KeyCode::Enter {
                        let text = self.editor.text();
                        if self.property == 1 && self.current().kind != Kind::Text {
                            bail!("Dropdown choices can only be added to a Text field");
                        }
                        let control = self.controls.entry(self.control_path.clone()).or_default();
                        if self.property == 1 {
                            let mut choices = vec![];
                            for line in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
                                if !choices.contains(&line.to_owned()) {
                                    choices.push(line.to_owned());
                                }
                            }
                            control.choices = choices;
                        } else {
                            control.description = text;
                        }
                        self.mode = Mode::Control;
                    } else if ctrl && key.code == KeyCode::Char('j') {
                        self.editor
                            .handle(KeyEvent::new(KeyCode::Char('\n'), KeyModifiers::NONE));
                    } else {
                        self.editor.handle(key);
                    }
                }
                Mode::AddName => {
                    if key.code == KeyCode::Enter {
                        let name = self.editor.text();
                        if name.trim().is_empty() {
                            bail!(
                                "Give the new field a name, for example Birthday or Account number"
                            );
                        }
                        if self
                            .draft
                            .pointer(&self.parent)
                            .and_then(Value::as_object)
                            .is_some_and(|o| o.contains_key(name.trim()))
                        {
                            bail!("This field already exists. Choose a different name.");
                        }
                        self.mode = Mode::Type;
                        self.choice = 0;
                    } else {
                        self.editor.handle(key);
                    }
                }
                Mode::Type
                | Mode::Choice
                | Mode::RecordType
                | Mode::Discard
                | Mode::ChangeType
                | Mode::Migration => {
                    let count = match self.mode {
                        Mode::Type | Mode::ChangeType => self.types().len(),
                        Mode::Choice => self.choices.len(),
                        Mode::RecordType => self.available_types.len(),
                        _ => 2,
                    };
                    if next || key.code == KeyCode::Tab {
                        self.choice = (self.choice + 1) % count;
                    } else if prev || key.code == KeyCode::BackTab {
                        self.choice = (self.choice + count - 1) % count;
                    } else if key.code == KeyCode::Enter {
                        match self.mode {
                            Mode::ChangeType => {
                                let row = self.current();
                                let kind = self.types()[self.choice].clone();
                                self.set(&row.path, kind.blank())?;
                                self.hints.insert(
                                    row.path,
                                    (
                                        kind,
                                        "User-defined field. Enter edits its value/default.".into(),
                                    ),
                                );
                                self.mode = Mode::Fields;
                            }
                            Mode::Migration => {
                                if self.choice == 1 {
                                    let migration =
                                        self.migration.as_ref().context("No migration preview")?;
                                    if !migration.conflicts.is_empty() {
                                        bail!(
                                            "Resolve the listed conflicts before applying. Esc returns to the fields."
                                        );
                                    }
                                    self.draft["data"] = migration.data.clone();
                                    self.hints.extend(migration.schema.hints.clone());
                                    self.controls = migration.schema.controls.clone();
                                    self.record_version = migration.schema.version;
                                    self.current_schema = Some(migration.schema.clone());
                                    self.selected = 0;
                                }
                                self.migration = None;
                                self.mode = Mode::Fields;
                            }
                            Mode::Type => {
                                let kind = self.types()[self.choice].clone();
                                self.insert_field(kind)?;
                                self.start_edit(records);
                            }
                            Mode::Choice => {
                                let value = self.choices[self.choice].1.clone();
                                if value.get("custom").is_some() {
                                    self.editor.set(
                                        self.draft
                                            .pointer(&self.path)
                                            .and_then(Value::as_str)
                                            .unwrap_or(""),
                                    );
                                    self.mode = Mode::Edit;
                                } else {
                                    self.set(&self.path.clone(), value)?;
                                    self.mode = Mode::Fields;
                                }
                            }
                            Mode::RecordType => {
                                self.kind = self.available_types[self.choice].slug.clone();
                                self.draft = json!({"name":"","data":{}});
                                self.hints.clear();
                                self.populate(records);
                                self.restore_schema(store)?;
                                self.mode = Mode::Fields;
                            }
                            Mode::Discard => {}
                            _ => {}
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            self.error = e.to_string();
        }
        if self.mode == Mode::Discard && key.code == KeyCode::Enter {
            if self.choice == 1 {
                return Outcome::Cancel;
            }
            self.mode = Mode::Fields;
        }
        Outcome::Continue
    }
    pub fn render(&mut self, frame: &mut Frame, p: Palette, records: &[Value]) {
        let area = frame.area();
        frame.render_widget(Block::default().style(p.base()), area);
        if area.width < 45 || area.height < 15 {
            frame.render_widget(
                Paragraph::new(
                    "Enlarge the terminal to at least 45 × 15 for data entry.\nEsc cancels.",
                )
                .style(p.base()),
                area,
            );
            return;
        }
        let areas = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(5),
            Constraint::Length(3),
        ])
        .split(area);
        let action = if self.schema_edit {
            "Schema"
        } else if self.id.is_some() {
            "Edit"
        } else {
            "New"
        };
        let current = self.current_schema.as_ref().map(|s| s.version).unwrap_or(0);
        let description = if self.schema_edit {
            format!(
                "Define fields and defaults for new {} records. F2 adds; F5 changes type. Save publishes v{}.",
                self.kind,
                current + 1
            )
        } else {
            format!(
                "Applied schema v{} • Current v{} • {}",
                self.record_version,
                current,
                if self.id.is_some() {
                    "F8 previews migration. Save commits this record."
                } else {
                    "* Name required. Save creates this record."
                }
            )
        };
        frame.render_widget(
            Paragraph::new(description)
                .wrap(Wrap { trim: false })
                .style(p.base())
                .block(
                    Block::bordered()
                        .title(format!(" {action} {} ", details::label(&self.kind)))
                        .border_style(p.base().fg(p.accent)),
                ),
            areas[0],
        );
        let rows = self.rows();
        let picking = matches!(
            self.mode,
            Mode::Type
                | Mode::Choice
                | Mode::RecordType
                | Mode::Discard
                | Mode::ChangeType
                | Mode::Migration
                | Mode::Control
        );
        if self.mode == Mode::Migration {
            if let Some(migration) = &self.migration {
                let panes =
                    Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(areas[1]);
                let mut lines = vec![
                    format!(
                        "Schema v{} → v{} • {} changes • {} conflicts",
                        self.record_version,
                        migration.schema.version,
                        migration.changes.len(),
                        migration.conflicts.len()
                    ),
                    "Extra fields are kept. Save commits; Esc leaves the draft unchanged.".into(),
                ];
                lines.extend(migration.conflicts.iter().map(|s| format!("CONFLICT: {s}")));
                if let Err(error) = migration.schema.validate(&migration.data) {
                    lines.push(format!("Before saving: {error}"));
                }
                lines.extend(migration.changes.clone());
                if migration.changes.is_empty() {
                    lines.push("No field additions or conversions needed.".into());
                }
                let report = Paragraph::new(lines.join("\n"))
                    .style(p.base())
                    .wrap(Wrap { trim: false });
                let max = report
                    .line_count(panes[0].width.saturating_sub(2))
                    .saturating_sub(panes[0].height.saturating_sub(2) as usize)
                    .min(u16::MAX as usize) as u16;
                self.migration_scroll = self.migration_scroll.min(max);
                frame.render_widget(
                    report.scroll((self.migration_scroll, 0)).block(
                        Block::bordered()
                            .title(" Migration preview • PgUp/PgDn ")
                            .border_style(p.base().fg(p.accent)),
                    ),
                    panes[0],
                );
                let mut state = ListState::default().with_selected(Some(self.choice));
                frame.render_stateful_widget(
                    List::new(["Keep editing without migrating", "Apply migration to draft"])
                        .style(p.base())
                        .highlight_symbol("› ")
                        .highlight_style(p.base().bg(p.selection)),
                    panes[1],
                    &mut state,
                );
            }
        } else if picking {
            let (title, labels): (String, Vec<String>) = match self.mode {
                Mode::ChangeType => (
                    format!(
                        "Change {} type — clears its value/default",
                        self.current().label
                    ),
                    self.types().iter().map(Kind::title).collect(),
                ),
                Mode::Type => (
                    if self
                        .draft
                        .pointer(&self.parent)
                        .is_some_and(Value::is_array)
                    {
                        "Choose a list item type".into()
                    } else {
                        format!("Field: {} — choose its type", self.editor.text().trim())
                    },
                    self.types().iter().map(Kind::title).collect(),
                ),
                Mode::Control => {
                    let control = self
                        .controls
                        .get(&self.control_path)
                        .cloned()
                        .unwrap_or_default();
                    (
                        format!("Field rules: {}", self.current().label),
                        vec![
                            format!("Required: {}", if control.required { "Yes" } else { "No" }),
                            format!("Dropdown choices: {}", control.choices.join(" • ")),
                            format!("Description: {}", control.description),
                        ],
                    )
                }
                Mode::Choice => (
                    format!("Choose {}", self.current().label),
                    self.choices.iter().map(|c| c.0.clone()).collect(),
                ),
                Mode::RecordType => (
                    if self.schema_edit {
                        "Which record schema would you like to define?".into()
                    } else {
                        "What would you like to create?".into()
                    },
                    self.available_types
                        .iter()
                        .map(|t| match t.slug.as_str() {
                            "contact" => "Contact — a person".into(),
                            "company" => "Company — an organization".into(),
                            "deal" => "Deal — a sales opportunity".into(),
                            "task" => "Task — a follow-up or to-do".into(),
                            "note" => "Note — activity or meeting notes".into(),
                            _ => format!("{} ({}) — custom record type", t.label, t.slug),
                        })
                        .collect(),
                ),
                _ => (
                    "Discard unsaved changes?".into(),
                    vec!["Keep editing".into(), "Discard changes".into()],
                ),
            };
            let mut state = ListState::default().with_selected(Some(self.choice));
            frame.render_stateful_widget(
                List::new(labels)
                    .style(p.base())
                    .highlight_style(p.base().bg(p.selection).fg(p.accent))
                    .highlight_symbol("› ")
                    .block(
                        Block::bordered()
                            .title(title)
                            .border_style(p.base().fg(p.accent)),
                    ),
                areas[1],
                &mut state,
            );
        } else if matches!(self.mode, Mode::Edit | Mode::AddName | Mode::PropertyEdit) {
            let before: String = self.editor.chars[..self.editor.cursor].iter().collect();
            let line = before.split('\n').count().saturating_sub(1);
            let col = Line::from(before.rsplit('\n').next().unwrap_or("")).width();
            let height = areas[1].height.saturating_sub(2) as usize;
            let width = areas[1].width.saturating_sub(2) as usize;
            let yscroll = line.saturating_sub(height.saturating_sub(1));
            let xscroll = col.saturating_sub(width.saturating_sub(1));
            frame.render_widget(
                Paragraph::new(self.editor.text())
                    .style(p.base())
                    .scroll((
                        yscroll.min(u16::MAX as usize) as u16,
                        xscroll.min(u16::MAX as usize) as u16,
                    ))
                    .block(
                        Block::bordered()
                            .title(if self.mode == Mode::Edit {
                                format!(" Editing {} ", self.current().label)
                            } else if self.mode == Mode::PropertyEdit {
                                if self.property == 1 {
                                    " Dropdown choices • one per line • C-j new line ".into()
                                } else {
                                    " Field description ".into()
                                }
                            } else {
                                " New field name (required) ".into()
                            })
                            .border_style(p.base().fg(p.accent)),
                    ),
                areas[1],
            );
            frame.set_cursor_position((
                areas[1].x + 1 + (col - xscroll) as u16,
                areas[1].y + 1 + (line - yscroll) as u16,
            ));
        } else {
            let items: Vec<ListItem> = rows
                .iter()
                .map(|r| {
                    let v = self.draft.pointer(&r.path).unwrap_or(&Value::Null);
                    let value = preview(v, records);
                    ListItem::new(vec![
                        Line::from(Span::styled(
                            format!("{}{}  ({})", "  ".repeat(r.depth), r.label, r.kind.title()),
                            p.base().add_modifier(Modifier::BOLD),
                        )),
                        Line::from(format!("{}  {}", "  ".repeat(r.depth), value)),
                    ])
                })
                .collect();
            let mut state = ListState::default().with_selected(Some(self.selected));
            frame.render_stateful_widget(
                List::new(items)
                    .style(p.base())
                    .highlight_style(p.base().bg(p.selection))
                    .highlight_symbol("› ")
                    .block(
                        Block::bordered()
                            .title(" Fields • Enter: edit • F2: named field • F4: list item ")
                            .border_style(p.base().fg(p.accent)),
                    ),
                areas[1],
                &mut state,
            );
        }
        let row = self.current();
        let instructions = if !self.error.is_empty() {
            format!("Please check: {}", self.error)
        } else if self.prefix {
            if self.schema_edit {
                "C-x → C-s Save schema • C-g Cancel shortcut".into()
            } else {
                "C-x → C-s Save record • C-g Cancel shortcut".into()
            }
        } else if self.schema_edit && self.mode == Mode::Fields {
            "Schema editor: Enter sets a default. F2 adds a field, F3 removes it, F5 changes its type. F10 saves a new schema version; existing records are not changed.".into()
        } else {
            match self.mode {
            Mode::Fields=>self.controls.get(&row.path).filter(|c|!c.description.is_empty()).map(|c|c.description.clone()).or_else(||self.hints.get(&row.path).map(|h|h.1.clone())).unwrap_or_else(||if row.path=="/name"{"Required: a clear name or title for this record.".into()}else{"Select a field. Enter edits it; F2 adds a field here.".into()}),
            Mode::Edit=>format!("Editing {} • Enter applies • C-g cancels edit\n{}",row.label,if self.edit_kind==Kind::Date{"Date format: YYYY-MM-DD"}else if self.edit_kind==Kind::Number{"Enter a number; empty means not set"}else{"C-a/e: start/end • C-k: kill • C-y: yank • C-j: new line"}),
            Mode::AddName=>if self.schema_edit {"Name a reusable schema field (for example Birthday). Enter chooses its type and default.".into()}else{"Name a field for this record. For reusable fields, use s in the records view to edit the schema. Enter continues.".into()},
            Mode::Choice=>if matches!(row.kind,Kind::Link(_)){"Choose a record by name. If none exist, cancel, save this draft, then create the linked record first.".into()}else{"Choose an option. Enter applies; C-g cancels.".into()},
            Mode::Type=>"Text, numbers, yes/no, dates, nested groups, lists or a link to another record. Enter selects.".into(),
            Mode::ChangeType=>"Choose the new type. Its previous default/value is cleared in this draft only; Esc keeps the original.".into(),
            Mode::Migration=>"Choose Apply to update this draft. Conflicts must be resolved first. Save commits the migration; cancelling leaves the saved record unchanged.".into(),
            Mode::RecordType=>"Choose the type of record with ↑/↓ or C-n/C-p, then Enter.".into(),
            Mode::Discard=>"Keep editing preserves your draft. Discard leaves the saved record unchanged.".into(),
            Mode::Control=>"Enter toggles Required or edits choices/description. Esc returns to fields. F7/F9 moves the selected field up/down in the schema.".into(),
            Mode::PropertyEdit=>if self.property==1{"One choice per line; C-j inserts a new line. Empty removes the dropdown. Enter applies.".into()}else{"Describe this field for people filling the form. Enter applies; C-j adds a new line.".into()},
        }
        };
        let lines = instructions;
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .style(p.base())
                .wrap(Wrap { trim: false })
                .block(
                    Block::bordered()
                        .title(" Guidance ")
                        .border_style(p.base().fg(p.accent)),
                ),
            areas[2],
        );
        let footer = if self.mode == Mode::Migration {
            "↑/↓: choose • PgUp/PgDn: scroll preview • Enter: select • Esc: back"
        } else if picking {
            "↑/↓ or C-n/p: choose • Enter: select • Esc/C-g: back"
        } else if self.mode == Mode::Edit {
            "Enter: apply edit • Tab: apply + next • C-j: new line\nF10 / C-x C-s: Save record • C-g: cancel this edit"
        } else if self.mode == Mode::AddName {
            "Type a field name • Enter: choose its type • C-g: back"
        } else {
            if self.schema_edit {
                "Tab / C-n/p: move • Enter: default • F2: field • F3: remove\nF4: item • F5: type • F6: field rules • F7/F9: field order\nF10 / C-x C-s: Save schema • Esc / C-g: cancel"
            } else {
                "Tab / C-n/p: move • Enter: edit • F2: add field • F3: remove\nF4: list item • F5: type • F8: migrate to current schema\nF10 / C-x C-s: Save record • Esc / C-g: cancel"
            }
        };
        frame.render_widget(Paragraph::new(footer).style(p.base().fg(p.muted)), areas[3]);
    }
}
fn preview(v: &Value, records: &[Value]) -> String {
    match v {
        Value::Null => "Not set".into(),
        Value::String(s) => records
            .iter()
            .find(|r| r["id"] == *s)
            .and_then(|r| r["name"].as_str())
            .unwrap_or(s)
            .replace('\n', " ↵ "),
        Value::Bool(b) => if *b { "Yes" } else { "No" }.into(),
        Value::Number(n) => n.to_string(),
        Value::Object(m) => format!("{} fields — F2 adds a named field", m.len()),
        Value::Array(a) => format!("{} items — Enter or F4 adds an item", a.len()),
    }
}
fn valid_date(s: &str) -> bool {
    if s.len() != 10 || s.as_bytes()[4] != b'-' || s.as_bytes()[7] != b'-' {
        return false;
    }
    let parts: Vec<_> = s.split('-').filter_map(|p| p.parse::<u32>().ok()).collect();
    if parts.len() != 3 {
        return false;
    }
    let (y, m, d) = (parts[0], parts[1], parts[2]);
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let max = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => return false,
    };
    y > 0 && d > 0 && d <= max
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    fn store() -> Store {
        Store::open(std::path::Path::new(":memory:")).unwrap()
    }
    fn key(f: &mut Form, s: &Store, records: &[Value], code: KeyCode) -> Outcome {
        f.handle(s, records, KeyEvent::new(code, KeyModifiers::NONE))
    }
    fn choose(f: &mut Form, path: &str) {
        f.selected = f
            .rows()
            .iter()
            .position(|r| r.path == path)
            .unwrap_or_else(|| panic!("Missing {path}"));
    }
    fn text(f: &mut Form, s: &Store, records: &[Value], path: &str, value: &str) {
        choose(f, path);
        key(f, s, records, KeyCode::Enter);
        assert!(f.mode == Mode::Edit);
        f.editor.set(value);
        key(f, s, records, KeyCode::Enter);
        assert!(f.mode == Mode::Fields, "{}", f.error);
    }
    fn saved(outcome: Outcome) -> Value {
        if let Outcome::Saved(record) = outcome {
            record
        } else {
            panic!("Expected saved record")
        }
    }
    #[test]
    fn guided_creation_for_every_kind_and_field_validation() {
        let s = store();
        for kind in ["contact", "company", "deal", "task", "note"] {
            let mut f = Form::new(Some(kind), None, &[]);
            key(&mut f, &s, &[], KeyCode::F(10));
            assert!(f.error.contains("required"));
            text(&mut f, &s, &[], "/name", &format!("Test {kind}"));
            for (field, _, _) in defaults(kind) {
                assert!(f.rows().iter().any(|r| r.path == child("/data", field)));
            }
            if kind == "deal" {
                choose(&mut f, "/data/value");
                key(&mut f, &s, &[], KeyCode::Enter);
                f.editor.set("not a number");
                key(&mut f, &s, &[], KeyCode::Enter);
                assert!(f.mode == Mode::Edit);
                assert!(f.error.contains("number"));
                f.editor.set("1250.50");
                key(&mut f, &s, &[], KeyCode::Enter);
                text(&mut f, &s, &[], "/data/expected_close", "2028-02-29");
            }
            if kind == "task" {
                choose(&mut f, "/data/due");
                key(&mut f, &s, &[], KeyCode::Enter);
                f.editor.set("2027-02-29");
                key(&mut f, &s, &[], KeyCode::Enter);
                assert!(f.mode == Mode::Edit);
                assert!(f.error.contains("real date"));
                f.editor.set("2028-02-29");
                key(&mut f, &s, &[], KeyCode::Enter);
                choose(&mut f, "/data/status");
                key(&mut f, &s, &[], KeyCode::Enter);
                assert!(f.mode == Mode::Choice);
                f.choice = 2;
                key(&mut f, &s, &[], KeyCode::Enter);
            }
            if kind == "note" {
                text(
                    &mut f,
                    &s,
                    &[],
                    "/data/body",
                    "Meeting summary\nNext steps: send proposal",
                );
            }
            let record = saved(key(&mut f, &s, &[], KeyCode::F(10)));
            assert_eq!(record["kind"], kind);
            assert_eq!(record["name"], format!("Test {kind}"));
            if kind == "deal" {
                assert_eq!(record["data"]["value"], 1250.5);
            }
            if kind == "task" {
                assert_eq!(record["data"]["status"], "done");
            }
            if kind == "note" {
                assert!(record["data"]["body"].as_str().unwrap().contains('\n'));
            }
        }
        assert_eq!(
            s.execute(&["export".into()])
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            5
        );
    }
    #[test]
    fn edits_preserve_nested_types_and_delete_fields_atomically() {
        let s = store();
        let data = json!({"address":{"city":"London","postal_code":"00123"},"scores":[1,true,null,{"label":"keep"}],"active":false,"exact":9007199254740993u64,"a/b~c":"escaped key","obsolete":"remove me"});
        let record = s
            .execute(&[
                "add".into(),
                "contact".into(),
                "Ada".into(),
                data.to_string(),
            ])
            .unwrap();
        let records = vec![record.clone()];
        let mut f = Form::new(None, Some(&record), &records);
        text(&mut f, &s, &records, "/data/address/city", "Paris");
        text(&mut f, &s, &records, "/data/a~1b~0c", "edited safely");
        choose(&mut f, "/data/active");
        key(&mut f, &s, &records, KeyCode::Enter);
        f.choice = 1;
        key(&mut f, &s, &records, KeyCode::Enter);
        choose(&mut f, "/data/obsolete");
        key(&mut f, &s, &records, KeyCode::F(3));
        let saved = saved(key(&mut f, &s, &records, KeyCode::F(10)));
        assert_eq!(saved["data"]["address"]["city"], "Paris");
        assert_eq!(saved["data"]["address"]["postal_code"], "00123");
        assert_eq!(saved["data"]["scores"], data["scores"]);
        assert_eq!(saved["data"]["exact"], data["exact"]);
        assert_eq!(saved["data"]["active"], true);
        assert_eq!(saved["data"]["a/b~c"], "edited safely");
        assert!(saved["data"].get("obsolete").is_none());
        assert_eq!(
            s.execute(&["get".into(), record["id"].as_str().unwrap().into()])
                .unwrap()["data"],
            saved["data"]
        );
    }
    #[test]
    fn dynamic_fields_links_and_list_items_need_no_json() {
        let s = store();
        let company = s
            .execute(&["add".into(), "company".into(), "Engine Works".into()])
            .unwrap();
        let peer = json!({"kind":"contact","data":{"loyalty_points":42,"preferences":{"language":"English"}}});
        let records = vec![company.clone(), peer];
        let mut f = Form::new(Some("contact"), None, &records);
        text(&mut f, &s, &records, "/name", "Ada");
        text(&mut f, &s, &records, "/data/loyalty_points", "12");
        text(&mut f, &s, &records, "/data/preferences/language", "French");
        choose(&mut f, "/data/company_id");
        key(&mut f, &s, &records, KeyCode::Enter);
        assert!(f.choices[1].0.contains("Engine Works"));
        f.choice = 1;
        key(&mut f, &s, &records, KeyCode::Enter);
        choose(&mut f, "/data");
        key(&mut f, &s, &records, KeyCode::F(2));
        assert!(f.mode == Mode::AddName);
        f.editor.set("VIP");
        key(&mut f, &s, &records, KeyCode::Enter);
        assert!(f.mode == Mode::Type);
        f.choice = 2;
        key(&mut f, &s, &records, KeyCode::Enter);
        assert!(f.mode == Mode::Choice);
        f.choice = 1;
        key(&mut f, &s, &records, KeyCode::Enter);
        choose(&mut f, "/data/tags");
        key(&mut f, &s, &records, KeyCode::Enter);
        assert!(f.mode == Mode::Type);
        f.choice = 0;
        key(&mut f, &s, &records, KeyCode::Enter);
        f.editor.set("customer, partner");
        key(&mut f, &s, &records, KeyCode::Enter);
        let record = saved(key(&mut f, &s, &records, KeyCode::F(10)));
        assert_eq!(record["data"]["company_id"], company["id"]);
        assert_eq!(record["data"]["VIP"], true);
        assert_eq!(record["data"]["loyalty_points"], 12);
        assert_eq!(record["data"]["preferences"]["language"], "French");
        assert_eq!(record["data"]["tags"], json!(["customer, partner"]));
    }
    #[test]
    fn declared_empty_field_types_and_named_links_survive_reopening() {
        let s = store();
        let company = s
            .execute(&["add".into(), "company".into(), "Sponsor Co".into()])
            .unwrap();
        let records = vec![company.clone()];
        let mut f = Form::new(Some("contact"), None, &records);
        text(&mut f, &s, &records, "/name", "Ada");
        choose(&mut f, "/data");
        key(&mut f, &s, &records, KeyCode::F(2));
        f.editor.set("Score");
        key(&mut f, &s, &records, KeyCode::Enter);
        f.choice = 1;
        key(&mut f, &s, &records, KeyCode::Enter);
        key(&mut f, &s, &records, KeyCode::Esc);
        choose(&mut f, "/data");
        key(&mut f, &s, &records, KeyCode::F(2));
        f.editor.set("Sponsor");
        key(&mut f, &s, &records, KeyCode::Enter);
        f.choice = 7;
        key(&mut f, &s, &records, KeyCode::Enter);
        f.choice = 1;
        key(&mut f, &s, &records, KeyCode::Enter);
        let record = saved(key(&mut f, &s, &records, KeyCode::F(10)));
        let records = vec![company, record.clone()];
        let mut reopened = Form::new(None, Some(&record), &records);
        reopened.restore_schema(&s).unwrap();
        choose(&mut reopened, "/data/Score");
        assert_eq!(reopened.current().kind, Kind::Number);
        key(&mut reopened, &s, &records, KeyCode::Enter);
        reopened.editor.set("invalid");
        key(&mut reopened, &s, &records, KeyCode::Enter);
        assert!(reopened.error.contains("number"));
        key(&mut reopened, &s, &records, KeyCode::Esc);
        choose(&mut reopened, "/data/Sponsor");
        assert_eq!(reopened.current().kind, Kind::Link("company".into()));
        key(&mut reopened, &s, &records, KeyCode::Enter);
        assert!(
            reopened
                .choices
                .iter()
                .any(|(label, _)| label.contains("Sponsor Co"))
        );
    }
    #[test]
    fn add_field_always_requires_a_name_even_when_a_list_is_selected() {
        let s = store();
        let record=s.execute(&["add".into(),"contact".into(),"Ada".into(),json!({"email":"ada@example.com","tags":["customer"],"profile":{"nickname":"Ada"},"items":[{"label":"original"}]}).to_string()]).unwrap();
        let records = vec![record.clone()];
        for (selected, parent) in [
            ("/name", "/data"),
            ("/data", "/data"),
            ("/data/email", "/data"),
            ("/data/tags", "/data"),
            ("/data/tags/0", "/data"),
            ("/data/profile", "/data/profile"),
            ("/data/profile/nickname", "/data/profile"),
            ("/data/items/0", "/data/items/0"),
        ] {
            let mut f = Form::new(None, Some(&record), &records);
            choose(&mut f, selected);
            key(&mut f, &s, &records, KeyCode::F(2));
            assert!(f.mode == Mode::AddName);
            assert_eq!(f.parent, parent);
            key(&mut f, &s, &records, KeyCode::Enter);
            assert!(f.mode == Mode::AddName);
            assert!(f.error.contains("name"));
            f.editor.set("Account / tier~");
            key(&mut f, &s, &records, KeyCode::Enter);
            assert!(f.mode == Mode::Type);
            let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
            terminal
                .draw(|frame| {
                    f.render(
                        frame,
                        super::super::theme::Theme::Dracula.palette(),
                        &records,
                    )
                })
                .unwrap();
            let screen = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(screen.contains("Field: Account / tier~"));
            key(&mut f, &s, &records, KeyCode::Enter);
            f.editor.set("Gold");
            key(&mut f, &s, &records, KeyCode::Enter);
            let saved = saved(key(&mut f, &s, &records, KeyCode::F(10)));
            let root = json!({"data":saved["data"]});
            assert_eq!(
                root.pointer(&child(parent, "Account / tier~")),
                Some(&json!("Gold"))
            );
            assert_eq!(saved["data"]["tags"], json!(["customer"]));
        }
        let mut f = Form::new(None, Some(&record), &records);
        choose(&mut f, "/data/tags");
        key(&mut f, &s, &records, KeyCode::F(4));
        assert!(f.mode == Mode::Type);
        key(&mut f, &s, &records, KeyCode::Enter);
        f.editor.set("partner");
        key(&mut f, &s, &records, KeyCode::Enter);
        assert_eq!(
            saved(key(&mut f, &s, &records, KeyCode::F(10)))["data"]["tags"],
            json!(["customer", "partner"])
        );
    }
    fn publish(form: &mut Form, store: &Store, records: &[Value]) -> i64 {
        match key(form, store, records, KeyCode::F(10)) {
            Outcome::SchemaSaved(_, version) => version,
            _ => panic!("Schema save failed: {}", form.error),
        }
    }
    #[test]
    fn reusable_schema_replaces_starters_and_migration_is_explicit_and_draft_only() {
        let s = store();
        let legacy = s
            .execute(&[
                "add".into(),
                "contact".into(),
                "Ada".into(),
                json!({"email":"private@example.com","legacy":"keep"}).to_string(),
            ])
            .unwrap();
        let records = vec![legacy.clone()];
        let mut schema = Form::schema_editor(Some("contact"), &records);
        schema.restore_schema(&s).unwrap();
        choose(&mut schema, "/data/email");
        key(&mut schema, &s, &records, KeyCode::F(3));
        choose(&mut schema, "/data");
        key(&mut schema, &s, &records, KeyCode::F(2));
        schema.editor.set("Score");
        key(&mut schema, &s, &records, KeyCode::Enter);
        schema.choice = 1;
        key(&mut schema, &s, &records, KeyCode::Enter);
        schema.editor.set("10");
        key(&mut schema, &s, &records, KeyCode::Enter);
        assert_eq!(publish(&mut schema, &s, &records), 1);
        for _ in 0..2 {
            let mut new = Form::new(Some("contact"), None, &records);
            new.restore_schema(&s).unwrap();
            assert_eq!(new.draft["data"]["Score"], 10);
            assert!(new.draft["data"].get("email").is_none());
            assert_eq!(new.record_version, 1);
        }
        let mut edit = Form::new(None, Some(&legacy), &records);
        edit.restore_schema(&s).unwrap();
        assert_eq!(edit.draft["data"], legacy["data"]);
        key(&mut edit, &s, &records, KeyCode::F(8));
        assert!(edit.mode == Mode::Migration);
        assert!(edit.migration.as_ref().unwrap().conflicts.is_empty());
        key(&mut edit, &s, &records, KeyCode::Esc);
        assert_eq!(edit.draft["data"], legacy["data"]);
        key(&mut edit, &s, &records, KeyCode::F(8));
        edit.choice = 1;
        key(&mut edit, &s, &records, KeyCode::Enter);
        assert_eq!(edit.draft["data"]["Score"], 10);
        assert_eq!(edit.draft["data"]["email"], "private@example.com");
        assert_eq!(s.get(legacy["id"].as_str().unwrap()).unwrap(), legacy);
        key(&mut edit, &s, &records, KeyCode::Esc);
        assert!(edit.mode == Mode::Discard);
        edit.choice = 1;
        assert!(matches!(
            key(&mut edit, &s, &records, KeyCode::Enter),
            Outcome::Cancel
        ));
        assert_eq!(s.get(legacy["id"].as_str().unwrap()).unwrap(), legacy);
        let mut edit = Form::new(None, Some(&legacy), &records);
        edit.restore_schema(&s).unwrap();
        key(&mut edit, &s, &records, KeyCode::F(8));
        edit.choice = 1;
        key(&mut edit, &s, &records, KeyCode::Enter);
        let migrated = saved(key(&mut edit, &s, &records, KeyCode::F(10)));
        assert_eq!(migrated["schema_version"], 1);
        assert_eq!(migrated["data"]["legacy"], "keep");
        let mut revised = Form::schema_editor(Some("contact"), &records);
        revised.restore_schema(&s).unwrap();
        choose(&mut revised, "/data/Score");
        key(&mut revised, &s, &records, KeyCode::F(5));
        revised.choice = 0;
        key(&mut revised, &s, &records, KeyCode::Enter);
        text(&mut revised, &s, &records, "/data/Score", "Default score");
        assert_eq!(publish(&mut revised, &s, &records), 2);
        let mut edit = Form::new(None, Some(&migrated), &records);
        edit.restore_schema(&s).unwrap();
        key(&mut edit, &s, &records, KeyCode::F(8));
        assert!(edit.migration.as_ref().unwrap().conflicts.is_empty());
        edit.choice = 1;
        key(&mut edit, &s, &records, KeyCode::Enter);
        let migrated = saved(key(&mut edit, &s, &records, KeyCode::F(10)));
        assert_eq!(migrated["schema_version"], 2);
        assert_eq!(migrated["data"]["Score"], "10");
    }
    #[test]
    fn schema_type_conflicts_block_migration_until_resolved_and_stale_edits_fail() {
        let s = store();
        let record = s
            .execute(&[
                "add".into(),
                "deal".into(),
                "Old deal".into(),
                json!({"value":"unknown"}).to_string(),
            ])
            .unwrap();
        let records = vec![record.clone()];
        let mut schema = Form::schema_editor(Some("deal"), &records);
        schema.restore_schema(&s).unwrap();
        choose(&mut schema, "/data/value");
        key(&mut schema, &s, &records, KeyCode::F(5));
        schema.choice = 1;
        key(&mut schema, &s, &records, KeyCode::Enter);
        publish(&mut schema, &s, &records);
        let mut edit = Form::new(None, Some(&record), &records);
        edit.restore_schema(&s).unwrap();
        key(&mut edit, &s, &records, KeyCode::F(8));
        assert_eq!(edit.migration.as_ref().unwrap().conflicts.len(), 1);
        edit.choice = 1;
        key(&mut edit, &s, &records, KeyCode::Enter);
        assert!(edit.mode == Mode::Migration);
        assert!(!edit.error.is_empty());
        assert_eq!(edit.draft["data"], record["data"]);
        key(&mut edit, &s, &records, KeyCode::Esc);
        choose(&mut edit, "/data/value");
        key(&mut edit, &s, &records, KeyCode::F(5));
        edit.choice = 1;
        key(&mut edit, &s, &records, KeyCode::Enter);
        text(&mut edit, &s, &records, "/data/value", "42");
        key(&mut edit, &s, &records, KeyCode::F(8));
        assert!(edit.migration.as_ref().unwrap().conflicts.is_empty());
        let mut first = Form::schema_editor(Some("deal"), &records);
        first.restore_schema(&s).unwrap();
        let mut stale = Form::schema_editor(Some("deal"), &records);
        stale.restore_schema(&s).unwrap();
        assert_eq!(publish(&mut first, &s, &records), 2);
        assert!(matches!(
            key(&mut stale, &s, &records, KeyCode::F(10)),
            Outcome::Continue
        ));
        assert!(stale.error.contains("changed"));
    }
    #[test]
    fn every_record_kind_can_have_an_empty_user_defined_schema() {
        let s = store();
        for kind in ["contact", "company", "deal", "task", "note"] {
            let mut schema = Form::schema_editor(Some(kind), &[]);
            schema.restore_schema(&s).unwrap();
            schema.draft["data"] = json!({});
            schema.hints.clear();
            assert_eq!(publish(&mut schema, &s, &[]), 1);
            let mut entry = Form::new(Some(kind), None, &[]);
            entry.restore_schema(&s).unwrap();
            assert_eq!(entry.rows().len(), 2);
            text(&mut entry, &s, &[], "/name", "No custom fields");
            let record = saved(key(&mut entry, &s, &[], KeyCode::F(10)));
            assert_eq!(record["data"], json!({}));
            assert_eq!(record["schema_version"], 1);
        }
    }
    #[test]
    fn choosing_another_record_type_does_not_inherit_the_previous_schema_version() {
        let s = store();
        schema::save(
            &s,
            "contact",
            0,
            &json!({"contact_only":"default"}),
            &BTreeMap::new(),
        )
        .unwrap();
        let mut form = Form::new(None, None, &[]);
        form.restore_schema(&s).unwrap();
        assert_eq!(form.record_version, 1);
        form.choice = 1;
        key(&mut form, &s, &[], KeyCode::Enter);
        assert_eq!(form.kind, "company");
        assert_eq!(form.record_version, 0);
        assert!(form.draft["data"].get("contact_only").is_none());
    }
    #[test]
    fn schema_rules_choices_descriptions_and_order_apply_to_forms_and_api() {
        let s = store();
        s.add_type("project", "Projects").unwrap();
        let mut schema = Form::schema_editor(Some("project"), &[]);
        schema.restore_schema(&s).unwrap();
        key(&mut schema, &s, &[], KeyCode::F(2));
        schema.editor.set("Stage");
        key(&mut schema, &s, &[], KeyCode::Enter);
        key(&mut schema, &s, &[], KeyCode::Enter);
        key(&mut schema, &s, &[], KeyCode::Enter);
        choose(&mut schema, "/data/Stage");
        key(&mut schema, &s, &[], KeyCode::F(6));
        key(&mut schema, &s, &[], KeyCode::Enter);
        key(&mut schema, &s, &[], KeyCode::Down);
        key(&mut schema, &s, &[], KeyCode::Enter);
        schema.editor.set(
            "Planned
Active
Done",
        );
        key(&mut schema, &s, &[], KeyCode::Enter);
        key(&mut schema, &s, &[], KeyCode::Down);
        key(&mut schema, &s, &[], KeyCode::Enter);
        schema.editor.set("Where this project stands");
        key(&mut schema, &s, &[], KeyCode::Enter);
        key(&mut schema, &s, &[], KeyCode::Esc);
        choose(&mut schema, "/data");
        key(&mut schema, &s, &[], KeyCode::F(2));
        schema.editor.set("Budget");
        key(&mut schema, &s, &[], KeyCode::Enter);
        schema.choice = 1;
        key(&mut schema, &s, &[], KeyCode::Enter);
        schema.editor.set("0");
        key(&mut schema, &s, &[], KeyCode::Enter);
        choose(&mut schema, "/data/Stage");
        key(&mut schema, &s, &[], KeyCode::F(7));
        publish(&mut schema, &s, &[]);
        let mut entry = Form::new(Some("project"), None, &[]);
        entry.restore_schema(&s).unwrap();
        assert_eq!(entry.rows()[2].path, "/data/Stage");
        assert!(entry.rows()[2].label.contains('*'));
        assert_eq!(
            entry.controls["/data/Stage"].description,
            "Where this project stands"
        );
        text(&mut entry, &s, &[], "/name", "Launch");
        key(&mut entry, &s, &[], KeyCode::F(10));
        assert!(entry.error.contains("required"));
        choose(&mut entry, "/data/Stage");
        key(&mut entry, &s, &[], KeyCode::Enter);
        assert_eq!(entry.choices.len(), 3);
        entry.choice = 1;
        key(&mut entry, &s, &[], KeyCode::Enter);
        let record = saved(key(&mut entry, &s, &[], KeyCode::F(10)));
        assert_eq!(record["data"]["Stage"], "Active");
        assert!(
            s.execute(&[
                "add".into(),
                "project".into(),
                "Bad".into(),
                json!({"Stage":"Unknown"}).to_string()
            ])
            .is_err()
        );
        let mut contact = Form::new(None, None, &[]);
        contact.restore_schema(&s).unwrap();
        contact.choice = 5;
        key(&mut contact, &s, &[], KeyCode::Enter);
        assert_eq!(contact.kind, "project");
        assert!(contact.types().contains(&Kind::Link("project".into())));
    }
    #[test]
    fn cancel_never_changes_saved_records_and_forms_explain_themselves() {
        let s = store();
        let record = s
            .execute(&[
                "add".into(),
                "note".into(),
                "Original".into(),
                json!({"body":"Keep this"}).to_string(),
            ])
            .unwrap();
        let records = vec![record.clone()];
        let mut f = Form::new(None, Some(&record), &records);
        text(&mut f, &s, &records, "/data/body", "Unsaved edit");
        key(&mut f, &s, &records, KeyCode::Esc);
        assert!(f.mode == Mode::Discard);
        f.choice = 1;
        assert!(matches!(
            key(&mut f, &s, &records, KeyCode::Enter),
            Outcome::Cancel
        ));
        assert_eq!(
            s.execute(&["get".into(), record["id"].as_str().unwrap().into()])
                .unwrap(),
            record
        );
        let mut f = Form::new(None, None, &records);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|frame| {
                f.render(
                    frame,
                    super::super::theme::Theme::Dracula.palette(),
                    &records,
                )
            })
            .unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(screen.contains("What would you like to create?"));
        assert!(screen.contains("Company — an organization"));
        key(&mut f, &s, &records, KeyCode::Enter);
        terminal
            .draw(|frame| {
                f.render(
                    frame,
                    super::super::theme::Theme::Dracula.palette(),
                    &records,
                )
            })
            .unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(screen.contains("Name / title *"));
        assert!(screen.contains("F10 / C-x C-s: Save record"));
        assert!(screen.contains("F2: add field"));
        assert!(!screen.contains("\"data\""));
    }
}
