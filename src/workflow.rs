use crate::*;
use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};

pub fn date() -> NaiveDate {
    Local::now().date_naive()
}
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}
fn record_date(s: &str) -> Option<NaiveDate> {
    parse_date(s).or_else(|| {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
            .ok()
            .map(|d| d.and_utc().with_timezone(&Local).date_naive())
    })
}
pub fn closed(record: &Value) -> bool {
    let status = record["data"]["status"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    [
        "done",
        "completed",
        "complete",
        "cancelled",
        "canceled",
        "closed",
    ]
    .contains(&status.as_str())
}
fn open_deal(record: &Value) -> bool {
    !matches!(
        record["data"]["stage"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .as_str(),
        "won" | "lost" | "closed" | "cancelled" | "canceled"
    )
}
fn linked(value: &Value, id: &str) -> bool {
    match value {
        Value::String(s) => s == id,
        Value::Array(a) => a.iter().any(|v| linked(v, id)),
        Value::Object(m) => m.values().any(|v| linked(v, id)),
        _ => false,
    }
}
fn related_ids(id: &str, records: &[Value]) -> std::collections::BTreeSet<String> {
    let mut ids = std::collections::BTreeSet::from([id.to_owned()]);
    let company = records
        .iter()
        .any(|r| r["id"] == id && r["kind"] == "company");
    loop {
        let mut added = false;
        for r in records {
            if !(matches!(r["kind"].as_str(), Some("deal" | "task" | "note"))
                || (company && r["kind"] == "contact"))
            {
                continue;
            }
            let rid = r["id"].as_str().unwrap_or("");
            if !ids.contains(rid) && ids.iter().any(|id| linked(&r["data"], id)) {
                added |= ids.insert(rid.into());
            }
        }
        if !added {
            break;
        }
    }
    ids
}
pub fn last_contact(contact: &Value, records: &[Value]) -> Option<NaiveDate> {
    let mut dates = Vec::new();
    if let Some(d) = contact["data"]["last_contacted"]
        .as_str()
        .and_then(parse_date)
    {
        dates.push(d);
    }
    let id = contact["id"].as_str().unwrap_or("");
    for r in records.iter().filter(|r| {
        linked(&r["data"], id) && (r["kind"] == "note" || (r["kind"] == "task" && closed(r)))
    }) {
        let d = r["data"]["occurred_on"]
            .as_str()
            .and_then(parse_date)
            .or_else(|| r["data"]["date"].as_str().and_then(parse_date))
            .or_else(|| r["data"]["completed_at"].as_str().and_then(parse_date))
            .or_else(|| {
                r[if r["kind"] == "task" {
                    "updated"
                } else {
                    "created"
                }]
                .as_str()
                .and_then(record_date)
            });
        if let Some(d) = d {
            dates.push(d);
        }
    }
    dates.into_iter().max()
}
fn follow_up(contact: &Value, records: &[Value], today: NaiveDate) -> bool {
    if let Some(d) = contact["data"]["next_contact"]
        .as_str()
        .or_else(|| contact["data"]["next_contact_date"].as_str())
        .and_then(parse_date)
    {
        return d <= today;
    }
    let days = contact["data"]["follow_up_days"]
        .as_i64()
        .unwrap_or(30)
        .clamp(1, 36500);
    last_contact(contact, records).is_none_or(|d| d <= today - Duration::days(days))
}
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    #[default]
    All,
    OverdueTasks,
    ActiveDeals,
    FollowUpContacts,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Equals,
    NotEquals,
    Contains,
    Before,
    After,
    Empty,
    NotEmpty,
    OlderThanDays,
}
impl Op {
    pub const ALL: [Self; 8] = [
        Self::Equals,
        Self::NotEquals,
        Self::Contains,
        Self::Before,
        Self::After,
        Self::Empty,
        Self::NotEmpty,
        Self::OlderThanDays,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            Self::Equals => "is",
            Self::NotEquals => "is not",
            Self::Contains => "contains",
            Self::Before => "before date",
            Self::After => "after date",
            Self::Empty => "is empty",
            Self::NotEmpty => "is not empty",
            Self::OlderThanDays => "older than days",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Filter {
    pub field: String,
    pub op: Op,
    #[serde(default)]
    pub value: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct View {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub rule: Rule,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default)]
    pub descending: bool,
    #[serde(default = "default_columns")]
    pub columns: Vec<String>,
}
fn default_sort() -> String {
    "name".into()
}
fn default_columns() -> Vec<String> {
    vec!["kind".into(), "status".into(), "due".into()]
}
impl View {
    pub fn blank() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: String::new(),
            rule: Rule::All,
            query: String::new(),
            filters: vec![],
            sort: default_sort(),
            descending: false,
            columns: default_columns(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("Give the view a name");
        }
        if !self.kind.is_empty() {
            validate_kind(&self.kind)?;
        }
        let required = match self.rule {
            Rule::All => None,
            Rule::OverdueTasks => Some("task"),
            Rule::ActiveDeals => Some("deal"),
            Rule::FollowUpContacts => Some("contact"),
        };
        if required.is_some_and(|kind| !self.kind.is_empty() && self.kind != kind) {
            bail!(
                "The base filter and record type do not match. Choose All records or the matching type."
            );
        }
        if self.sort.trim().is_empty() {
            bail!("Choose a sort field");
        }
        for f in &self.filters {
            if f.field.is_empty() {
                bail!("Choose a field for each filter");
            }
            match f.op {
                Op::Before | Op::After => {
                    if f.value != "$today" && parse_date(&f.value).is_none() {
                        bail!("Date filters need YYYY-MM-DD or $today");
                    }
                }
                Op::OlderThanDays
                    if f.value
                        .parse::<i64>()
                        .ok()
                        .filter(|d| (0..=36500).contains(d))
                        .is_none() =>
                {
                    bail!("Age filters need a number of days from 0 to 36500")
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn matches(&self, record: &Value, records: &[Value], today: NaiveDate) -> bool {
        if !self.kind.is_empty() && record["kind"] != self.kind {
            return false;
        }
        let rule = match self.rule {
            Rule::All => true,
            Rule::OverdueTasks => {
                record["kind"] == "task"
                    && !closed(record)
                    && record["data"]["due"]
                        .as_str()
                        .and_then(parse_date)
                        .is_some_and(|d| d < today)
            }
            Rule::ActiveDeals => record["kind"] == "deal" && open_deal(record),
            Rule::FollowUpContacts => {
                record["kind"] == "contact" && follow_up(record, records, today)
            }
        };
        rule && record
            .to_string()
            .to_lowercase()
            .contains(&self.query.to_lowercase())
            && self
                .filters
                .iter()
                .all(|f| f.matches(record, records, today))
    }
    pub fn select<'a>(&self, records: &'a [Value], today: NaiveDate) -> Vec<&'a Value> {
        let mut result: Vec<_> = records
            .iter()
            .filter(|r| self.matches(r, records, today))
            .collect();
        result.sort_by(|a, b| {
            let av = field(a, &self.sort, records);
            let bv = field(b, &self.sort, records);
            // Missing fields stay at the bottom in either direction.
            if av.is_null() != bv.is_null() {
                return av.is_null().cmp(&bv.is_null());
            }
            let order = if let (Some(a), Some(b)) = (av.as_f64(), bv.as_f64()) {
                a.total_cmp(&b)
            } else {
                friendly(&av, records)
                    .to_lowercase()
                    .cmp(&friendly(&bv, records).to_lowercase())
            };
            (if self.descending {
                order.reverse()
            } else {
                order
            })
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        result
    }
}
impl Filter {
    fn matches(&self, record: &Value, records: &[Value], today: NaiveDate) -> bool {
        let v = field(record, &self.field, records);
        let text = friendly(&v, records).to_lowercase();
        let target = self.value.trim().to_lowercase();
        let empty = v.is_null()
            || v.as_str().is_some_and(|s| s.trim().is_empty())
            || v.as_array().is_some_and(Vec::is_empty);
        let equal = if let Some(number) = v.as_f64() {
            self.value.parse::<f64>().is_ok_and(|n| n == number)
        } else {
            v.as_str().is_some_and(|s| s.to_lowercase() == target) || text == target
        };
        match self.op {
            Op::Equals => equal,
            Op::NotEquals => !equal,
            Op::Contains => text.contains(&target),
            Op::Empty => empty,
            Op::NotEmpty => !empty,
            Op::Before | Op::After => {
                let operand = if self.value == "$today" {
                    Some(today)
                } else {
                    parse_date(&self.value)
                };
                v.as_str()
                    .and_then(record_date)
                    .zip(operand)
                    .is_some_and(|(a, b)| if self.op == Op::Before { a < b } else { a > b })
            }
            Op::OlderThanDays => v
                .as_str()
                .and_then(record_date)
                .zip(self.value.parse::<i64>().ok())
                .is_some_and(|(d, days)| d < today - Duration::days(days.clamp(0, 36500))),
        }
    }
}
pub fn field(record: &Value, path: &str, records: &[Value]) -> Value {
    if path == "last_activity" {
        return last_contact(record, records)
            .map(|d| json!(d.to_string()))
            .unwrap_or(Value::Null);
    }
    if path.starts_with('/') {
        return record.pointer(path).cloned().unwrap_or(Value::Null);
    }
    if ["id", "name", "kind", "created", "updated", "schema_version"].contains(&path) {
        return record[path].clone();
    }
    let mut v = &record["data"];
    for key in path.split('.') {
        v = &v[key];
    }
    v.clone()
}
pub fn friendly(value: &Value, records: &[Value]) -> String {
    match value {
        Value::Null => "—".into(),
        Value::String(s) => records
            .iter()
            .find(|r| r["id"].as_str() == Some(s))
            .and_then(|r| r["name"].as_str())
            .unwrap_or(s)
            .replace('\n', " ↵ "),
        Value::Bool(b) => if *b { "Yes" } else { "No" }.into(),
        Value::Array(a) => a
            .iter()
            .map(|v| friendly(v, records))
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(m) => format!("{} fields", m.len()),
        _ => value.to_string(),
    }
}
#[derive(Clone, Serialize)]
pub struct AgendaItem {
    pub section: String,
    pub reason: String,
    pub record: Value,
}
pub fn agenda(records: &[Value], today: NaiveDate) -> Vec<AgendaItem> {
    let mut result = Vec::new();
    for r in records {
        let mut add = |section: &str, reason: String| {
            result.push(AgendaItem {
                section: section.into(),
                reason,
                record: r.clone(),
            })
        };
        if r["kind"] == "task" && !closed(r) {
            match r["data"]["due"].as_str().filter(|s| !s.trim().is_empty()) {
                None => add("Unscheduled tasks", "Choose a due date".into()),
                Some(raw) => match parse_date(raw) {
                    None => add("Dates to fix", format!("Invalid due date: {raw}")),
                    Some(d) => {
                        if d < today {
                            add(
                                "Overdue",
                                format!("Due {d} • {} days overdue", (today - d).num_days()),
                            );
                        } else if d == today {
                            add("Due today", format!("Due {d}"));
                        } else if d <= today + Duration::days(7) {
                            add("Next 7 days", format!("Due {d}"));
                        }
                    }
                },
            }
        } else if r["kind"] == "deal" && open_deal(r) {
            let id = r["id"].as_str().unwrap_or("");
            let next = records
                .iter()
                .any(|task| task["kind"] == "task" && !closed(task) && linked(&task["data"], id));
            if !next {
                add(
                    "Deals needing next action",
                    "No open follow-up task linked to this deal".into(),
                );
            } else if r["updated"]
                .as_str()
                .and_then(record_date)
                .is_some_and(|d| d < today - Duration::days(30))
            {
                add("Stale deals", "No record update in over 30 days".into());
            }
        } else if r["kind"] == "contact" && follow_up(r, records, today) {
            add(
                "Contacts to follow up",
                last_contact(r, records)
                    .map(|d| format!("Last interaction {d}"))
                    .unwrap_or("No interactions recorded".into()),
            );
        }
    }
    let order = [
        "Overdue",
        "Due today",
        "Dates to fix",
        "Next 7 days",
        "Unscheduled tasks",
        "Deals needing next action",
        "Stale deals",
        "Contacts to follow up",
    ];
    result.sort_by(|a, b| {
        order
            .iter()
            .position(|s| *s == a.section)
            .cmp(&order.iter().position(|s| *s == b.section))
            .then_with(|| {
                a.record["data"]["due"]
                    .as_str()
                    .cmp(&b.record["data"]["due"].as_str())
            })
            .then_with(|| a.record["name"].as_str().cmp(&b.record["name"].as_str()))
    });
    result
}
pub fn initialize(store: &Store) -> Result<()> {
    store.conn.execute_batch("CREATE TABLE IF NOT EXISTS saved_views (id VARCHAR PRIMARY KEY, name VARCHAR UNIQUE NOT NULL, definition VARCHAR NOT NULL); CREATE SEQUENCE IF NOT EXISTS activity_event_order START 1; CREATE TABLE IF NOT EXISTS activity_events(id VARCHAR PRIMARY KEY,record_id VARCHAR NOT NULL,kind VARCHAR NOT NULL,name VARCHAR NOT NULL,action VARCHAR NOT NULL,before_record VARCHAR,after_record VARCHAR,happened_at TIMESTAMP DEFAULT current_timestamp,event_order BIGINT DEFAULT nextval('activity_event_order')); ALTER TABLE activity_events ADD COLUMN IF NOT EXISTS event_order BIGINT DEFAULT nextval('activity_event_order'); CREATE TABLE IF NOT EXISTS preferences(key VARCHAR PRIMARY KEY,value VARCHAR NOT NULL)")?;
    if store
        .conn
        .query_row(
            "SELECT value FROM preferences WHERE key='views_initialized'",
            [],
            |r| r.get::<_, String>(0),
        )
        .is_err()
    {
        store.atomic(|| {
            for (id, name, kind, rule) in [
                (
                    "preset-overdue",
                    "Overdue tasks",
                    "task",
                    Rule::OverdueTasks,
                ),
                ("preset-deals", "Active deals", "deal", Rule::ActiveDeals),
                (
                    "preset-followup",
                    "Contacts needing follow-up",
                    "contact",
                    Rule::FollowUpContacts,
                ),
            ] {
                let view = View {
                    id: id.into(),
                    name: name.into(),
                    kind: kind.into(),
                    rule,
                    ..View::blank()
                };
                store.save_view(&view)?;
            }
            store.conn.execute(
                "INSERT INTO preferences VALUES ('views_initialized','yes')",
                [],
            )?;
            Ok(())
        })?;
    }
    Ok(())
}
impl Store {
    pub fn atomic<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        if self.in_transaction.get() {
            return f();
        }
        let tx = self.conn.unchecked_transaction()?;
        self.in_transaction.set(true);
        struct Reset<'a>(&'a std::cell::Cell<bool>);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _reset = Reset(&self.in_transaction);
        match f() {
            Ok(value) => {
                tx.commit()?;
                Ok(value)
            }
            Err(error) => Err(error),
        }
    }
    pub fn all_records(&self) -> Result<Vec<Value>> {
        Ok(self
            .execute(&["export".into()])?
            .as_array()
            .cloned()
            .unwrap_or_default())
    }
    pub fn views(&self) -> Result<Vec<View>> {
        let mut stmt = self
            .conn
            .prepare("SELECT definition FROM saved_views ORDER BY lower(name),id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn save_view(&self, view: &View) -> Result<View> {
        view.validate()?;
        if !view.kind.is_empty() {
            self.ensure_kind(&view.kind)?;
        }
        let mut view = view.clone();
        view.name = view.name.trim().into();
        if view.id.is_empty() {
            view.id = uuid::Uuid::new_v4().to_string();
        }
        self.conn.execute("INSERT INTO saved_views VALUES (?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,definition=excluded.definition",params![view.id,view.name,serde_json::to_string(&view)?])?;
        Ok(view)
    }
    pub fn find_view(&self, id: &str) -> Result<View> {
        self.views()?
            .into_iter()
            .find(|v| v.id == id || v.name == id)
            .context("Saved view not found")
    }
    pub fn delete_view(&self, id: &str) -> Result<()> {
        let view = self.find_view(id)?;
        self.conn
            .execute("DELETE FROM saved_views WHERE id=?", params![view.id])?;
        Ok(())
    }
    pub fn log_change(
        &self,
        before: Option<&Value>,
        after: Option<&Value>,
        action: &str,
    ) -> Result<()> {
        let record = after.or(before).context("Missing activity record")?;
        self.conn.execute("INSERT INTO activity_events(id,record_id,kind,name,action,before_record,after_record) VALUES (?,?,?,?,?,?,?)",params![uuid::Uuid::new_v4().to_string(),record["id"].as_str().unwrap_or(""),record["kind"].as_str().unwrap_or(""),record["name"].as_str().unwrap_or(""),action,before.map(Value::to_string),after.map(Value::to_string)])?;
        Ok(())
    }
    pub fn timeline(&self, id: &str) -> Result<Value> {
        let record = self.get(id)?;
        let records = self.all_records()?;
        let mut related = related_ids(id, &records);
        let mut aliases = self
            .conn
            .prepare("SELECT source_id FROM record_merges WHERE target_id=?")?;
        for alias in aliases.query_map(params![id], |r| r.get::<_, String>(0))? {
            related.insert(alias?);
        }
        let mut stmt=self.conn.prepare("SELECT id,record_id,kind,name,action,before_record,after_record,CAST(happened_at AS VARCHAR),event_order FROM activity_events ORDER BY happened_at DESC,event_order DESC")?;
        let mut rows = stmt.query([])?;
        let mut events = Vec::new();
        let mut created = std::collections::BTreeSet::new();
        while let Some(row) = rows.next()? {
            let event_id: String = row.get(0)?;
            let rid: String = row.get(1)?;
            let before: Option<String> = row.get(5)?;
            let after: Option<String> = row.get(6)?;
            let before: Value = before
                .map(|s| serde_json::from_str(&s))
                .transpose()?
                .unwrap_or(Value::Null);
            let after: Value = after
                .map(|s| serde_json::from_str(&s))
                .transpose()?
                .unwrap_or(Value::Null);
            let action: String = row.get(4)?;
            if action == "created" {
                created.insert(rid.clone());
            }
            if rid != id
                && !related
                    .iter()
                    .any(|id| linked(&before["data"], id) || linked(&after["data"], id))
            {
                continue;
            }
            let mut changes = Vec::new();
            if action == "updated" {
                if before["name"] != after["name"] {
                    changes.push("name".to_owned());
                }
                let keys: std::collections::BTreeSet<_> = before["data"]
                    .as_object()
                    .into_iter()
                    .flat_map(|m| m.keys())
                    .chain(after["data"].as_object().into_iter().flat_map(|m| m.keys()))
                    .collect();
                for key in keys {
                    if before["data"].get(key) != after["data"].get(key) {
                        changes.push(key.clone());
                    }
                }
            }
            events.push(json!({"id":event_id,"sequence":row.get::<_,i64>(8)?,"record_id":rid,"kind":row.get::<_,String>(2)?,"name":row.get::<_,String>(3)?,"action":action,"at":row.get::<_,String>(7)?,"changes":changes,"before":before.clone(),"after":after.clone(),"record":if after.is_null(){before}else{after}}));
        }
        for r in &records {
            let rid = r["id"].as_str().unwrap_or("");
            if related.contains(rid) && !created.contains(rid) {
                events.push(json!({"record_id":rid,"kind":r["kind"],"name":r["name"],"action":"created (imported history)","at":r["created"],"changes":[],"record":r}));
            }
        }
        events.sort_by(|a, b| {
            b["at"]
                .as_str()
                .cmp(&a["at"].as_str())
                .then_with(|| b["sequence"].as_i64().cmp(&a["sequence"].as_i64()))
        });
        let tasks: Vec<_> = records
            .iter()
            .filter(|r| {
                r["kind"] == "task"
                    && !closed(r)
                    && related.contains(r["id"].as_str().unwrap_or(""))
            })
            .collect();
        Ok(json!({"record":record,"open_tasks":tasks,"events":events}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(id: &str, kind: &str, data: Value) -> Value {
        json!({"id":id,"kind":kind,"name":id,"data":data,"created":"2026-11-01","updated":"2026-11-08"})
    }
    #[test]
    fn today_prioritizes_open_tasks_followups_and_deals_without_next_actions() {
        let today = parse_date("2026-11-09").unwrap();
        let records = vec![
            record("old", "contact", json!({"last_contacted":"2026-09-01"})),
            record("recent", "contact", json!({})),
            record("later", "contact", json!({"next_contact":"2026-12-01"})),
            record(
                "call",
                "note",
                json!({"contact_id":"recent","occurred_on":"2026-11-08"}),
            ),
            record(
                "overdue",
                "task",
                json!({"due":"2026-11-08","status":"open"}),
            ),
            record("today", "task", json!({"due":"2026-11-09"})),
            record("next", "task", json!({"due":"2026-11-16"})),
            record(
                "future",
                "task",
                json!({"due":"2026-12-01","deal_id":"planned"}),
            ),
            record("done", "task", json!({"due":"2026-01-01","status":"Done"})),
            record("cancelled", "task", json!({"status":"cancelled"})),
            record("invalid", "task", json!({"due":"soon"})),
            record("unscheduled", "task", json!({})),
            record("no-action", "deal", json!({"stage":"qualified"})),
            record("planned", "deal", json!({"stage":"lead"})),
            record("won", "deal", json!({"stage":"won"})),
        ];
        let items = agenda(&records, today);
        let ids: Vec<_> = items
            .iter()
            .map(|i| i.record["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            vec![
                "overdue",
                "today",
                "invalid",
                "next",
                "unscheduled",
                "no-action",
                "old"
            ]
        );
        assert_eq!(items[0].section, "Overdue");
        assert_eq!(
            last_contact(&records[1], &records),
            parse_date("2026-11-08")
        );
        let view = View {
            rule: Rule::OverdueTasks,
            name: "Overdue".into(),
            ..View::blank()
        };
        assert_eq!(view.select(&records, today).len(), 1);
        assert_eq!(view.select(&records, today + Duration::days(1)).len(), 2);
    }
    #[test]
    fn views_filter_nested_fields_and_sort_numbers_with_missing_values_last() {
        let today = parse_date("2026-11-09").unwrap();
        let records = vec![
            record(
                "two",
                "deal",
                json!({"value":2,"address":{"city":"London"}}),
            ),
            record(
                "ten",
                "deal",
                json!({"value":10,"address":{"city":"London"}}),
            ),
            record("missing", "deal", json!({"address":{"city":"Paris"}})),
        ];
        let mut view = View {
            name: "Amounts".into(),
            sort: "value".into(),
            descending: true,
            ..View::blank()
        };
        assert_eq!(
            view.select(&records, today)
                .iter()
                .map(|r| r["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["ten", "two", "missing"]
        );
        view.filters.push(Filter {
            field: "/data/address/city".into(),
            op: Op::Equals,
            value: "London".into(),
        });
        assert_eq!(view.select(&records, today).len(), 2);
        view.filters.push(Filter {
            field: "value".into(),
            op: Op::Equals,
            value: "2.0".into(),
        });
        assert_eq!(view.select(&records, today)[0]["id"], "two");
        view.filters.push(Filter {
            field: "updated".into(),
            op: Op::Before,
            value: "$today".into(),
        });
        view.validate().unwrap();
        assert_eq!(view.select(&records, today).len(), 1);
        view.filters.last_mut().unwrap().value = "not a date".into();
        assert!(view.validate().is_err());
    }
    #[test]
    fn saved_view_crud_persists_and_deleted_presets_are_not_resurrected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("views.duckdb");
        let id;
        {
            let s = Store::open(&path).unwrap();
            assert_eq!(s.views().unwrap().len(), 3);
            s.delete_view("Overdue tasks").unwrap();
            let view = s
                .save_view(&View {
                    name: "My deals".into(),
                    kind: "deal".into(),
                    columns: vec!["value".into()],
                    ..View::blank()
                })
                .unwrap();
            id = view.id;
            let mut view = s.find_view(&id).unwrap();
            view.name = "Renamed".into();
            s.save_view(&view).unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert!(s.find_view("Overdue tasks").is_err());
        assert_eq!(s.find_view("Renamed").unwrap().columns, ["value"]);
        s.delete_view(&id).unwrap();
        assert!(s.find_view(&id).is_err());
    }
    #[test]
    fn timeline_keeps_history_deleted_notes_related_tasks_and_transactions_atomic() {
        let s = Store::open(std::path::Path::new(":memory:")).unwrap();
        let person = s
            .execute(&["add".into(), "contact".into(), "Ada".into()])
            .unwrap();
        let id = person["id"].as_str().unwrap();
        let deal = s
            .execute(&[
                "add".into(),
                "deal".into(),
                "Proposal".into(),
                json!({"contact_id":id}).to_string(),
            ])
            .unwrap();
        let task = s
            .execute(&[
                "add".into(),
                "task".into(),
                "Follow up".into(),
                json!({"deal_id":deal["id"],"due":"2026-11-09"}).to_string(),
            ])
            .unwrap();
        let note = s
            .execute(&[
                "add".into(),
                "note".into(),
                "Call".into(),
                json!({"contact_id":id,"body":"Discussed proposal"}).to_string(),
            ])
            .unwrap();
        let noteid = note["id"].as_str().unwrap();
        s.execute(&[
            "update".into(),
            noteid.into(),
            json!({"body":"Updated summary"}).to_string(),
        ])
        .unwrap();
        s.execute(&["delete".into(), noteid.into()]).unwrap();
        let timeline = s.timeline(id).unwrap();
        assert_eq!(timeline["open_tasks"][0]["id"], task["id"]);
        let events = timeline["events"].as_array().unwrap();
        assert!(
            events
                .iter()
                .any(|e| e["action"] == "deleted" && e["name"] == "Call")
        );
        assert!(events.iter().any(|e| e["changes"] == json!(["body"])));
        let task_id = task["id"].as_str().unwrap();
        s.atomic(|| {
            s.execute(&[
                "update".into(),
                task_id.into(),
                json!({"status":"in progress"}).to_string(),
            ])?;
            s.execute(&[
                "update".into(),
                task_id.into(),
                json!({"status":"done"}).to_string(),
            ])?;
            Ok(())
        })
        .unwrap();
        let history = s.timeline(task_id).unwrap();
        assert_eq!(history["events"][0]["record"]["data"]["status"], "done");
        assert_eq!(
            history["events"][1]["record"]["data"]["status"],
            "in progress"
        );
        let before = s.all_records().unwrap().len();
        let events_before: i64 = s
            .conn
            .query_row("SELECT count(*) FROM activity_events", [], |r| r.get(0))
            .unwrap();
        let result: Result<()> = s.atomic(|| {
            s.execute(&["add".into(), "note".into(), "Rolled back".into()])?;
            bail!("Intentional rollback")
        });
        assert!(result.is_err());
        assert_eq!(s.all_records().unwrap().len(), before);
        assert_eq!(
            s.conn
                .query_row("SELECT count(*) FROM activity_events", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            events_before
        );
    }
}
