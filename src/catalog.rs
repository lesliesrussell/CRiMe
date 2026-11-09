use crate::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordType {
    pub slug: String,
    pub label: String,
    pub builtin: bool,
}
pub fn builtins() -> Vec<RecordType> {
    [
        ("contact", "Contacts"),
        ("company", "Companies"),
        ("deal", "Deals"),
        ("task", "Tasks"),
        ("note", "Notes"),
    ]
    .into_iter()
    .map(|(slug, label)| RecordType {
        slug: slug.into(),
        label: label.into(),
        builtin: true,
    })
    .collect()
}
pub fn initialize(store: &Store) -> Result<()> {
    store.conn.execute_batch("CREATE TABLE IF NOT EXISTS record_types(slug VARCHAR PRIMARY KEY,label VARCHAR NOT NULL,builtin BOOLEAN NOT NULL,position INTEGER NOT NULL); CREATE SEQUENCE IF NOT EXISTS trash_order START 1; CREATE TABLE IF NOT EXISTS record_trash(id VARCHAR PRIMARY KEY,record VARCHAR NOT NULL,deleted_at TIMESTAMP DEFAULT current_timestamp,deletion_order BIGINT DEFAULT nextval('trash_order'))")?;
    for (i, kind) in builtins().iter().enumerate() {
        store.conn.execute(
            "INSERT INTO record_types VALUES (?,?,true,?) ON CONFLICT DO NOTHING",
            params![kind.slug, kind.label, i as i64],
        )?;
    }
    Ok(())
}
impl Store {
    pub fn types(&self) -> Result<Vec<RecordType>> {
        let mut stmt = self
            .conn
            .prepare("SELECT slug,label,builtin FROM record_types ORDER BY position,slug")?;
        Ok(stmt
            .query_map([], |r| {
                Ok(RecordType {
                    slug: r.get(0)?,
                    label: r.get(1)?,
                    builtin: r.get(2)?,
                })
            })?
            .collect::<duckdb::Result<Vec<_>>>()?)
    }
    pub fn ensure_kind(&self, kind: &str) -> Result<()> {
        validate_kind(kind)?;
        if !self.types()?.iter().any(|t| t.slug == kind) {
            bail!(
                "Unknown record type {kind}. Create it with type-add or the TUI command palette."
            );
        }
        Ok(())
    }
    pub fn add_type(&self, slug: &str, label: &str) -> Result<RecordType> {
        validate_kind(slug)?;
        let label = label.trim();
        if label.is_empty() {
            bail!("Give the category a display name");
        }
        if self
            .types()?
            .iter()
            .any(|t| t.label.eq_ignore_ascii_case(label))
        {
            bail!("A category with that display name already exists");
        }
        if self.types()?.iter().any(|t| t.slug == slug) {
            bail!("A record type named {slug} already exists");
        }
        self.conn.execute("INSERT INTO record_types VALUES (?,?,false,(SELECT COALESCE(MAX(position),0)+1 FROM record_types))",params![slug,label])?;
        Ok(RecordType {
            slug: slug.into(),
            label: label.into(),
            builtin: false,
        })
    }
    pub fn trash(&self) -> Result<Vec<Value>> {
        let mut stmt=self.conn.prepare("SELECT record,CAST(deleted_at AS VARCHAR) FROM record_trash ORDER BY deletion_order DESC")?;
        let mut rows = stmt.query([])?;
        let mut values = vec![];
        while let Some(row) = rows.next()? {
            let raw: String = row.get(0)?;
            values.push(json!({"record":serde_json::from_str::<Value>(&raw)?,"deleted_at":row.get::<_,String>(1)?}));
        }
        Ok(values)
    }
    pub fn restore(&self, id: &str) -> Result<Value> {
        self.atomic(||{
        let raw:String=self.conn.query_row("SELECT record FROM record_trash WHERE id=?",params![id],|r|r.get(0)).context("Record not found in trash")?;let record:Value=serde_json::from_str(&raw)?;
        if self.get(id).is_ok(){bail!("An active record already uses this ID; nothing was overwritten");}
        self.ensure_kind(record["kind"].as_str().context("Trash record has no type")?)?;
        self.conn.execute("INSERT INTO records(id,kind,name,data,created,updated,schema_version) VALUES (?,?,?,?,CAST(? AS TIMESTAMP),CAST(? AS TIMESTAMP),?)",params![id,record["kind"].as_str(),record["name"].as_str(),record["data"].to_string(),record["created"].as_str(),record["updated"].as_str(),record["schema_version"].as_i64().unwrap_or(0)])?;
        self.conn.execute("DELETE FROM record_trash WHERE id=?",params![id])?;let restored=self.get(id)?;self.log_change(None,Some(&restored),"restored")?;Ok(restored)
    })
    }
    pub fn undo_delete(&self) -> Result<Value> {
        let id: String = self
            .conn
            .query_row(
                "SELECT id FROM record_trash ORDER BY deletion_order DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .context("Trash is empty; nothing to undo")?;
        self.restore(&id)
    }
    pub fn purge_trash(&self, id: &str) -> Result<()> {
        if self
            .conn
            .execute("DELETE FROM record_trash WHERE id=?", params![id])?
            == 0
        {
            bail!("Record not found in trash");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trash_undo_preserves_ids_links_versions_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trash.duckdb");
        let first;
        let second;
        {
            let s = Store::open(&path).unwrap();
            first = s
                .execute(&["add".into(), "contact".into(), "Ada".into()])
                .unwrap();
            second = s
                .execute(&[
                    "add".into(),
                    "task".into(),
                    "Call Ada".into(),
                    json!({"contact_id":first["id"]}).to_string(),
                ])
                .unwrap();
            for r in [&first, &second] {
                s.execute(&["delete".into(), r["id"].as_str().unwrap().into()])
                    .unwrap();
            }
            assert_eq!(s.trash().unwrap().len(), 2);
            assert_eq!(s.undo_delete().unwrap(), second);
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.restore(first["id"].as_str().unwrap()).unwrap(), first);
        assert_eq!(
            s.get(second["id"].as_str().unwrap()).unwrap()["data"]["contact_id"],
            first["id"]
        );
        assert!(s.trash().unwrap().is_empty());
        s.execute(&["delete".into(), first["id"].as_str().unwrap().into()])
            .unwrap();
        s.purge_trash(first["id"].as_str().unwrap()).unwrap();
        assert!(s.undo_delete().is_err());
    }
    #[test]
    fn custom_types_persist_and_are_usable_in_records_and_views() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("types.duckdb");
        {
            let s = Store::open(&path).unwrap();
            s.add_type("project", "Projects").unwrap();
            assert!(s.add_type("project", "Another").is_err());
            assert!(s.add_type("bad type", "Bad").is_err());
            s.execute(&["add".into(), "project".into(), "Launch".into()])
                .unwrap();
            s.save_view(&workflow::View {
                name: "Projects".into(),
                kind: "project".into(),
                ..workflow::View::blank()
            })
            .unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.types().unwrap().last().unwrap().slug, "project");
        assert_eq!(
            s.execute(&["list".into(), "project".into()])
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(s.find_view("Projects").unwrap().kind, "project");
    }
}
