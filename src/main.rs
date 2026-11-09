use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use duckdb::{Connection, params};
use serde_json::{Value, json};
use std::path::PathBuf;
mod actions;
mod catalog;
mod importer;
mod mcp;
mod tui;
mod workflow;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[arg(long, env = "CRIME_DB", global = true)]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Create a record
    Add {
        kind: String,
        name: String,
        #[arg(long, default_value = "{}")]
        data: String,
    },
    /// Search records by kind and text
    List {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        search: Option<String>,
    },
    /// Show one record
    Get { id: String },
    /// Merge JSON fields into a record
    Update {
        id: String,
        #[arg(long)]
        data: String,
    },
    /// Move a record to trash
    Delete { id: String },
    /// Preview or merge a duplicate into a surviving record
    Merge {
        target: String,
        source: String,
        #[arg(long)]
        preview: bool,
    },
    /// Show directly linked records in either direction
    Related { id: String },
    /// Apply update/trash/export atomically to a JSON array of record IDs
    Bulk {
        action: String,
        ids: String,
        #[arg(long, default_value = "{}")]
        data: String,
    },
    /// Show deleted records available for recovery
    Trash,
    /// Restore the most recently deleted record
    Undo,
    /// Restore a record from trash
    Restore { id: String },
    /// List builtin and custom record types
    Types,
    /// Create a custom record type
    TypeAdd { slug: String, label: String },
    /// Show the current schema for a record type
    Schema { kind: String },
    /// Import a CSV using automatic or explicit column mappings
    ImportCsv {
        path: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        preview: bool,
        #[arg(long, value_enum, default_value = "skip")]
        duplicates: importer::Duplicates,
        #[arg(long)]
        match_field: Option<String>,
        #[arg(long)]
        name_column: Option<String>,
        #[arg(long = "map")]
        mappings: Vec<String>,
    },
    /// Show overdue tasks, next actions and follow-ups
    Today {
        #[arg(long)]
        date: Option<String>,
    },
    /// Show unified activity history for a record
    Timeline { id: String },
    /// List saved views
    Views,
    /// Run a saved view by ID or name
    View { id: String },
    /// Export all records as JSON
    Export,
    /// Open the interactive terminal
    Tui {
        #[arg(long, env = "CRIME_THEME")]
        theme: Option<tui::theme::Theme>,
    },
    /// Serve MCP over stdin/stdout
    Mcp,
    /// Run a CRM command (use --help for syntax)
    Run {
        #[arg(trailing_var_arg = true, required = true)]
        args: Vec<String>,
    },
    /// Print an MCP server definition with an absolute executable and database path
    McpConfig,
}
pub struct Store {
    conn: Connection,
    in_transaction: std::cell::Cell<bool>,
}
impl Store {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)
            .context("Cannot open CRM database (another process may hold its lock)")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS records (id VARCHAR PRIMARY KEY, kind VARCHAR NOT NULL, name VARCHAR NOT NULL, data VARCHAR NOT NULL, created TIMESTAMP DEFAULT current_timestamp, updated TIMESTAMP DEFAULT current_timestamp);")?;
        conn.execute_batch(
            "ALTER TABLE records ADD COLUMN IF NOT EXISTS schema_version BIGINT DEFAULT 0",
        )?;
        let store = Self {
            conn,
            in_transaction: std::cell::Cell::new(false),
        };
        catalog::initialize(&store)?;
        workflow::initialize(&store)?;
        store.conn.execute_batch("CREATE TABLE IF NOT EXISTS record_merges(source_id VARCHAR PRIMARY KEY,target_id VARCHAR NOT NULL,source_record VARCHAR NOT NULL)")?;
        Ok(store)
    }
    pub fn execute(&self, args: &[String]) -> Result<Value> {
        let arg = |i: usize| {
            args.get(i)
                .map(String::as_str)
                .context("Missing argument; run help")
        };
        let command = arg(0)?;
        let bounds = match command {
            "related" => Some((2, 2)),
            "merge" | "merge-preview" => Some((3, 3)),
            "bulk" => Some((3, 4)),
            "help" | "export" => Some((1, 1)),
            "get" | "delete" => Some((2, 2)),
            "update" => Some((3, 3)),
            "add" => Some((3, 4)),
            "list" => Some((1, 3)),
            "today" => Some((1, 2)),
            "timeline" | "view" | "view-save" | "view-delete" => Some((2, 2)),
            "merges" | "views" | "types" | "trash" | "undo" => Some((1, 1)),
            "restore" | "trash-purge" => Some((2, 2)),
            "type-add" | "schema-save" => Some((3, 3)),
            "schema" => Some((2, 2)),
            "import-preview" | "import-csv" => Some((4, 4)),
            _ => None,
        };
        if let Some((min, max)) = bounds
            && (args.len() < min || args.len() > max)
        {
            bail!("Incorrect arguments for {command}; run help");
        }
        match command {
            "merges" => {
                let mut stmt=self.conn.prepare("SELECT source_id,target_id,source_record FROM record_merges ORDER BY source_id")?;
                let rows = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?;
                let mut out = vec![];
                for row in rows {
                    let (source, target, raw) = row?;
                    out.push(json!({"source_id":source,"target_id":target,"source_record":serde_json::from_str::<Value>(&raw)?}));
                }
                Ok(json!(out))
            }
            "related" => Ok(json!(self.related_records(arg(1)?)?)),
            "merge" | "merge-preview" => {
                self.merge_records(arg(1)?, arg(2)?, command == "merge-preview")
            }
            "bulk" => self.bulk(
                arg(1)?,
                &serde_json::from_str::<Vec<String>>(arg(2)?)?,
                &serde_json::from_str::<Value>(args.get(3).map(String::as_str).unwrap_or("{}"))?,
            ),
            "help" => Ok(
                json!({"commands":["merges", "related ID", "merge-preview SURVIVOR_ID SOURCE_ID", "merge SURVIVOR_ID SOURCE_ID", "bulk update|trash|export IDS_JSON [PATCH_JSON]","list [contact|company|deal|task|note] [search]", "add KIND NAME [JSON]", "get ID", "update ID JSON", "delete ID", "export", "today [YYYY-MM-DD]", "timeline ID", "views", "view ID_OR_NAME", "view-save JSON", "view-delete ID_OR_NAME", "types", "type-add TYPE_ID LABEL", "schema KIND", "schema-save KIND JSON", "trash", "undo", "restore ID", "trash-purge ID", "import-preview PATH KIND CONFIG_JSON", "import-csv PATH KIND CONFIG_JSON", "help"],"record_types":self.types()?, "fields":{"contact":"email, phone, company_id, tags", "company":"website, industry", "deal":"contact_id, company_id, stage, value, currency", "task":"contact_id, due (YYYY-MM-DD), status", "note":"contact_id, body"}, "example":"add contact 'Ada Lovelace' '{\"email\":\"ada@example.com\"}'"}),
            ),
            "add" => {
                let kind = arg(1)?;
                self.ensure_kind(kind)?;
                let name = arg(2)?.trim();
                if name.is_empty() {
                    bail!("Name cannot be empty");
                }
                let data: Value =
                    serde_json::from_str(args.get(3).map(String::as_str).unwrap_or("{}"))?;
                validate_data(&data)?;
                let mut data = data;
                let schema = tui::form::schema::load(self, kind)?;
                let version = schema.as_ref().map(|s| s.version).unwrap_or(0);
                if let Some(schema) = schema {
                    merge_defaults(&mut data, &schema.template);
                    schema.validate(&data)?;
                }
                let id = uuid::Uuid::new_v4().to_string();
                self.atomic(|| {
                    self.conn.execute(
                        "INSERT INTO records(id,kind,name,data,schema_version) VALUES (?,?,?,?,?)",
                        params![id, kind, name, data.to_string(), version],
                    )?;
                    let record = self.get(&id)?;
                    self.log_change(None, Some(&record), "created")?;
                    Ok(record)
                })
            }
            "get" => self.get(arg(1)?),
            "schema" => {
                self.ensure_kind(arg(1)?)?;
                Ok(serde_json::to_value(tui::form::Form::schema_for(
                    self,
                    arg(1)?,
                )?)?)
            }
            "schema-save" => {
                let schema: tui::form::schema::Schema = serde_json::from_str(arg(2)?)?;
                let version = tui::form::schema::save_with_controls(
                    self,
                    arg(1)?,
                    schema.version,
                    &schema.template,
                    &schema.hints,
                    &schema.controls,
                )?;
                Ok(json!({"kind":arg(1)?,"version":version}))
            }
            "import-preview" | "import-csv" => {
                let file = importer::read(arg(1)?)?;
                let config: importer::Config = serde_json::from_str(arg(3)?)?;
                if config.kind != arg(2)? {
                    bail!("Import kind and mapping kind differ");
                }
                let prepared = importer::prepare(self, &file, &config)?;
                if command == "import-preview" {
                    Ok(prepared.preview)
                } else {
                    prepared.apply(self)
                }
            }
            "types" => Ok(serde_json::to_value(self.types()?)?),
            "type-add" => Ok(serde_json::to_value(self.add_type(arg(1)?, arg(2)?)?)?),
            "trash" => Ok(json!(self.trash()?)),
            "restore" => self.restore(arg(1)?),
            "undo" => self.undo_delete(),
            "trash-purge" => {
                self.purge_trash(arg(1)?)?;
                Ok(json!({"purged":arg(1)?}))
            }
            "today" => {
                let date = if let Some(date) = args.get(1) {
                    workflow::parse_date(date).context("Date must be YYYY-MM-DD")?
                } else {
                    workflow::date()
                };
                Ok(
                    json!({"date":date.to_string(),"items":workflow::agenda(&self.all_records()?,date)}),
                )
            }
            "timeline" => self.timeline(arg(1)?),
            "views" => Ok(serde_json::to_value(self.views()?)?),
            "view" => {
                let view = self.find_view(arg(1)?)?;
                let records = self.all_records()?;
                Ok(json!({"view":view,"records":view.select(&records,workflow::date())}))
            }
            "view-save" => Ok(serde_json::to_value(
                self.save_view(&serde_json::from_str(arg(1)?)?)?,
            )?),
            "view-delete" => {
                self.delete_view(arg(1)?)?;
                Ok(json!({"deleted_view":arg(1)?}))
            }
            "update" => {
                let id = arg(1)?;
                let mut record = self.get(id)?;
                let patch: Value = serde_json::from_str(arg(2)?)?;
                validate_data(&patch)?;
                let mut name = record["name"].as_str().unwrap().to_string();
                for (key, value) in patch.as_object().unwrap() {
                    if key == "name" {
                        name = value
                            .as_str()
                            .context("name must be a string")?
                            .trim()
                            .to_string();
                    } else {
                        record["data"][key] = value.clone();
                    }
                }
                if name.is_empty() {
                    bail!("Name cannot be empty");
                }
                self.replace_record(id, &name, &record["data"])
            }
            "delete" => {
                let id = arg(1)?;
                self.atomic(|| {
                    let record = self.get(id)?;
                    self.conn.execute(
                        "INSERT INTO record_trash(id,record) VALUES (?,?)",
                        params![id, record.to_string()],
                    )?;
                    self.conn
                        .execute("DELETE FROM records WHERE id=?", params![id])?;
                    self.log_change(Some(&record), None, "deleted")?;
                    Ok(json!({"deleted":id}))
                })
            }
            "list" | "export" => {
                let kind = args.get(1).map(String::as_str).unwrap_or("");
                if !kind.is_empty() {
                    self.ensure_kind(kind)?;
                }
                let search = args.get(2).map(String::as_str).unwrap_or("").to_lowercase();
                let mut stmt=self.conn.prepare("SELECT id,kind,name,data,CAST(created AS VARCHAR),CAST(updated AS VARCHAR),schema_version FROM records WHERE (?='' OR kind=?) ORDER BY created,id")?;
                let rows = stmt.query_map(params![kind, kind], row_json)?;
                let mut records = Vec::new();
                for row in rows {
                    let r = row?;
                    if r.to_string().to_lowercase().contains(&search) {
                        records.push(r);
                    }
                }
                Ok(json!(records))
            }
            other => bail!("Unknown command {other}; run help"),
        }
    }
    pub fn replace_record(&self, id: &str, name: &str, data: &Value) -> Result<Value> {
        if name.trim().is_empty() {
            bail!("Name / title is required");
        }
        validate_data(data)?;
        let version = self.get(id)?["schema_version"].as_i64().unwrap_or(0);
        self.replace_record_version(id, name, data, version)
    }
    pub(crate) fn replace_record_version(
        &self,
        id: &str,
        name: &str,
        data: &Value,
        version: i64,
    ) -> Result<Value> {
        if name.trim().is_empty() {
            bail!("Name / title is required");
        }
        validate_data(data)?;
        self.atomic(|| {
            let before = self.get(id)?;
            self.validate_record(before["kind"].as_str().unwrap_or(""),data,version)?;
            if self.conn.execute(
                "UPDATE records SET name=?,data=?,schema_version=?,updated=current_timestamp WHERE id=?",
                params![name.trim(), data.to_string(),version,id],
            )? == 0
            {
                bail!("Record no longer exists");
            }
            let record = self.get(id)?;
            if before["name"] != record["name"] || before["data"] != record["data"] {
                self.log_change(Some(&before), Some(&record), "updated")?;
            }
            Ok(record)
        })
    }

    fn get(&self, id: &str) -> Result<Value> {
        self.conn.query_row("SELECT id,kind,name,data,CAST(created AS VARCHAR),CAST(updated AS VARCHAR),schema_version FROM records WHERE id=?",params![id],row_json).context("Record not found")
    }
}
fn row_json(row: &duckdb::Row<'_>) -> duckdb::Result<Value> {
    let data: String = row.get(3)?;
    Ok(
        json!({"id":row.get::<_,String>(0)?,"kind":row.get::<_,String>(1)?,"name":row.get::<_,String>(2)?,"data":serde_json::from_str::<Value>(&data).unwrap_or(Value::Null),"created":row.get::<_,String>(4)?,"updated":row.get::<_,String>(5)?,"schema_version":row.get::<_,i64>(6)?}),
    )
}
fn validate_kind(kind: &str) -> Result<()> {
    if kind.is_empty()
        || kind.len() > 64
        || !kind.starts_with(|c: char| c.is_ascii_lowercase())
        || !kind
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        bail!(
            "Type ID must start with a lowercase letter and use lowercase letters, digits, underscores or hyphens (maximum 64 characters)"
        );
    }
    Ok(())
}

