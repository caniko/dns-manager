use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use url::Url;

use super::{mcp, Client, Credentials};

const ID: &str = "3a6b94b5-d20e-40bd-a7cc-521f5c79fab3";
const SECRET: &str = "minted-secret-that-must-not-appear-in-replies";

struct Exchange {
    request: String,
    token: &'static str,
    payload: Option<Value>,
    status: &'static str,
    headers: &'static str,
    response: Value,
}

fn stub(exchanges: Vec<Exchange>) -> (Url, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = Url::parse(&format!(
        "http://{}/api/v1/",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let task = thread::spawn(move || {
        for step in exchanges {
            let start = std::time::Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            && start.elapsed() < Duration::from_secs(2) =>
                    {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(err) => panic!("expected {}: {err}", step.request),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert_eq!(line.trim_end(), format!("{} HTTP/1.1", step.request));
            let mut auth = None;
            let mut len: usize = 0;
            loop {
                line.clear();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line.trim_end().is_empty() {
                    break;
                }
                let (k, v) = line.trim_end().split_once(':').unwrap();
                match k.to_ascii_lowercase().as_str() {
                    "authorization" => auth = Some(v.trim().to_owned()),
                    "content-length" => len = v.trim().parse().unwrap(),
                    _ => (),
                }
            }
            assert_eq!(auth.as_deref(), Some(step.token));
            if let Some(body) = step.payload {
                let mut bytes = vec![0; len];
                reader.read_exact(&mut bytes).unwrap();
                assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), body);
            }
            let body = if step.status.starts_with("204") {
                String::new()
            } else {
                step.response.to_string()
            };
            write!(
                stream,
                "HTTP/1.1 {}\r\nConnection: close\r\nContent-Length: {}\r\n{}\r\n{body}",
                step.status,
                body.len(),
                step.headers
            )
            .unwrap();
        }
    });
    (url, task)
}

fn calls(
    credentials: &Credentials,
    output_dir: Option<&std::path::Path>,
    calls: &[(&str, Value)],
) -> Vec<Value> {
    let mut input = Vec::new();
    writeln!(
        input,
        "{}",
        json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}
            }
        })
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
    )
    .unwrap();
    for (id, (name, args)) in calls.iter().enumerate() {
        writeln!(input, "{}", json!({
            "jsonrpc":"2.0","id":id+1,"method":"tools/call","params":{"name":name,"arguments":args}
        })).unwrap();
    }
    let mut out = Vec::new();
    mcp::serve(input.as_slice(), &mut out, credentials, output_dir).unwrap();
    std::str::from_utf8(&out)
        .unwrap()
        .lines()
        .skip(1)
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn content(response: &Value) -> &str {
    response["result"]["content"][0]["text"].as_str().unwrap()
}

#[test]
fn read_tools_use_read_token_and_follow_same_endpoint_cursors() {
    let reads = |request: &str, response| Exchange {
        request: request.into(),
        token: "Token read-token",
        payload: None,
        status: "200 OK",
        headers: "",
        response,
    };
    let keys = json!([{"dnskey":"257 3 13 fixture","ds":["12345 13 2 fixture"]}]);
    let (url, task) = stub(vec![
        reads("GET /api/v1/domains/?cursor=", json!([{"name":"example.com"}])),
        reads("GET /api/v1/domains/example.com/", json!({"name":"example.com", "keys":keys})),
        Exchange {
            headers: "Link: <?cursor=more>; rel=\"next\"\r\n",
            ..reads("GET /api/v1/domains/example.com/rrsets/?cursor=", json!([{"type":"A"}]))
        },
        reads("GET /api/v1/domains/example.com/rrsets/?cursor=more", json!([{"type":"TXT"}])),
        Exchange {
            headers: "Link: <https://other.invalid/api/v1/domains/example.com/rrsets/?cursor=more>; rel=\"next\"\r\n",
            ..reads("GET /api/v1/domains/example.com/rrsets/?cursor=", json!([]))
        },
    ]);
    let credentials = Credentials {
        read: Some(Client::at("read-token".into(), url.clone()).unwrap()),
        admin: Some(Client::at("admin-token".into(), url).unwrap()),
    };
    let name = json!({"name":"EXAMPLE.COM."});
    let replies = calls(
        &credentials,
        None,
        &[
            ("desec_domains", json!({})),
            ("desec_domain", name.clone()),
            ("desec_rrsets", name.clone()),
            ("desec_rrsets", name),
        ],
    );
    task.join().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(content(&replies[1])).unwrap()["keys"],
        keys
    );
    assert_eq!(
        serde_json::from_str::<Value>(content(&replies[2])).unwrap(),
        json!([{"type":"A"},{"type":"TXT"}])
    );
    assert_eq!(replies[3]["result"]["isError"], true);
    assert!(content(&replies[3]).contains("leaves the expected endpoint"));
}

