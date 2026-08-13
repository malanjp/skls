//! Parse plugin-bundled MCP configs (`mcp.json` / `.mcp.json`).
//!
//! Accepts Agent Plugins 1.0 (`type` required) and the looser Copilot/Claude
//! form (infer stdio from `command`, http from `url`).

use crate::model::{McpTransport, SkillLocation};
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredMcp {
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub plugin: Option<String>,
    pub location: SkillLocation,
}

pub fn read_mcp_files(plugin_dir: &Path) -> Result<(Vec<ParsedMcpServer>, Vec<String>)> {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for name in ["mcp.json", ".mcp.json"] {
        let path = plugin_dir.join(name);
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        match parse_mcp_json(&content) {
            Ok(servers) => out.extend(servers),
            Err(err) => warnings.push(format!("skip {}: {err}", path.display())),
        }
    }
    Ok((out, warnings))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMcpServer {
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
}

pub fn parse_mcp_json(content: &str) -> Result<Vec<ParsedMcpServer>> {
    let doc: McpDocument = serde_json::from_str(content)?;
    let mut out = Vec::new();
    for (name, raw) in doc.mcp_servers {
        if let Some(server) = parse_server(&name, raw) {
            out.push(server);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn parse_server(name: &str, raw: Value) -> Option<ParsedMcpServer> {
    let obj = raw.as_object()?;
    let type_s = obj.get("type").and_then(Value::as_str);
    let command = obj
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_string);
    let url = obj.get("url").and_then(Value::as_str).map(str::to_string);
    let args = match obj.get("args") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };
    let transport = if let Some(t) = type_s.and_then(McpTransport::parse) {
        t
    } else if command.is_some() {
        McpTransport::Stdio
    } else if url.is_some() {
        McpTransport::Http
    } else {
        return None;
    };
    Some(ParsedMcpServer {
        name: name.to_string(),
        transport,
        command,
        args,
        url,
    })
}

#[derive(Debug, Deserialize)]
struct McpDocument {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: HashMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_agent_plugins_mcp_json() {
        let json = r#"{
          "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
          "mcpServers": {
            "validator": {
              "type": "stdio",
              "command": "./bin/validator",
              "args": ["--data", "${PLUGIN_DATA}/validator"]
            },
            "deployment-api": {
              "type": "streamable-http",
              "url": "https://deploy.example.com/mcp"
            }
          }
        }"#;
        let servers = parse_mcp_json(json).unwrap();
        assert_eq!(servers.len(), 2);
        let api = servers.iter().find(|s| s.name == "deployment-api").unwrap();
        assert_eq!(api.transport, McpTransport::Http);
        assert_eq!(api.url.as_deref(), Some("https://deploy.example.com/mcp"));
        let val = servers.iter().find(|s| s.name == "validator").unwrap();
        assert_eq!(val.transport, McpTransport::Stdio);
        assert_eq!(val.command.as_deref(), Some("./bin/validator"));
        assert_eq!(val.args, vec!["--data", "${PLUGIN_DATA}/validator"]);
    }

    #[test]
    fn parse_lenient_copilot_mcp_without_type() {
        let json = r#"{
          "mcpServers": {
            "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"] }
          }
        }"#;
        let servers = parse_mcp_json(json).unwrap();
        assert_eq!(servers[0].name, "github");
        assert_eq!(servers[0].transport, McpTransport::Stdio);
        assert_eq!(servers[0].command.as_deref(), Some("npx"));
    }

    #[test]
    fn parse_skips_entries_without_command_or_url() {
        let json = r#"{"mcpServers":{"broken":{"type":"unknown"}}}"#;
        let servers = parse_mcp_json(json).unwrap();
        assert!(servers.is_empty());
    }
}
