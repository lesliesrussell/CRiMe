use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};
fn run(db: &std::path::Path, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_crime"))
        .arg("--db")
        .arg(db)
        .arg("run")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn persistent_crm_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("crm.duckdb");
    let person = run(
        &db,
        &[
            "add",
            "contact",
            "Ada Lovelace",
            r#"{"email":"ada@example.com"}"#,
        ],
    );
    let id = person["id"].as_str().unwrap();
    assert_eq!(run(&db, &["get", id])["data"]["email"], "ada@example.com");
    assert_eq!(
        run(&db, &["update", id, r#"{"name":"Ada","phone":"123"}"#])["name"],
        "Ada"
    );
    assert_eq!(
        run(&db, &["list", "contact", "ada@example.com"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(run(&db, &["export"]).as_array().unwrap().len(), 1);
    run(&db, &["delete", id]);
    assert_eq!(run(&db, &["list"]).as_array().unwrap().len(), 0);
}
#[test]
fn mcp_stdio_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_crime"))
        .arg("--db")
        .arg(dir.path().join("mcp.duckdb"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for request in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"crm","arguments":{"args":["add","alien","Invalid"]}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"crm","arguments":{"args":["add","task","Follow up","{\"status\":\"open\"}"]}}}),
    ] {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let lines: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[1]["result"]["tools"][0]["name"], "crm");
    assert_eq!(lines[2]["result"]["isError"], true);
    assert_eq!(lines[0]["result"]["serverInfo"]["name"], "crime");
    assert_eq!(lines[3]["result"]["isError"], false);
    assert!(
        lines[3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Follow up")
    );
}

#[test]
fn direct_flags_and_invalid_input() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("crm.duckdb");
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_crime"))
            .arg("--db")
            .arg(&db)
            .args(args)
            .output()
            .unwrap()
    };
    let out = invoke(&[
        "add",
        "deal",
        "New business",
        "--data",
        r#"{"stage":"qualified","value":500}"#,
    ]);
    assert!(out.status.success());
    let out = invoke(&["list", "--kind", "deal", "--search", "New business"]);
    let records: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(records.as_array().unwrap().len(), 1);
    for args in [
        vec!["add", "alien", "Bad"],
        vec!["add", "contact", "Bad", "--data", "[]"],
        vec!["get", "missing"],
    ] {
        assert!(!invoke(&args).status.success());
    }
    assert_eq!(run(&db, &["export"]).as_array().unwrap().len(), 1);
}

#[test]
fn daily_workflows_are_shared_by_cli_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("daily.duckdb");
    let contact = run(&db, &["add", "contact", "Ada"]);
    let data = json!({"contact_id":contact["id"],"due":"2026-01-01","status":"open"}).to_string();
    let task = run(&db, &["add", "task", "Call Ada", &data]);
    let definition=json!({"name":"My open tasks","kind":"task","filters":[{"field":"status","op":"equals","value":"open"}],"sort":"due","columns":["due","contact_id"]}).to_string();
    let view = run(&db, &["view-save", &definition]);
    let cli = Command::new(env!("CARGO_BIN_EXE_crime"))
        .arg("--db")
        .arg(&db)
        .args(["today", "--date", "2026-11-09"])
        .output()
        .unwrap();
    assert!(cli.status.success());
    let today: Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert!(
        today["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["record"]["id"] == task["id"] && i["section"] == "Overdue")
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_crime"))
        .arg("--db")
        .arg(&db)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for (id, args) in [
        (1, json!(["view", view["id"]])),
        (2, json!(["timeline", contact["id"]])),
        (3, json!(["today", "2026-11-09"])),
    ] {
        writeln!(stdin,"{}",json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"crm","arguments":{"args":args}}})).unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect();
    let payloads: Vec<Value> = responses
        .iter()
        .map(|r| {
            assert_eq!(r["result"]["isError"], false);
            serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
        })
        .collect();
    assert_eq!(payloads[0]["records"][0]["id"], task["id"]);
    assert_eq!(payloads[1]["open_tasks"][0]["id"], task["id"]);
    assert!(payloads[1]["events"].as_array().unwrap().len() >= 2);
    assert_eq!(payloads[2]["date"], "2026-11-09");
}

