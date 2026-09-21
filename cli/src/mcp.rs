// Hand-rolled MCP (Model Context Protocol) stdio server — `cmduse mcp`.
//
// Why hand-rolled: the tools-only server surface of MCP is a small, stable
// JSON-RPC profile (initialize / ping / tools/list / tools/call over
// newline-delimited JSON on stdin/stdout), and this crate's dependency
// footprint is deliberately tiny (ureq, serde, lexopt — no async runtime).
//
// Protocol notes:
// - Requests carry an "id" and get exactly one response line; notifications
//   (no "id", e.g. `notifications/initialized`) get none.
// - stdout carries protocol traffic ONLY; diagnostics go to stderr.
// - Tool execution failures are results with `isError: true` (hosts render
//   them to the model), not JSON-RPC errors.
//
// Every tool body is the same code path the CLI subcommand uses (see the
// *_output helpers in main.rs) — no logic duplicated here.

use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2025-06-18";

pub fn run() {
    use std::io::{BufRead, Write};
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue, // not JSON: ignore (a host bug shouldn't kill the server)
        };
        if let Some(resp) = handle(&msg) {
            if writeln!(out, "{resp}").is_err() {
                break; // stdout closed: host is gone
            }
            out.flush().ok();
        }
    }
}

/// Handle one incoming message; `None` for notifications (no "id").
pub fn handle(msg: &Value) -> Option<Value> {
    let method = msg.get("method")?.as_str()?;
    let id = msg.get("id")?;
    let result = match method {
        "initialize" => Ok(json!({
            // Echo the client's version so the negotiated pair is always
            // something both sides understand; fall back to the newest spec
            // this server implements.
            "protocolVersion": msg
                .pointer("/params/protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or(PROTOCOL_VERSION),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "cmduse", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => tool_call(msg),
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(v) => json!({ "jsonrpc": "2.0", "id": id, "result": v }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": code, "message": message },
        }),
    })
}

fn tool_call(msg: &Value) -> Result<Value, (i64, String)> {
    let name = msg
        .pointer("/params/name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let args = msg
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or(json!({}));
    let get = |k: &str| args.get(k);
    let opt_usize = |k: &str| get(k).and_then(|v| v.as_u64()).map(|n| n as usize);
    let opt_bool = |k: &str| get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let opt_tz = |k: &str| -> Result<i64, (i64, String)> {
        match get(k) {
            None => Ok(0),
            Some(v) => match v.as_str().and_then(crate::cli::parse_tz) {
                Some(secs) => Ok(secs),
                None => Err((-32602, format!("{k} needs an offset like +05:30 or -08:00"))),
            },
        }
    };
    let text = match name {
        "usage" => Ok(crate::usage_output()),
        "plans" => Ok(crate::plans_output(false)),
        "models" => crate::models_output(true),
        "daily" => {
            let tz = opt_tz("tz")?;
            crate::daily_output(opt_usize("days"), tz, opt_bool("local"))
        }
        "hourly" => {
            let tz = opt_tz("tz")?;
            crate::hourly_output(opt_usize("hours"), tz, opt_bool("local"))
        }
        _ => {
            return Ok(json!({
                "content": [{ "type": "text", "text": format!("unknown tool: {name}") }],
                "isError": true,
            }))
        }
    };
    Ok(match text {
        Ok(t) => json!({ "content": [{ "type": "text", "text": t }] }),
        Err(e) => json!({
            "content": [{ "type": "text", "text": format!("error: {e}") }],
            "isError": true,
        }),
    })
}

fn tools() -> Value {
    json!([
    {
        "name": "usage",
        "description": "Command Code usage dashboard: plan, monthly credits remaining, 5-hour and weekly spend windows, billing-period totals.",
        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
    },
    {
        "name": "plans",
        "description": "Command Code plan comparison table (price, monthly credits, window caps), with the account's current plan marked.",
        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
    },
    {
        "name": "models",
        "description": "Live Command Code model list from the API, filtered to what the current plan allows.",
        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
    },
    {
        "name": "daily",
        "description": "Command Code usage by day (all harnesses). Falls back to local CLI logs when no API key is set or local=true.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "days": { "type": "integer", "description": "Days back (default 7, max 365)" },
                "tz": { "type": "string", "description": "UTC offset for day buckets, e.g. +05:30" },
                "local": { "type": "boolean", "description": "Use local CLI logs only (skip the account API)" },
            },
            "additionalProperties": false,
        },
    },
    {
        "name": "hourly",
        "description": "Command Code usage by hour (all harnesses, or local CLI sessions with local=true).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "hours": { "type": "integer", "description": "Hours back (default 24, max 168)" },
                "tz": { "type": "string", "description": "UTC offset for hour buckets, e.g. -08:00" },
                "local": { "type": "boolean", "description": "Use local CLI logs only (skip the account API)" },
            },
            "additionalProperties": false,
        },
    },
    ])
}