fn merge_defaults(data: &mut Value, defaults: &Value) {
    if let (Some(data), Some(defaults)) = (data.as_object_mut(), defaults.as_object()) {
        for (key, value) in defaults {
            if let Some(existing) = data.get_mut(key) {
                merge_defaults(existing, value);
            } else {
                data.insert(key.clone(), value.clone());
            }
        }
    }
}
fn validate_data(data: &Value) -> Result<()> {
    if !data.is_object() {
        bail!("Data must be a JSON object");
    }
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let path = cli.db.unwrap_or_else(|| {
        directories::ProjectDirs::from("", "", "crime")
            .expect("No home directory; set --db")
            .data_dir()
            .join("crm.duckdb")
    });
    if matches!(cli.command, Some(Command::McpConfig)) {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"mcpServers":{"crime":{"command":std::env::current_exe()?,"args":["--db",std::path::absolute(path)?,"mcp"]}}})
            )?
        );
        return Ok(());
    }
    let store = Store::open(&path)?;
    match cli.command {
        Some(Command::Mcp) => mcp::serve(store).await?,
        Some(Command::Run { args }) => {
            println!("{}", serde_json::to_string_pretty(&store.execute(&args)?)?)
        }
        Some(Command::Add { kind, name, data }) => {
            print_result(&store, vec!["add".into(), kind, name, data])?
        }
        Some(Command::List { kind, search }) => print_result(
            &store,
            vec![
                "list".into(),
                kind.unwrap_or_default(),
                search.unwrap_or_default(),
            ],
        )?,
        Some(Command::Get { id }) => print_result(&store, vec!["get".into(), id])?,
        Some(Command::Update { id, data }) => {
            print_result(&store, vec!["update".into(), id, data])?
        }
        Some(Command::Delete { id }) => print_result(&store, vec!["delete".into(), id])?,
        Some(Command::Related { id }) => print_result(&store, vec!["related".into(), id])?,
        Some(Command::Merge {
            target,
            source,
            preview,
        }) => print_result(
            &store,
            vec![
                if preview { "merge-preview" } else { "merge" }.into(),
                target,
                source,
            ],
        )?,
        Some(Command::Bulk { action, ids, data }) => {
            print_result(&store, vec!["bulk".into(), action, ids, data])?
        }
        Some(Command::Trash) => print_result(&store, vec!["trash".into()])?,
        Some(Command::Undo) => print_result(&store, vec!["undo".into()])?,
        Some(Command::Restore { id }) => print_result(&store, vec!["restore".into(), id])?,
        Some(Command::Types) => print_result(&store, vec!["types".into()])?,
        Some(Command::TypeAdd { slug, label }) => {
            print_result(&store, vec!["type-add".into(), slug, label])?
        }
        Some(Command::Schema { kind }) => print_result(&store, vec!["schema".into(), kind])?,
        Some(Command::ImportCsv {
            path,
            kind,
            preview,
            duplicates,
            match_field,
            name_column,
            mappings,
        }) => {
            let file = importer::read(&path)?;
            let mut config = importer::auto_config(&store, &kind, &file)?;
            config.duplicates = duplicates;
            if let Some(field) = match_field {
                config.duplicate_field = field;
            }
            if let Some(name) = name_column {
                let column = file
                    .headers
                    .iter()
                    .position(|h| *h == name)
                    .context("Name column not found")?;
                config.mapping.retain(|_, p| p != "name");
                config.mapping.insert(column, "name".into());
            }
            for mapping in mappings {
                let (header, target) = mapping
                    .split_once('=')
                    .context("Use --map 'CSV header=field' (or header=- to ignore)")?;
                let column = file
                    .headers
                    .iter()
                    .position(|h| h == header)
                    .context("Mapped CSV header not found")?;
                config.mapping.remove(&column);
                if target != "-" && !target.is_empty() {
                    let target = if target == "name" || target.starts_with("/data/") {
                        target.into()
                    } else {
                        format!("/data/{}", target.replace('~', "~0").replace('/', "~1"))
                    };
                    config.mapping.retain(|_, p| *p != target);
                    config.mapping.insert(column, target);
                }
            }
            let prepared = importer::prepare(&store, &file, &config)?;
            let result = if preview {
                prepared.preview
            } else {
                prepared.apply(&store)?
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Some(Command::Today { date }) => {
            let mut args = vec!["today".into()];
            args.extend(date);
            print_result(&store, args)?;
        }
        Some(Command::Timeline { id }) => print_result(&store, vec!["timeline".into(), id])?,
        Some(Command::Views) => print_result(&store, vec!["views".into()])?,
        Some(Command::View { id }) => print_result(&store, vec!["view".into(), id])?,
        Some(Command::Export) => print_result(&store, vec!["export".into()])?,
        Some(Command::Tui { theme }) => tui::run(&store, theme)?,
        None => tui::run(&store, None)?,
        Some(Command::McpConfig) => unreachable!(),
    }
    Ok(())
}

fn print_result(store: &Store, args: Vec<String>) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&store.execute(&args)?)?);
    Ok(())
}
