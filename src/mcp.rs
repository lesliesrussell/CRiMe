use crate::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
pub async fn serve(store: Store) -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                out.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32700,\"message\":\"Parse error\"}}\n").await?;
                out.flush().await?;
                continue;
            }
        };
        if !request.is_object() || request["jsonrpc"] != "2.0" || !request["method"].is_string() {
            let response = json!({"jsonrpc":"2.0","id":request.get("id").unwrap_or(&Value::Null),"error":{"code":-32600,"message":"Invalid Request"}});
            out.write_all(format!("{response}\n").as_bytes()).await?;
            out.flush().await?;
            continue;
        }
        let Some(id) = request.get("id") else {
            continue;
        };
        let result = match request["method"].as_str().unwrap_or("") {
            "initialize" => Ok(
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"crime","version":env!("CARGO_PKG_VERSION")},"instructions":"Use crm with an args array. Call with [help] for commands and fields."}),
            ),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(
                json!({"tools":[{"name":"crm","description":"Manage CRM records, duplicate merges, bulk update/trash/export, relationship navigation, custom types, schemas, CSV imports, trash recovery, Today, saved views and timelines. Call args=[help] for syntax. Delete moves records to trash; undo or restore recovers them.","inputSchema":{"type":"object","properties":{"args":{"type":"array","items":{"type":"string"},"minItems":1}},"required":["args"],"additionalProperties":false}}]}),
            ),
            "tools/call" => {
                let result = (|| -> Result<Value> {
                    if request["params"]["name"] != "crm" {
                        bail!("Unknown tool");
                    }
                    let args = request["params"]["arguments"]["args"]
                        .as_array()
                        .context("args must be an array")?
                        .iter()
                        .map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .context("args must contain strings")
                        })
                        .collect::<Result<Vec<_>>>()?;
                    store.execute(&args)
                })();
                Ok(match result {
                    Ok(v) => {
                        json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&v)?}],"isError":false})
                    }
                    Err(e) => {
                        json!({"content":[{"type":"text","text":e.to_string()}],"isError":true})
                    }
                })
            }
            _ => Err(json!({"code":-32601,"message":"Method not found"})),
        };
        let response = match result {
            Ok(v) => json!({"jsonrpc":"2.0","id":id,"result":v}),
            Err(e) => json!({"jsonrpc":"2.0","id":id,"error":e}),
        };
        out.write_all(format!("{response}\n").as_bytes()).await?;
        out.flush().await?;
    }
    Ok(())
}
