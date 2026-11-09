use crate::tui::form::{Form, Kind, schema::Schema};
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
pub struct CsvFile {
    pub headers: Vec<String>,
    pub rows: Vec<(u64, Vec<String>)>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Duplicates {
    Skip,
    Merge,
    KeepBoth,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub kind: String,
    pub mapping: BTreeMap<usize, String>,
    pub duplicate_field: String,
    pub duplicates: Duplicates,
}
pub struct Prepared {
    pub preview: Value,
    actions: Vec<Action>,
    baseline: Vec<Value>,
    kind: String,
    version: i64,
}
enum Action {
    Add {
        virtual_id: String,
        name: String,
        data: Value,
    },
    Merge {
        id: String,
        name: String,
        data: Value,
    },
}
fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
fn normalized(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
pub fn read(path: &str) -> Result<CsvFile> {
    let path = if let Some(path) = path.strip_prefix("~/") {
        std::path::PathBuf::from(std::env::var("HOME").context("No home directory")?).join(path)
    } else {
        path.into()
    };
    let mut reader = csv::ReaderBuilder::new()
        .from_path(&path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    let mut headers: Vec<_> = reader
        .headers()
        .context("Cannot read CSV headers")?
        .iter()
        .map(str::to_owned)
        .collect();
    if headers.is_empty() {
        bail!("CSV needs a header row");
    }
    headers[0] = headers[0].trim_start_matches('\u{feff}').into();
    let mut rows = vec![];
    for row in reader.records() {
        let row = row.context(
            "CSV rows must have the same number of columns as the header; check quotes and commas",
        )?;
        if rows.len() >= 100000 {
            bail!("Import at most 100,000 rows at a time");
        }
        rows.push((
            row.position()
                .map(|p| p.line())
                .unwrap_or(rows.len() as u64 + 2),
            row.iter().map(str::to_owned).collect(),
        ));
    }
    Ok(CsvFile { headers, rows })
}
pub fn targets(store: &Store, kind: &str) -> Result<Vec<(String, String)>> {
    let schema = Form::schema_for(store, kind)?;
    let mut fields = vec![("Record name / title *".into(), "name".into())];
    for (path, (kind, _)) in schema.hints {
        if kind != Kind::Object && !path.split('/').skip(2).any(|s| s.parse::<usize>().is_ok()) {
            fields.push((label(&path), path));
        }
    }
    Ok(fields)
}
pub fn label(path: &str) -> String {
    if path == "name" {
        return "Record name / title".into();
    }
    path.trim_start_matches("/data/")
        .split('/')
        .map(|s| {
            s.replace("~1", "/")
                .replace("~0", "~")
                .trim_end_matches("_id")
                .replace('_', " ")
        })
        .collect::<Vec<_>>()
        .join(" › ")
}
pub fn auto_config(store: &Store, kind: &str, file: &CsvFile) -> Result<Config> {
    store.ensure_kind(kind)?;
    let fields = targets(store, kind)?;
    let mut mapping = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let name = file
        .headers
        .iter()
        .position(|h| {
            ["name", "title", "fullname", "displayname"].contains(&normalized(h).as_str())
        })
        .unwrap_or(0);
    for (i, header) in file.headers.iter().enumerate() {
        let target = if i == name {
            "name".into()
        } else {
            fields
                .iter()
                .find(|(label, path)| {
                    *path != "name"
                        && (normalized(label) == normalized(header)
                            || normalized(path.rsplit('/').next().unwrap_or(""))
                                == normalized(header))
                })
                .map(|(_, p)| p.clone())
                .unwrap_or_else(|| {
                    format!(
                        "/data/{}",
                        escape(if header.trim().is_empty() {
                            "Unnamed column"
                        } else {
                            header.trim()
                        })
                    )
                })
        };
        if seen.insert(target.clone()) {
            mapping.insert(i, target);
        }
    }
    let duplicate_field = mapping
        .values()
        .find(|p| normalized(p.rsplit('/').next().unwrap_or("")) == "email")
        .cloned()
        .unwrap_or("name".into());
    Ok(Config {
        kind: kind.into(),
        mapping,
        duplicate_field,
        duplicates: Duplicates::Skip,
    })
}
fn set(data: &mut Value, path: &str, value: Value) -> Result<()> {
    let parts: Vec<_> = path
        .strip_prefix("/data/")
        .context("Map fields under /data/ or use name")?
        .split('/')
        .map(|p| p.replace("~1", "/").replace("~0", "~"))
        .collect();
    if parts.iter().any(|s| s.is_empty()) {
        bail!("Field names cannot be empty");
    }
    let mut target = data;
    for part in &parts[..parts.len() - 1] {
        if target.get(part).is_none() {
            target[part] = json!({});
        }
        target = target
            .get_mut(part)
            .context("Cannot map a field below a scalar value")?;
        if !target.is_object() {
            bail!("Cannot map a field below a scalar value");
        }
    }
    target[parts.last().unwrap()] = value;
    Ok(())
}
fn convert(
    raw: &str,
    kind: &Kind,
    path: &str,
    schema: &Schema,
    records: &[Value],
) -> Result<Value> {
    let text = raw.trim();
    match kind {
        Kind::Text => Ok(json!(raw)),
        Kind::Date => {
            if workflow::parse_date(text).is_none() {
                bail!("{} needs a real YYYY-MM-DD date", label(path));
            }
            Ok(json!(text))
        }
        Kind::Number => Ok(Value::Number(
            text.parse::<serde_json::Number>().with_context(|| {
                format!("{} needs a number without currency symbols", label(path))
            })?,
        )),
        Kind::Bool => match text.to_lowercase().as_str() {
            "true" | "yes" | "1" => Ok(json!(true)),
            "false" | "no" | "0" => Ok(json!(false)),
            _ => bail!("{} needs yes/no, true/false or 1/0", label(path)),
        },
        Kind::Link(kind) => {
            let matches: Vec<_> = records
                .iter()
                .filter(|r| {
                    r["kind"] == *kind
                        && (r["id"] == text
                            || r["name"]
                                .as_str()
                                .is_some_and(|n| n.to_lowercase() == text.to_lowercase()))
                })
                .collect();
            if matches.len() != 1 {
                bail!(
                    "{}: expected exactly one {kind} matching '{text}', found {}. Create the linked record first, or use its ID.",
                    label(path),
                    matches.len()
                );
            }
            Ok(matches[0]["id"].clone())
        }
        Kind::Array => {
            let default = schema
                .template
                .pointer(path.trim_start_matches("/data"))
                .and_then(Value::as_array)
                .and_then(|a| a.first());
            let item_kind = schema
                .hints
                .get(&format!("{path}/0"))
                .map(|h| h.0.clone())
                .unwrap_or_else(|| match default {
                    Some(Value::Number(_)) => Kind::Number,
                    Some(Value::Bool(_)) => Kind::Bool,
                    _ => Kind::Text,
                });
            Ok(Value::Array(
                text.split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|item| convert(item, &item_kind, path, schema, records))
                    .collect::<Result<Vec<_>>>()?,
            ))
        }
        Kind::Object => bail!(
            "Map a column to a field inside {}, not to the whole group",
            label(path)
        ),
    }
}
pub fn prepare(store: &Store, file: &CsvFile, config: &Config) -> Result<Prepared> {
    store.ensure_kind(&config.kind)?;
    let schema = Form::schema_for(store, &config.kind)?;
    if config.mapping.values().filter(|p| *p == "name").count() != 1 {
        bail!("Map exactly one CSV column to Record name / title");
    }
    let mut targets = BTreeSet::new();
    for (column, target) in &config.mapping {
        if *column >= file.headers.len() {
            bail!("Mapped column does not exist");
        }
        if !targets.insert(target) {
            bail!("More than one column maps to {}", label(target));
        }
        if target != "name" && !target.starts_with("/data/") {
            bail!("Invalid mapping target {target}");
        }
    }
    let baseline = store.all_records()?;
    let mut working = baseline.clone();
    let mut actions = vec![];
    let mut errors = vec![];
    let mut rows = vec![];
    let (mut added, mut merged, mut skipped) = (0, 0, 0);
    for (line, cells) in &file.rows {
        let result = (|| -> Result<()> {
            if cells.iter().all(|s| s.trim().is_empty()) {
                skipped += 1;
                rows.push(json!({"line":line,"action":"skip empty row"}));
                return Ok(());
            }
            if config.duplicates == Duplicates::Skip {
                let canonical = |field: &str| {
                    if field == "name" {
                        "name".to_owned()
                    } else if field.starts_with("/data/") {
                        field.to_owned()
                    } else {
                        format!("/data/{}", field.replace('.', "/"))
                    }
                };
                if let Some((column, _)) = config
                    .mapping
                    .iter()
                    .find(|(_, target)| canonical(target) == canonical(&config.duplicate_field))
                {
                    let key = cells[*column].trim().to_lowercase();
                    if !key.is_empty()
                        && working.iter().any(|r| {
                            r["kind"] == config.kind
                                && workflow::friendly(
                                    &workflow::field(r, &config.duplicate_field, &working),
                                    &working,
                                )
                                .trim()
                                .to_lowercase()
                                    == key
                        })
                    {
                        skipped += 1;
                        rows.push(
                            json!({"line":line,"name":cells[*column],"action":"skip duplicate"}),
                        );
                        return Ok(());
                    }
                }
            }
            let mut name = String::new();
            let mut provided = json!({});
            for (column, target) in &config.mapping {
                let value = &cells[*column];
                if target == "name" {
                    name = value.trim().into();
                } else if !value.trim().is_empty() {
                    let kind = schema
                        .hints
                        .get(target)
                        .map(|h| h.0.clone())
                        .unwrap_or(Kind::Text);
                    set(
                        &mut provided,
                        target,
                        convert(value, &kind, target, &schema, &working)?,
                    )?;
                }
            }
            let probe = json!({"name":name,"data":provided});
            let key = workflow::field(&probe, &config.duplicate_field, &working);
            let key = if key.is_null() {
                String::new()
            } else {
                workflow::friendly(&key, &working).trim().to_lowercase()
            };
            let matches: Vec<_> = working
                .iter()
                .filter(|r| {
                    r["kind"] == config.kind
                        && !key.is_empty()
                        && workflow::friendly(
                            &workflow::field(r, &config.duplicate_field, &working),
                            &working,
                        )
                        .trim()
                        .to_lowercase()
                            == key
                })
                .cloned()
                .collect();
            if !matches.is_empty() && config.duplicates == Duplicates::Skip {
                skipped += 1;
                rows.push(json!({"line":line,"name":name,"action":"skip duplicate","matches":matches.iter().map(|r|r["name"].clone()).collect::<Vec<_>>() }));
                return Ok(());
            }
            if !matches.is_empty() && config.duplicates == Duplicates::Merge {
                if matches.len() != 1 {
                    bail!(
                        "Duplicate key matches {} records; choose another match field or Keep both",
                        matches.len()
                    );
                }
                let old = &matches[0];
                let id = old["id"]
                    .as_str()
                    .context("Duplicate has no ID")?
                    .to_owned();
                if name.is_empty() {
                    name = old["name"].as_str().unwrap_or("").into();
                }
                let mut data = old["data"].clone();
                merge_provided(&mut data, &provided);
                let version = old["schema_version"].as_i64().unwrap_or(0);
                store.validate_record(&config.kind, &data, version)?;
                if let Some(row) = working.iter_mut().find(|r| r["id"] == id) {
                    row["name"] = json!(name);
                    row["data"] = data.clone();
                }
                actions.push(Action::Merge {
                    id,
                    name: name.clone(),
                    data: data.clone(),
                });
                merged += 1;
                rows.push(
                    json!({"line":line,"name":name,"action":"merge duplicate","data":data.clone()}),
                );
            } else {
                if name.is_empty() {
                    bail!("Record name / title is required");
                }
                let mut data = provided;
                merge_defaults(&mut data, &schema.template);
                schema.validate(&data)?;
                let id = format!("csv:{}", uuid::Uuid::new_v4());
                working.push(json!({"id":id,"kind":config.kind,"name":name,"data":data,"schema_version":schema.version}));
                actions.push(Action::Add {
                    virtual_id: id,
                    name: name.clone(),
                    data: data.clone(),
                });
                added += 1;
                rows.push(json!({"line":line,"name":name,"data":data.clone(),"action":if matches.is_empty(){"add"}else{"keep both"}}));
            }
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(json!({"line":line,"error":error.to_string()}));
        }
    }
    Ok(Prepared {
        preview: json!({"rows":rows,"errors":errors,"add":added,"merge":merged,"skip":skipped,"schema_version":schema.version}),
        actions,
        baseline,
        kind: config.kind.clone(),
        version: schema.version,
    })
}
fn merge_provided(data: &mut Value, provided: &Value) {
    if let (Some(data), Some(provided)) = (data.as_object_mut(), provided.as_object()) {
        for (key, value) in provided {
            if value.is_object() && data.get(key).is_some_and(Value::is_object) {
                merge_provided(data.get_mut(key).unwrap(), value);
            } else {
                data.insert(key.clone(), value.clone());
            }
        }
    }
}
impl Prepared {
    pub fn apply(&self, store: &Store) -> Result<Value> {
        if self.preview["errors"]
            .as_array()
            .is_some_and(|e| !e.is_empty())
        {
            bail!("Fix the preview errors before importing. No rows were imported.");
        }
        store.atomic(||{
            if store.all_records()?!=self.baseline || Form::schema_for(store,&self.kind)?.version!=self.version {bail!("Records or schema changed since preview. Preview the import again.");}
            fn resolve(value:&mut Value,ids:&BTreeMap<String,String>){match value {Value::String(s)=>if let Some(id)=ids.get(s){*s=id.clone();},Value::Object(m)=>for v in m.values_mut(){resolve(v,ids);},Value::Array(a)=>for v in a{resolve(v,ids);},_=>{}}}
            let mut ids=BTreeMap::new();for action in &self.actions {match action {Action::Add{virtual_id,name,data}=>{let mut data=data.clone();resolve(&mut data,&ids);let record=store.execute(&["add".into(),self.kind.clone(),name.clone(),data.to_string()])?;ids.insert(virtual_id.clone(),record["id"].as_str().unwrap().to_owned());},Action::Merge{id,name,data}=>{let mut data=data.clone();resolve(&mut data,&ids);store.replace_record(ids.get(id).unwrap_or(id),name,&data)?;}}}
            Ok(json!({"added":self.preview["add"],"merged":self.preview["merge"],"skipped":self.preview["skip"]}))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::form::schema::{self, FieldControl};
    fn store() -> Store {
        Store::open(std::path::Path::new(":memory:")).unwrap()
    }
    fn schema(store: &Store) {
        store.add_type("project", "Projects").unwrap();
        let template = json!({"email":null,"budget":0,"active":false,"due":null,"stage":"Planned","tags":[],"company_id":null,"notes":null});
        let mut hints = BTreeMap::new();
        for (key, kind) in [
            ("email", Kind::Text),
            ("budget", Kind::Number),
            ("active", Kind::Bool),
            ("due", Kind::Date),
            ("stage", Kind::Text),
            ("tags", Kind::Array),
            ("company_id", Kind::Link("company".into())),
            ("notes", Kind::Text),
        ] {
            hints.insert(format!("/data/{key}"), (kind, String::new()));
        }
        let controls = BTreeMap::from([(
            "/data/stage".into(),
            FieldControl {
                required: true,
                choices: vec!["Planned".into(), "Active".into(), "Done".into()],
                ..FieldControl::default()
            },
        )]);
        schema::save_with_controls(store, "project", 0, &template, &hints, &controls).unwrap();
    }
    #[test]
    fn quoted_multiline_csv_maps_typed_values_and_previews_without_writing() {
        let s = store();
        schema(&s);
        let company = s
            .execute(&["add".into(), "company".into(), "Engine Works".into()])
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quoted.csv");
        std::fs::write(&path,"Name,Email,Budget,Active,Due,Stage,Tags,Company,Notes\r\n\"Launch, first\",a@example.com,12.5,yes,2028-02-29,Active,a;b,Engine Works,\"Line one\nLine two\"\r\n").unwrap();
        let file = read(path.to_str().unwrap()).unwrap();
        let config = auto_config(&s, "project", &file).unwrap();
        let prepared = prepare(&s, &file, &config).unwrap();
        assert_eq!(prepared.preview["add"], 1);
        assert_eq!(s.all_records().unwrap().len(), 1);
        assert_eq!(prepared.preview["errors"], json!([]));
        prepared.apply(&s).unwrap();
        let record = s.execute(&["list".into(), "project".into()]).unwrap()[0].clone();
        assert_eq!(record["name"], "Launch, first");
        assert_eq!(record["data"]["budget"], 12.5);
        assert_eq!(record["data"]["active"], true);
        assert_eq!(record["data"]["company_id"], company["id"]);
        assert_eq!(record["data"]["tags"], json!(["a", "b"]));
        assert_eq!(record["data"]["notes"], "Line one\nLine two");
        assert_eq!(record["schema_version"], 1);
    }
    #[test]
    fn duplicate_policies_and_atomic_validation_handle_existing_and_infile_matches() {
        let s = store();
        schema(&s);
        let existing = s
            .execute(&[
                "add".into(),
                "project".into(),
                "Original".into(),
                json!({"email":"a@example.com","budget":5,"notes":"Keep","stage":"Planned"})
                    .to_string(),
            ])
            .unwrap();
        let file = CsvFile {
            headers: vec![
                "Name".into(),
                "Email".into(),
                "Budget".into(),
                "Stage".into(),
            ],
            rows: vec![
                (
                    2,
                    vec![
                        "Revised".into(),
                        "A@example.com".into(),
                        "20".into(),
                        "Active".into(),
                    ],
                ),
                (
                    3,
                    vec![
                        "Beta".into(),
                        "b@example.com".into(),
                        "3".into(),
                        "Planned".into(),
                    ],
                ),
            ],
        };
        let mut config = auto_config(&s, "project", &file).unwrap();
        let skip = prepare(&s, &file, &config).unwrap();
        assert_eq!(skip.preview["skip"], 1);
        assert_eq!(skip.preview["add"], 1);
        config.duplicates = Duplicates::KeepBoth;
        assert_eq!(prepare(&s, &file, &config).unwrap().preview["add"], 2);
        config.duplicates = Duplicates::Merge;
        let merge = prepare(&s, &file, &config).unwrap();
        assert_eq!(merge.preview["merge"], 1);
        merge.apply(&s).unwrap();
        let updated = s.get(existing["id"].as_str().unwrap()).unwrap();
        assert_eq!(updated["name"], "Revised");
        assert_eq!(updated["data"]["notes"], "Keep");
        assert_eq!(updated["data"]["budget"], 20);
        let before = s.all_records().unwrap();
        let mut bad = file.clone();
        bad.rows[1].1[2] = "bad number".into();
        let invalid = prepare(&s, &bad, &config).unwrap();
        assert!(!invalid.preview["errors"].as_array().unwrap().is_empty());
        assert!(invalid.apply(&s).is_err());
        assert_eq!(s.all_records().unwrap(), before);
        let file = CsvFile {
            headers: file.headers,
            rows: vec![
                (
                    2,
                    vec![
                        "New".into(),
                        "new@example.com".into(),
                        "1".into(),
                        "Planned".into(),
                    ],
                ),
                (
                    3,
                    vec![
                        "New revised".into(),
                        "NEW@example.com".into(),
                        "2".into(),
                        "Active".into(),
                    ],
                ),
            ],
        };
        let import = prepare(&s, &file, &config).unwrap();
        assert_eq!(import.preview["add"], 1);
        assert_eq!(import.preview["merge"], 1);
        import.apply(&s).unwrap();
        assert!(
            s.all_records()
                .unwrap()
                .iter()
                .any(|r| r["name"] == "New revised" && r["data"]["budget"] == 2)
        );
    }
}
