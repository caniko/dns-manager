use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

#[test]
fn stdio_mcp_negotiates_lists_and_rejects_writes_without_admin() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dns-manager"))
        .args(["desec", "mcp-serve"])
        .env_remove("DESEC_TOKEN")
        .env_remove("DESEC_TOKEN_FILE")
        .env_remove("DESEC_ADMIN_TOKEN_FILE")
        .env_remove("DESEC_TOKEN_OUTPUT_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}
        }}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":"list","method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"desec_domain","arguments":{"name":"example.com"}
        }}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{
            "name":"desec_domain_create","arguments":{"name":"example.com"}
        }}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{
            "name":"desec_token_mint","arguments":{"name":"test","out":"token"}
        }}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{
            "name":"desec_token_revoke","arguments":{"id":"3a6b94b5-d20e-40bd-a7cc-521f5c79fab3"}
        }}),
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"desec_rrset_create"}}),
        json!({"jsonrpc":"2.0","id":8,"method":"ping"}),
    ];
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{{").unwrap(); // JSON-RPC parse error, then recover.
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let responses: Vec<Value> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(responses.len(), 9, "{stdout}");
    assert_eq!(responses[0]["error"]["code"], -32700);
    assert_eq!(responses[1]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(responses[2]["id"], "list");
    let tools = responses[2]["result"]["tools"].as_array().unwrap();
    let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "desec_domains",
            "desec_domain",
            "desec_rrsets",
            "desec_domain_create",
            "desec_token_mint",
            "desec_token_revoke",
        ]
    );
    assert!(responses[3]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("DESEC_TOKEN_FILE"));
    for response in &responses[4..7] {
        assert_eq!(response["result"]["isError"], true, "{response}");
        assert!(response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("DESEC_ADMIN_TOKEN_FILE"));
    }
    assert_eq!(responses[7]["error"]["code"], -32602);
    assert_eq!(responses[8]["result"], json!({}));
}