#[test]
fn custom_types_schemas_csv_and_trash_work_through_cli_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("extended.duckdb");
    run(&db, &["type-add", "project", "Projects"]);
    let mut schema = run(&db, &["schema", "project"]);
    schema["template"] = json!({"email":null,"budget":0,"stage":null});
    schema["hints"] = json!({"/data/email":["Text","Email address"],"/data/budget":["Number","Amount"],"/data/stage":["Text","Stage"]});
    schema["controls"] = json!({"/data/stage":{"required":true,"choices":["Planned","Active"],"description":"Project state","order":0}});
    run(&db, &["schema-save", "project", &schema.to_string()]);
    let existing = run(
        &db,
        &[
            "add",
            "project",
            "Original",
            &json!({"email":"a@example.com","stage":"Planned","budget":5,"notes":"Keep"})
                .to_string(),
        ],
    );
    let csv = dir.path().join("import.csv");
    std::fs::write(
        &csv,
        "Person,Email,Cost,Stage\nUpdated,a@example.com,12,Active\nNew,b@example.com,2,Planned\n",
    )
    .unwrap();
    let invoke = |preview: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_crime"));
        command
            .arg("--db")
            .arg(&db)
            .arg("import-csv")
            .arg(&csv)
            .args([
                "--kind",
                "project",
                "--duplicates",
                "merge",
                "--match-field",
                "email",
                "--name-column",
                "Person",
                "--map",
                "Cost=budget",
            ]);
        if preview {
            command.arg("--preview");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let preview = invoke(true);
    assert_eq!(preview["merge"], 1);
    assert_eq!(preview["add"], 1);
    assert_eq!(
        run(&db, &["get", existing["id"].as_str().unwrap()])["data"]["budget"],
        5
    );
    let imported = invoke(false);
    assert_eq!(imported["merged"], 1);
    run(&db, &["delete", existing["id"].as_str().unwrap()]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_crime"))
        .arg("--db")
        .arg(&db)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for (id, args) in [
        (1, json!(["types"])),
        (2, json!(["schema", "project"])),
        (3, json!(["trash"])),
        (4, json!(["restore", existing["id"]])),
    ] {
        writeln!(stdin,"{}",json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"crm","arguments":{"args":args}}})).unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let payloads: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let response: Value = serde_json::from_str(line).unwrap();
            assert_eq!(response["result"]["isError"], false);
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap()
        })
        .collect();
    assert!(
        payloads[0]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["slug"] == "project")
    );
    assert_eq!(payloads[1]["controls"]["/data/stage"]["required"], true);
    assert_eq!(payloads[2][0]["record"]["id"], existing["id"]);
    assert_eq!(payloads[3]["data"]["budget"], 12);
    assert_eq!(payloads[3]["data"]["notes"], "Keep");
    assert_eq!(run(&db, &["export"]).as_array().unwrap().len(), 2);
}

#[test]
fn merge_bulk_and_relationships_persist_through_cli_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("actions.duckdb");
    let a = run(
        &db,
        &["add", "contact", "Ada", r#"{"email":"ada@example.com"}"#],
    );
    let b = run(
        &db,
        &["add", "contact", "Ada duplicate", r#"{"phone":"123"}"#],
    );
    let aid = a["id"].as_str().unwrap();
    let bid = b["id"].as_str().unwrap();
    let task = run(
        &db,
        &[
            "add",
            "task",
            "Call",
            &json!({"contact_id":bid}).to_string(),
        ],
    );
    let out = Command::new(env!("CARGO_BIN_EXE_crime"))
        .arg("--db")
        .arg(&db)
        .args(["merge", aid, bid, "--preview"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(run(&db, &["list", "contact"]).as_array().unwrap().len(), 2);
    let mut child = Command::new(env!("CARGO_BIN_EXE_crime"))
        .arg("--db")
        .arg(&db)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for (i, args) in [
        vec!["merge".into(), aid.into(), bid.into()],
        vec!["related".into(), aid.into()],
        vec![
            "bulk".into(),
            "update".into(),
            json!([aid]).to_string(),
            json!({"city":"London"}).to_string(),
        ],
        vec!["merges".into()],
    ]
    .into_iter()
    .enumerate()
    {
        writeln!(stdin,"{}",json!({"jsonrpc":"2.0","id":i,"method":"tools/call","params":{"name":"crm","arguments":{"args":args}}})).unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(responses.len(), 4);
    for r in &responses {
        assert_eq!(r["result"]["isError"], false, "{r}");
    }
    assert_eq!(run(&db, &["get", aid])["data"]["phone"], "123");
    assert_eq!(run(&db, &["get", aid])["data"]["city"], "London");
    assert_eq!(
        run(&db, &["get", task["id"].as_str().unwrap()])["data"]["contact_id"],
        aid
    );
    assert!(run(&db, &["merges"])[0]["source_record"]["name"] == "Ada duplicate");
}
