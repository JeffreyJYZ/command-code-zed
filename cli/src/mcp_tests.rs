// Protocol-level tests for the MCP server: pure dispatch over `handle()`.
// Tool bodies (usage/plans/models/daily/hourly) are the shared CLI helpers
// and are covered by the CLI tests; here we pin the JSON-RPC contract.

use crate::mcp::handle;
use serde_json::{json, Value};

fn call(method: &str, params: Value, id: i64) -> Value {
    handle(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
        .expect("request must get a response")
}

#[test]
fn notification_gets_no_response() {
    // no "id" → notification → None (e.g. notifications/initialized)
    let msg = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(handle(&msg).is_none());
}

#[test]
fn non_jsonrpc_message_gets_no_response() {
    assert!(handle(&json!({ "foo": 1 })).is_none());
}

#[test]
fn initialize_echoes_client_protocol_version() {
    let r = call(
        "initialize",
        json!({ "protocolVersion": "2025-03-26", "clientInfo": { "name": "zed" } }),
        1,
    );
    assert_eq!(r["id"], 1);
    assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(r["result"]["serverInfo"]["name"], "cmduse");
    assert_eq!(r["result"]["capabilities"]["tools"], json!({}));
}

#[test]
fn initialize_without_client_version_uses_latest() {
    let r = call("initialize", json!({}), 2);
    assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
}

#[test]
fn ping_returns_empty_result() {
    let r = call("ping", json!({}), 3);
    assert_eq!(r["result"], json!({}));
}

#[test]
fn tools_list_exposes_five_tools() {
    let r = call("tools/list", json!({}), 4);
    let tools = r["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["usage", "plans", "models", "daily", "hourly"]);
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object");
    }
}

#[test]
fn unknown_method_is_jsonrpc_error() {
    let r = call("resources/list", json!({}), 5);
    assert_eq!(r["error"]["code"], -32601);
}

#[test]
fn unknown_tool_is_iserror_result_not_rpc_error() {
    let r = call("tools/call", json!({ "name": "nope", "arguments": {} }), 6);
    assert!(r.get("error").is_none());
    assert_eq!(r["result"]["isError"], true);
}

#[test]
fn bad_tz_argument_is_invalid_params_error() {
    let r = call(
        "tools/call",
        json!({ "name": "daily", "arguments": { "tz": "mars" } }),
        7,
    );
    assert_eq!(r["error"]["code"], -32602);
}

#[test]
fn result_envelope_carries_request_id() {
    let r = call("tools/list", json!({}), 99);
    assert_eq!(r["id"], 99);
    assert_eq!(r["jsonrpc"], "2.0");
}
