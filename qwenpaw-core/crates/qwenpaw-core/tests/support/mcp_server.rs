use std::io::BufRead;
use std::io::Write;

use serde_json::Value;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        let Some(id) = request.get("id") else {
            continue;
        };
        let result = match request.get("method").and_then(Value::as_str) {
            Some("initialize") => json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "qwenpaw-core-test-mcp", "version": "0.1.0"}
            }),
            Some("tools/list") => json!({
                "tools": [{
                    "name": "echo",
                    "description": "Echo a text value",
                    "inputSchema": {
                        "type": "object",
                        "properties": {"text": {"type": "string"}},
                        "required": ["text"]
                    }
                }]
            }),
            Some("tools/call") => {
                let text = request["params"]["arguments"]["text"]
                    .as_str()
                    .unwrap_or_default();
                if text == "slow" {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                }
                json!({
                    "content": [{"type": "text", "text": format!("echo: {text}")}],
                    "structuredContent": {"echo": text},
                    "isError": false
                })
            }
            _ => continue,
        };
        writeln!(
            stdout,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "result": result})
        )?;
        stdout.flush()?;
    }
    Ok(())
}
