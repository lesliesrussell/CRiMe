use super::*;
#[derive(Clone, Default, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct FieldControl {
    pub required: bool,
    pub choices: Vec<String>,
    pub description: String,
    pub order: Option<usize>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct Schema {
    pub version: i64,
    pub template: Value,
    pub hints: BTreeMap<String, (Kind, String)>,
    pub controls: BTreeMap<String, FieldControl>,
}
pub(crate) fn load(store: &Store, kind: &str) -> Result<Option<Schema>> {
    load_version(store, kind, 0)
}
pub(crate) fn load_version(store: &Store, kind: &str, version: i64) -> Result<Option<Schema>> {
    store.conn.execute_batch("CREATE TABLE IF NOT EXISTS record_schemas (kind VARCHAR NOT NULL,version BIGINT NOT NULL,template VARCHAR NOT NULL,hints VARCHAR NOT NULL,controls VARCHAR DEFAULT '{}',PRIMARY KEY(kind,version)); ALTER TABLE record_schemas ADD COLUMN IF NOT EXISTS controls VARCHAR DEFAULT '{}'")?;
    let mut stmt=store.conn.prepare("SELECT version,template,hints,controls FROM record_schemas WHERE kind=? AND (?=0 OR version=?) ORDER BY version DESC LIMIT 1")?;
    let mut rows = stmt.query(params![kind, version, version])?;
    if let Some(row) = rows.next()? {
        let template: String = row.get(1)?;
        let hints: String = row.get(2)?;
        let controls: String = row.get(3)?;
        Ok(Some(Schema {
            version: row.get(0)?,
            template: serde_json::from_str(&template)?,
            hints: serde_json::from_str(&hints)?,
            controls: serde_json::from_str(&controls)?,
        }))
    } else {
        Ok(None)
    }
}
#[cfg(test)]
pub(super) fn save(
    store: &Store,
    kind: &str,
    expected: i64,
    template: &Value,
    hints: &BTreeMap<String, (Kind, String)>,
) -> Result<i64> {
    save_with_controls(store, kind, expected, template, hints, &BTreeMap::new())
}
pub(crate) fn save_with_controls(
    store: &Store,
    kind: &str,
    expected: i64,
    template: &Value,
    hints: &BTreeMap<String, (Kind, String)>,
    controls: &BTreeMap<String, FieldControl>,
) -> Result<i64> {
    store.ensure_kind(kind)?;
    let version = load(store, kind)?.map(|s| s.version).unwrap_or(0);
    if version != expected {
        bail!("Schema changed since you opened it. Cancel and reopen the schema editor.");
    }
    let root = json!({"data":template});
    let hints: BTreeMap<_, _> = hints
        .iter()
        .filter(|(path, _)| root.pointer(path).is_some())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let controls: BTreeMap<_, _> = controls
        .iter()
        .filter(|(path, _)| root.pointer(path).is_some())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (path, c) in &controls {
        if !c.choices.is_empty() && hints.get(path).is_none_or(|h| h.0 != Kind::Text) {
            bail!(
                "Dropdown choices require a Text field: {}",
                field_name(path)
            );
        }
    }
    let schema = Schema {
        version: version + 1,
        template: template.clone(),
        hints,
        controls,
    };
    schema.validate_defaults()?;
    store.conn.execute(
        "INSERT INTO record_schemas(kind,version,template,hints,controls) VALUES (?,?,?,?,?)",
        params![
            kind,
            version + 1,
            template.to_string(),
            serde_json::to_string(&schema.hints)?,
            serde_json::to_string(&schema.controls)?
        ],
    )?;
    Ok(version + 1)
}
impl Schema {
    pub(crate) fn validate(&self, data: &Value) -> Result<()> {
        self.validate_impl(data, true)
    }
    fn validate_defaults(&self) -> Result<()> {
        self.validate_impl(&self.template, false)
    }
    fn validate_impl(&self, data: &Value, required: bool) -> Result<()> {
        fn walk(
            s: &Schema,
            template: &Value,
            value: &Value,
            path: &str,
            required: bool,
            errors: &mut Vec<String>,
        ) {
            let empty = value.is_null()
                || value.as_str().is_some_and(|s| s.trim().is_empty())
                || value.as_array().is_some_and(Vec::is_empty)
                || value.as_object().is_some_and(|o| o.is_empty());
            if let Some(c) = s.controls.get(path) {
                if required && c.required && empty {
                    errors.push(format!("{} is required", field_name(path)));
                }
                if !empty
                    && !c.choices.is_empty()
                    && !value
                        .as_str()
                        .is_some_and(|v| c.choices.iter().any(|c| c == v))
                {
                    errors.push(format!(
                        "{} must be one of: {}",
                        field_name(path),
                        c.choices.join(", ")
                    ));
                }
            }
            if !value.is_null()
                && let Some((kind, _)) = s.hints.get(path)
            {
                let valid = match kind {
                    Kind::Text | Kind::Link(_) => value.is_string(),
                    Kind::Number => value.is_number(),
                    Kind::Bool => value.is_boolean(),
                    Kind::Date => value
                        .as_str()
                        .is_some_and(|v| v.is_empty() || valid_date(v)),
                    Kind::Object => value.is_object(),
                    Kind::Array => value.is_array(),
                };
                if !valid {
                    errors.push(format!("{} must be {}", field_name(path), kind.title()));
                    return;
                }
            }
            if let Some(map) = template.as_object() {
                for (key, child_value) in map {
                    walk(
                        s,
                        child_value,
                        value.get(key).unwrap_or(&Value::Null),
                        &child(path, key),
                        required,
                        errors,
                    );
                }
            }
            if let Some(example) = template.as_array().and_then(|a| a.first())
                && let Some(items) = value.as_array()
            {
                for item in items {
                    walk(s, example, item, &child(path, "0"), required, errors);
                }
            }
        }
        let mut errors = vec![];
        walk(self, &self.template, data, "/data", required, &mut errors);
        if !errors.is_empty() {
            bail!("{}", errors.join("; "));
        }
        Ok(())
    }
}
impl Store {
    pub(crate) fn validate_record(&self, kind: &str, data: &Value, version: i64) -> Result<()> {
        if version > 0
            && let Some(schema) = load_version(self, kind, version)?
        {
            schema.validate(data)?;
        }
        Ok(())
    }
}
pub(super) struct Migration {
    pub data: Value,
    pub changes: Vec<String>,
    pub conflicts: Vec<String>,
    pub schema: Schema,
}
pub(super) fn preview(data: &Value, schema: Schema) -> Migration {
    let mut m = Migration {
        data: data.clone(),
        changes: vec![],
        conflicts: vec![],
        schema,
    };
    let template = m.schema.template.clone();
    let hints = m.schema.hints.clone();
    migrate(
        &mut m.data,
        &template,
        "/data",
        "/data",
        &hints,
        &mut m.changes,
        &mut m.conflicts,
    );
    m
}
fn field_name(path: &str) -> String {
    let parts: Vec<_> = path
        .split('/')
        .skip(2)
        .map(|p| {
            let p = p.replace("~1", "/").replace("~0", "~");
            if let Ok(index) = p.parse::<usize>() {
                format!("Item {}", index + 1)
            } else {
                details::label(&p)
            }
        })
        .collect();
    if parts.is_empty() {
        "Record fields".into()
    } else {
        parts.join(" › ")
    }
}
fn migrate(
    data: &mut Value,
    template: &Value,
    path: &str,
    schema_path: &str,
    hints: &BTreeMap<String, (Kind, String)>,
    changes: &mut Vec<String>,
    conflicts: &mut Vec<String>,
) {
    let key = schema_path.rsplit('/').next().unwrap_or("");
    let kind = hints
        .get(schema_path)
        .map(|h| h.0.clone())
        .unwrap_or_else(|| infer(key, template));
    if data.is_null() {
        if !template.is_null() {
            *data = template.clone();
            changes.push(format!("Set default: {}", field_name(path)));
        }
        return;
    }
    let compatible = match &kind {
        Kind::Text => data.is_string(),
        Kind::Number => data.is_number(),
        Kind::Bool => data.is_boolean(),
        Kind::Date => data.as_str().is_some_and(valid_date),
        Kind::Object => data.is_object(),
        Kind::Array => data.is_array(),
        Kind::Link(_) => data.is_string(),
    };
    if !compatible {
        let converted = match &kind {
            Kind::Number => data
                .as_str()
                .and_then(|s| s.trim().parse::<serde_json::Number>().ok())
                .map(Value::Number),
            Kind::Bool => data
                .as_str()
                .and_then(|s| match s.trim().to_lowercase().as_str() {
                    "true" | "yes" | "1" => Some(json!(true)),
                    "false" | "no" | "0" => Some(json!(false)),
                    _ => None,
                }),
            Kind::Text => {
                if data.is_boolean() || data.is_number() {
                    Some(json!(data.to_string()))
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(v) = converted {
            *data = v;
            changes.push(format!("Convert {} to {}", field_name(path), kind.title()));
        } else {
            conflicts.push(format!(
                "{}: expected {}. Use F5 to change its type/value before migrating.",
                field_name(path),
                kind.title()
            ));
            return;
        }
    }
    if let (Some(values), Some(fields)) = (data.as_object_mut(), template.as_object()) {
        for (key, default) in fields {
            let target = child(path, key);
            let source = child(schema_path, key);
            if let Some(value) = values.get_mut(key) {
                migrate(value, default, &target, &source, hints, changes, conflicts);
            } else {
                values.insert(key.clone(), default.clone());
                changes.push(format!(
                    "Add {} (default: {})",
                    field_name(&target),
                    super::preview(default, &[])
                ));
            }
        }
        for key in values.keys().filter(|key| !fields.contains_key(*key)) {
            changes.push(format!(
                "Keep extra field {}",
                field_name(&child(path, key))
            ));
        }
    }
    if let (Some(values), Some(example)) = (
        data.as_array_mut(),
        template.as_array().and_then(|a| a.first()),
    ) {
        for (index, value) in values.iter_mut().enumerate() {
            migrate(
                value,
                example,
                &child(path, &index.to_string()),
                &child(schema_path, "0"),
                hints,
                changes,
                conflicts,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_migrations_convert_values_add_defaults_and_keep_extras() {
        let template = json!({"profile":{"score":5},"items":[{"active":false,"label":"New"}]});
        let mut hints = BTreeMap::new();
        collect_hints(&template, "/data", &mut hints);
        let result = preview(
            &json!({"profile":{"score":"12","legacy":"keep"},"items":[{"active":"yes","extra":42}]}),
            Schema {
                version: 3,
                template,
                hints,
                controls: BTreeMap::new(),
            },
        );
        assert!(result.conflicts.is_empty());
        assert_eq!(result.data["profile"]["score"], 12);
        assert_eq!(result.data["profile"]["legacy"], "keep");
        assert_eq!(result.data["items"][0]["active"], true);
        assert_eq!(result.data["items"][0]["label"], "New");
        assert_eq!(result.data["items"][0]["extra"], 42);
    }
    #[test]
    fn old_database_upgrades_without_changing_records_and_schemas_persist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.duckdb");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE records (id VARCHAR PRIMARY KEY, kind VARCHAR, name VARCHAR, data VARCHAR, created TIMESTAMP DEFAULT current_timestamp, updated TIMESTAMP DEFAULT current_timestamp); INSERT INTO records(id,kind,name,data) VALUES ('old','contact','Ada','{\"email\":\"ada@example.com\"}')").unwrap();
        }
        {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.get("old").unwrap()["schema_version"], 0);
            assert_eq!(
                save(
                    &store,
                    "contact",
                    0,
                    &json!({"custom":null}),
                    &BTreeMap::new()
                )
                .unwrap(),
                1
            );
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.get("old").unwrap()["data"]["email"],
            "ada@example.com"
        );
        let schema = load(&store, "contact").unwrap().unwrap();
        assert_eq!(schema.version, 1);
        assert!(schema.template.get("custom").is_some());
    }
}
