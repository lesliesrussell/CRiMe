use crate::*;
use std::collections::BTreeSet;

pub fn links(value: &Value, id: &str) -> bool {
    match value {
        Value::String(s) => s == id,
        Value::Array(a) => a.iter().any(|v| links(v, id)),
        Value::Object(o) => o.values().any(|v| links(v, id)),
        _ => false,
    }
}
fn redirect(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::String(s) if s == from => *s = to.into(),
        Value::Array(a) => a.iter_mut().for_each(|v| redirect(v, from, to)),
        Value::Object(o) => o.values_mut().for_each(|v| redirect(v, from, to)),
        _ => {}
    }
}
fn combine(target: &mut Value, source: &Value, path: &str, conflicts: &mut Vec<Value>) {
    if let (Some(t), Some(s)) = (target.as_object_mut(), source.as_object()) {
        for (k, v) in s {
            let p = format!("{path}/{}", k.replace('~', "~0").replace('/', "~1"));
            if let Some(old) = t.get_mut(k) {
                combine(old, v, &p, conflicts);
            } else {
                t.insert(k.clone(), v.clone());
            }
        }
    } else if target.is_null() || target.as_str() == Some("") {
        *target = source.clone();
    } else if let (Some(t), Some(s)) = (target.as_array_mut(), source.as_array()) {
        for v in s {
            if !t.contains(v) {
                t.push(v.clone());
            }
        }
    } else if target != source && !source.is_null() && source.as_str() != Some("") {
        conflicts.push(json!({"field":path,"kept":target,"archived":source}));
    }
}
impl Store {
    pub fn related_records(&self, id: &str) -> Result<Vec<Value>> {
        let current = self.get(id)?;
        Ok(self
            .all_records()?
            .into_iter()
            .filter(|r| {
                r["id"] != id
                    && (links(&current["data"], r["id"].as_str().unwrap_or(""))
                        || links(&r["data"], id))
            })
            .collect())
    }
    pub fn merge_records(&self, target: &str, source: &str, preview: bool) -> Result<Value> {
        self.atomic(|| {
            if target==source { bail!("Choose two different records"); }
            let before=self.get(target)?; let duplicate=self.get(source)?;
            if before["kind"]!=duplicate["kind"] { bail!("Merge requires records of the same type"); }
            let mut data=before["data"].clone(); let mut conflicts=vec![];
            combine(&mut data,&duplicate["data"],"/data",&mut conflicts);
            if before["name"]!=duplicate["name"] { conflicts.push(json!({"field":"name","kept":before["name"],"archived":duplicate["name"]})); }
            redirect(&mut data,source,target);
            self.validate_record(before["kind"].as_str().unwrap(),&data,before["schema_version"].as_i64().unwrap_or(0))?;
            let affected:Vec<_>=self.all_records()?.into_iter().filter(|r|r["id"]!=source && r["id"]!=target && links(&r["data"],source)).collect();
            let summary=json!({"survivor":target,"source":source,"data":data,"conflicts":conflicts,"redirected":affected.len(),"preview":preview});
            if preview { return Ok(summary); }
            self.replace_record(target,before["name"].as_str().unwrap(),&data)?;
            for r in affected { let mut d=r["data"].clone();redirect(&mut d,source,target);self.replace_record(r["id"].as_str().unwrap(),r["name"].as_str().unwrap(),&d)?; }
            // Recovery snapshots also follow the survivor when later restored.
            for row in self.trash()? { let mut r=row["record"].clone();if links(&r["data"],source) {redirect(&mut r["data"],source,target);self.conn.execute("UPDATE record_trash SET record=? WHERE id=?",params![r.to_string(),r["id"].as_str()])?;} }
            self.conn.execute("INSERT INTO record_merges(source_id,target_id,source_record) VALUES (?,?,?)",params![source,target,duplicate.to_string()])?;
            self.conn.execute("UPDATE record_merges SET target_id=? WHERE target_id=?",params![target,source])?;
            self.conn.execute("UPDATE activity_events SET record_id=? WHERE record_id=?",params![target,source])?;
            self.log_change(Some(&duplicate),Some(&self.get(target)?),"merged")?;
            self.conn.execute("DELETE FROM records WHERE id=?",params![source])?;
            Ok(summary)
        })
    }
    pub fn bulk(&self, action: &str, ids: &[String], patch: &Value) -> Result<Value> {
        if ids.is_empty() {
            bail!("Select at least one record");
        }
        let ids: BTreeSet<_> = ids.iter().collect();
        self.atomic(|| {
            let records = ids
                .iter()
                .map(|id| self.get(id))
                .collect::<Result<Vec<_>>>()?;
            if action == "export" {
                return Ok(json!(records));
            }
            if !["update", "trash"].contains(&action) {
                bail!("Bulk action must be update, trash or export");
            }
            if action == "update" {
                validate_data(patch)?;
            }
            for r in records {
                let id = r["id"].as_str().unwrap();
                if action == "trash" {
                    self.execute(&["delete".into(), id.into()])?;
                } else {
                    self.execute(&["update".into(), id.into(), patch.to_string()])?;
                }
            }
            Ok(json!({"action":action,"count":ids.len()}))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn add(s: &Store, kind: &str, name: &str, data: Value) -> Value {
        s.execute(&["add".into(), kind.into(), name.into(), data.to_string()])
            .unwrap()
    }
    #[test]
    fn merge_preserves_fields_nested_links_trash_and_activity() {
        let s = Store::open(std::path::Path::new(":memory:")).unwrap();
        let a = add(
            &s,
            "contact",
            "Ada",
            json!({"email":"kept@example.com","tags":["a"],"nested":{"phone":"123"}}),
        );
        let b = add(
            &s,
            "contact",
            "Ada duplicate",
            json!({"email":"old@example.com","tags":["b"],"nested":{"city":"London"}}),
        );
        let aid = a["id"].as_str().unwrap();
        let bid = b["id"].as_str().unwrap();
        let task = add(
            &s,
            "task",
            "Call",
            json!({"contact_id":bid,"nested":{"links":[bid]},"body":format!("Call {bid} later")}),
        );
        let note = add(&s, "note", "Old meeting", json!({"contact_id":bid}));
        s.execute(&["delete".into(), note["id"].as_str().unwrap().into()])
            .unwrap();
        let preview = s.merge_records(aid, bid, true).unwrap();
        assert_eq!(preview["redirected"], 1);
        assert_eq!(s.all_records().unwrap().len(), 3);
        s.merge_records(aid, bid, false).unwrap();
        assert!(s.get(bid).is_err());
        let merged = s.get(aid).unwrap();
        assert_eq!(merged["data"]["email"], "kept@example.com");
        assert_eq!(merged["data"]["tags"], json!(["a", "b"]));
        assert_eq!(merged["data"]["nested"]["city"], "London");
        let task = s.get(task["id"].as_str().unwrap()).unwrap();
        assert_eq!(task["data"]["nested"]["links"][0], aid);
        assert_eq!(task["data"]["body"], format!("Call {bid} later"));
        let restored = s.restore(note["id"].as_str().unwrap()).unwrap();
        assert_eq!(restored["data"]["contact_id"], aid);
        let timeline = s.timeline(aid).unwrap().to_string();
        assert!(timeline.contains("Ada duplicate"));
        assert!(timeline.contains("Old meeting"));
        let raw: String = s
            .conn
            .query_row(
                "SELECT source_record FROM record_merges WHERE source_id=?",
                params![bid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), b);
        let related = s.related_records(aid).unwrap();
        assert_eq!(related.len(), 2);
    }
    #[test]
    fn validation_after_an_earlier_write_rolls_back_bulk_and_merge() {
        let s = Store::open(std::path::Path::new(":memory:")).unwrap();
        let a = add(&s, "contact", "A", json!({}));
        let b = add(&s, "contact", "B", json!({}));
        let mut ids = vec![
            a["id"].as_str().unwrap().to_owned(),
            b["id"].as_str().unwrap().to_owned(),
        ];
        ids.sort();
        let schema = json!({"version":0,"template":{"status":"open"},"hints":{"/data/status":["Text", "Status"]},"controls":{"/data/status":{"required":true,"choices":["open"]}}});
        s.execute(&["schema-save".into(), "contact".into(), schema.to_string()])
            .unwrap();
        let last = s.get(&ids[1]).unwrap();
        s.replace_record_version(
            &ids[1],
            last["name"].as_str().unwrap(),
            &json!({"status":"open"}),
            1,
        )
        .unwrap();
        let before = s.all_records().unwrap();
        assert!(
            s.bulk("update", &ids, &json!({"status":"invalid"}))
                .is_err()
        );
        assert_eq!(s.all_records().unwrap(), before);
        let source = add(&s, "company", "Source", json!({"phone":"123"}));
        let target = add(&s, "company", "Target", json!({}));
        let sid = source["id"].as_str().unwrap();
        let tid = target["id"].as_str().unwrap();
        let schema = json!({"version":0,"template":{"company_id":sid},"hints":{"/data/company_id":["Text", "Company"]},"controls":{"/data/company_id":{"choices":[sid]}}});
        s.execute(&["schema-save".into(), "task".into(), schema.to_string()])
            .unwrap();
        add(&s, "task", "Linked", json!({}));
        let before = s.all_records().unwrap();
        assert!(s.merge_records(tid, sid, false).is_err());
        assert_eq!(s.all_records().unwrap(), before);
        assert_eq!(
            s.conn
                .query_row("SELECT COUNT(*) FROM record_merges", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn bulk_and_merge_failures_roll_back_every_change() {
        let s = Store::open(std::path::Path::new(":memory:")).unwrap();
        let a = add(&s, "contact", "A", json!({"city":"London"}));
        let b = add(&s, "contact", "B", json!({}));
        let task = add(&s, "task", "Task", json!({}));
        let aid = a["id"].as_str().unwrap().to_owned();
        let bid = b["id"].as_str().unwrap().to_owned();
        assert!(
            s.bulk("trash", &[aid.clone(), "missing".into()], &json!({}))
                .is_err()
        );
        assert!(s.trash().unwrap().is_empty());
        assert!(
            s.bulk("update", &[aid.clone(), bid.clone()], &json!({"name":""}))
                .is_err()
        );
        assert_eq!(s.get(&aid).unwrap(), a);
        assert!(
            s.merge_records(&aid, task["id"].as_str().unwrap(), false)
                .is_err()
        );
        assert!(s.merge_records(&aid, &aid, false).is_err());
        s.bulk(
            "update",
            &[aid.clone(), bid.clone()],
            &json!({"status":"active"}),
        )
        .unwrap();
        assert_eq!(s.get(&aid).unwrap()["data"]["city"], "London");
        assert_eq!(s.get(&bid).unwrap()["data"]["status"], "active");
        assert_eq!(
            s.bulk(
                "export",
                &[aid.clone(), aid.clone(), bid.clone()],
                &json!({})
            )
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
            2
        );
        s.bulk("trash", &[aid, bid], &json!({})).unwrap();
        assert_eq!(s.trash().unwrap().len(), 2);
    }
}
