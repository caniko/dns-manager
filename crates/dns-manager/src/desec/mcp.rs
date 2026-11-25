use std::fs;
use std::io::{BufRead, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Map, Value};

use super::{Credentials, MintArgs};
use crate::{Error, Result};

const VERSION: &str = "2025-11-25";
const MAX_MESSAGE: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Name {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

fn tool(
    name: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    read_only: bool,
) -> Value {
    json!({
        "name": name, "description": description,
        "inputSchema": {
            "type": "object", "properties": properties, "required": required, "additionalProperties": false
        },
        "annotations": {"readOnlyHint": read_only}
    })
}

fn tools() -> Value {
    let string = json!({"type": "string"});
    let name = json!({"name": string});
    json!({
        "tools": [
            tool("desec_domains", "List deSEC domains", json!({}), &[], true),
            tool("desec_domain", "Read a domain including DNSKEY and DS records", name.clone(), &["name"], true),
            tool("desec_rrsets", "Read a domain's RRsets", name.clone(), &["name"], true),
            tool("desec_domain_create", "Create an empty domain (admin token required)", name, &["name"], false),
            tool("desec_token_mint", "Mint a token into a new file in the configured private output directory (admin token required)",
                json!({
                    "name": string, "out": string, "domain": string, "subname": string, "type": string,
                    "write": {"type":"boolean"}, "perm_manage_tokens": {"type":"boolean"}
                }), &["name", "out"], false),
            tool("desec_token_revoke", "Revoke a token by UUID (admin token required)", json!({"id": string}), &["id"], false)
        ]
    })
}

type RpcError = (i64, &'static str);
const INVALID_PARAMS: RpcError = (-32602, "Invalid params");

fn arguments<T: DeserializeOwned>(args: Value) -> std::result::Result<T, RpcError> {
    serde_json::from_value(args).map_err(|_| INVALID_PARAMS)
}

fn token_path(root: Option<&Path>, requested: &Path) -> Result<PathBuf> {
    let root = root.ok_or_else(|| {
        Error::msg("MCP token mint requires --token-output-dir or DESEC_TOKEN_OUTPUT_DIR")
    })?;
    let root = root.canonicalize()?;
    let metadata = fs::metadata(&root)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(Error::msg(
            "MCP token output directory must be private (mode 0700)",
        ));
    }
    let name = requested
        .file_name()
        .ok_or_else(|| Error::msg("expected a token filename"))?;
    let parent = requested.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(parent) = parent {
        if !requested.is_absolute() || parent.canonicalize()? != root {
            return Err(Error::msg(
                "token output must be directly inside the configured private directory",
            ));
        }
    }
    Ok(root.join(name))
}

fn dispatch(
    credentials: &Credentials,
    output_dir: Option<&Path>,
    params: &Map<String, Value>,
) -> std::result::Result<Value, RpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or(INVALID_PARAMS)?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let result = match name {
        "desec_domains" => {
            let _: Empty = arguments(args)?;
            credentials.read().and_then(|api| api.domains())
        }
        "desec_domain" | "desec_rrsets" | "desec_domain_create" => {
            let Name { name: domain } = arguments(args)?;
            match name {
                "desec_domain" => credentials.read().and_then(|api| api.domain(&domain)),
                "desec_rrsets" => credentials.read().and_then(|api| api.rrsets(&domain)),
                _ => credentials
                    .admin()
                    .and_then(|api| api.domain_create(&domain)),
            }
        }
        "desec_token_mint" => {
            let args: MintArgs = arguments(args)?;
            credentials.admin().and_then(|api| {
                let out = token_path(output_dir, &args.out)?;
                Ok(serde_json::to_value(api.token_mint(&args, &out)?)?)
            })
        }
        "desec_token_revoke" => {
            let Id { id } = arguments(args)?;
            credentials.admin().and_then(|api| api.token_revoke(&id))
        }
        _ => return Err((-32602, "Unknown tool")),
    };
    let (text, is_error) = match result {
        Ok(value) => (value.to_string(), false),
        Err(err) => (err.to_string(), true),
    };
    Ok(json!({"content": [{"type":"text", "text":text}], "isError":is_error}))
}

fn error(id: Value, (code, message): RpcError) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

#[derive(PartialEq)]
enum Session {
    New,
    Initializing,
    Ready,
}

impl Session {
    // https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle
    fn handle(
        &mut self,
        request: Value,
        credentials: &Credentials,
        output_dir: Option<&Path>,
    ) -> Option<Value> {
        let id = request.get("id");
        let method = request.get("method").and_then(Value::as_str);
        if request["jsonrpc"] != "2.0"
            || method.is_none()
            || id.is_some_and(|id| !(id.is_string() || id.is_i64() || id.is_u64()))
        {
            return Some(error(Value::Null, (-32600, "Invalid Request")));
        }
        let method = method?;
        let Some(id) = id.cloned() else {
            if method == "notifications/initialized" && *self == Self::Initializing {
                *self = Self::Ready;
            }
            return None; // Notifications never run tools or yield replies.
        };
        let empty = Map::new();
        let params = match request.get("params") {
            None => &empty,
            Some(Value::Object(params)) => params,
            _ => return Some(error(id, INVALID_PARAMS)),
        };
        let result = match method {
            "initialize" if *self == Self::New => {
                let client = params.get("clientInfo");
                if let Some(version) = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .filter(|_| {
                        params.get("capabilities").is_some_and(Value::is_object)
                            && client
                                .is_some_and(|c| c["name"].is_string() && c["version"].is_string())
                    })
                {
                    let version = match version {
                        "2025-06-18" => version,
                        _ => VERSION,
                    };
                    *self = Self::Initializing;
                    Ok(json!({
                        "protocolVersion": version, "capabilities": {"tools": {}},
                        "serverInfo": {"name":"dns-manager-desec", "version":env!("CARGO_PKG_VERSION")}
                    }))
                } else {
                    Err(INVALID_PARAMS)
                }
            }
            "ping" => Ok(json!({})),
            "tools/list" | "tools/call" if *self != Self::Ready => {
                Err((-32002, "Server not initialized"))
            }
            "tools/list" if !params.contains_key("cursor") => Ok(tools()),
            "tools/list" => Err(INVALID_PARAMS),
            "tools/call" => dispatch(credentials, output_dir, params),
            _ => Err((-32601, "Method not found")),
        };
        Some(match result {
            Ok(result) => json!({"jsonrpc":"2.0", "id":id, "result":result}),
            Err(err) => error(id, err),
        })
    }
}

pub(super) fn serve(
    mut input: impl BufRead,
    mut out: impl Write,
    credentials: &Credentials,
    output_dir: Option<&Path>,
) -> Result<()> {
    let mut session = Session::New;
    let mut line = Vec::new();
    loop {
        line.clear();
        if input
            .by_ref()
            .take((MAX_MESSAGE + 1) as u64)
            .read_until(b'\n', &mut line)?
            == 0
        {
            return Ok(());
        }
        if line.len() > MAX_MESSAGE {
            return Err(Error::msg("MCP request exceeds the 1 MiB limit"));
        }
        let response = match serde_json::from_slice(&line) {
            Ok(request) => session.handle(request, credentials, output_dir),
            Err(_) => Some(error(Value::Null, (-32700, "Parse error"))),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut out, &response)?;
            writeln!(out)?;
            out.flush()?;
        }
    }
}
