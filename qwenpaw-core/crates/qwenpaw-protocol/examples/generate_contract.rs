use std::fs;
use std::path::Path;

use qwenpaw_protocol::app_protocol_fixtures;
use qwenpaw_protocol::json_schema_contract;
use qwenpaw_protocol::protocol_inventory;
use qwenpaw_protocol::typescript_contract;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    write(
        &root.join("sdk/typescript/src/protocol.ts"),
        &typescript_contract(),
    )?;
    write_json(
        &root.join("docs/api-contract/app-protocol-v2.schema.json"),
        &json_schema_contract(),
    )?;
    write_json(
        &root.join("docs/api-contract/fixtures/app-protocol-v2.json"),
        &app_protocol_fixtures(),
    )?;
    write(
        &root.join("docs/api-contract/app-protocol-inventory.md"),
        &protocol_inventory(),
    )?;
    Ok(())
}

fn write(path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let mut contents = serde_json::to_string_pretty(value)?;
    contents.push('\n');
    write(path, &contents)
}