struct PrivateDir(PathBuf);

impl PrivateDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "dns-manager-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        Self(path)
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn admin_mint_scopes_writes_and_never_returns_the_secret() {
    let output = PrivateDir::new();
    let admin = |request: &str, payload, response| Exchange {
        request: request.into(),
        token: "Token admin-token",
        payload,
        status: "201 Created",
        headers: "",
        response,
    };
    let mint = || {
        admin(
            "POST /api/v1/auth/tokens/",
            Some(json!({
                "name":"octodns","perm_create_domain":false,"perm_delete_domain":false,"perm_manage_tokens":false
            })),
            json!({"id":ID,"token":SECRET}),
        )
    };
    let policy = |body, status| Exchange {
        status,
        ..admin(
            &format!("POST /api/v1/auth/tokens/{ID}/policies/rrsets/"),
            Some(body),
            json!({"echo":SECRET}),
        )
    };
    let revoke = || Exchange {
        status: "204 No Content",
        ..admin(
            &format!("DELETE /api/v1/auth/tokens/{ID}/"),
            None,
            Value::Null,
        )
    };
    let (url, task) = stub(vec![
        admin(
            "POST /api/v1/domains/",
            Some(json!({"name":"example.com"})),
            json!({"name":"example.com"}),
        ),
        mint(),
        policy(json!({"perm_write":false}), "201 Created"),
        policy(
            json!({"domain":"example.com","subname":"","type":"A","perm_write":true}),
            "201 Created",
        ),
        revoke(),
        mint(),
        policy(json!({"perm_write":false}), "403 Forbidden"),
        revoke(),
    ]);
    let credentials = Credentials {
        read: None,
        admin: Some(Client::at("admin-token".into(), url).unwrap()),
    };
    let args = json!({"name":"octodns","out":"token","domain":"example.com","subname":"","type":"a","write":true});
    let replies = calls(
        &credentials,
        Some(&output.0),
        &[
            ("desec_domain_create", json!({"name":"example.com"})),
            (
                "desec_token_mint",
                json!({"name":"octodns","out":"../outside"}),
            ),
            ("desec_token_mint", args.clone()),
            ("desec_token_mint", args),
            ("desec_token_revoke", json!({"id":ID})),
            ("desec_token_mint", json!({"name":"octodns","out":"failed"})),
        ],
    );
    task.join().unwrap();
    for (reply, is_error) in replies.iter().zip([false, true, false, true, false, true]) {
        assert_eq!(reply["result"]["isError"], is_error, "{reply}");
        assert!(!reply.to_string().contains(SECRET), "{reply}");
    }
    assert!(content(&replies[5]).contains("403"));
    let minted: Value = serde_json::from_str(content(&replies[2])).unwrap();
    let path = output.0.join("token");
    assert_eq!(minted, json!({"id": ID, "path":path}));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("{SECRET}\n")
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!output.0.join("failed").exists());
}
