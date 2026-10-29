//! The MCP server: JSON-RPC 2.0, one message per line, over stdin and stdout.
//!
//! [`Server::handle`] is the whole protocol and it is pure - a line in, at most
//! one line out - so the framing, the error codes and the tier checks are unit
//! tests, not a socket fixture. [`serve`] is the loop around it: read a line,
//! answer it, flush, and at the end of input let the handler clean up. There is
//! no listening socket and no thread: the process lives exactly as long as the
//! client that started it holds its stdin open.
//!
//! Every `tools/call` goes through the same gate, in this order: the name is
//! refused or unknown (a JSON-RPC error, logged), the arguments fail the schema
//! (a tool error the model can correct), the limit is spent (a tool error), and
//! only then does the handler run. The handler never sees a call that skipped a
//! step.

use std::collections::{BTreeMap, VecDeque};
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

use crate::manifest::{self, Limit, Tier, ToolSpec};
use crate::schema;

/// The protocol revisions this server speaks, newest first. A client asking for
/// one of them gets it back; any other gets the newest, which is what the
/// specification asks a server to answer.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// JSON-RPC error codes, plus MCP's resource-not-found.
pub mod code {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const RESOURCE_NOT_FOUND: i64 = -32002;
}

/// A line longer than this is refused unread: no tool takes more than a path
/// and a few short strings, and an unbounded read is how a pipe becomes memory.
pub const MAX_LINE: usize = 1 << 20;

/// What a tool call produced.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    /// For the model: what happened, or the data as text.
    pub text: String,
    /// The same data as JSON, for clients that read `structuredContent`.
    pub structured: Option<Value>,
}

impl Reply {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            structured: None,
        }
    }

    /// Data as pretty JSON text plus the same value structured.
    pub fn data(value: Value) -> Self {
        Self {
            text: serde_json::to_string_pretty(&value).unwrap_or_default(),
            structured: Some(value),
        }
    }
}

/// A document the agent can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime: String,
}

/// What answers the calls. The binary implements it with the programs the
/// menus run; the crate's tests implement it with a recorder.
pub trait Handler {
    /// Run a tool whose name, arguments and limit have already been checked.
    /// An `Err` is a failure the person or the agent can act on, in one line.
    fn call(&mut self, tool: &'static ToolSpec, args: &Map<String, Value>)
    -> Result<Reply, String>;

    fn resources(&mut self) -> Vec<Resource>;

    /// The text of one resource, or `None` for a URI it does not serve.
    fn read(&mut self, uri: &str) -> Option<(String, String)>;

    /// The client is gone: remove whatever the session left (spec 008: the
    /// screenshots).
    fn finish(&mut self) {}
}

/// One line per call in `mcp.log`: when, which tool, its tier, how it ended,
/// how long it took. Never the arguments - a clipboard, a path or a title is
/// the person's, and the log is a file anyone with the account can read.
pub struct CallLog {
    path: Option<PathBuf>,
}

/// The log is cut over at this size, keeping one previous file.
const LOG_LIMIT: u64 = 256 * 1024;

impl CallLog {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn record(&self, tool: &str, tier: &str, outcome: &str, elapsed: Duration) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::metadata(path).is_ok_and(|m| m.len() > LOG_LIMIT) {
            let _ = std::fs::rename(path, path.with_extension("log.1"));
        }
        let line = format!(
            "{}\t{}\t{}\t{}\t{}ms\n",
            utc_now(),
            clean(tool),
            tier,
            outcome,
            elapsed.as_millis()
        );
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

/// A name from the client goes into the log only as printable ASCII without
/// tabs: an unknown tool's name is attacker text, and a log is read by people.
fn clean(name: &str) -> String {
    name.chars()
        .take(64)
        .map(|c| if c.is_ascii_graphic() { c } else { '?' })
        .collect()
}

/// `YYYY-MM-DDTHH:MM:SSZ` without a date crate (days-from-civil, inverted).
pub fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    utc(secs)
}

pub fn utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

pub struct Server<H: Handler> {
    handler: H,
    log: CallLog,
    name: String,
    version: String,
    /// When each limited tool last ran, newest last.
    calls: BTreeMap<&'static str, VecDeque<Instant>>,
}

impl<H: Handler> Server<H> {
    pub fn new(handler: H, log: CallLog, version: impl Into<String>) -> Self {
        Self {
            handler,
            log,
            name: "lazyos".to_string(),
            version: version.into(),
            calls: BTreeMap::new(),
        }
    }

    pub fn handler(&mut self) -> &mut H {
        &mut self.handler
    }

    /// One message in, at most one message out. A notification, or a response
    /// from the client, gets no answer at all - JSON-RPC forbids one.
    pub fn handle(&mut self, line: &str) -> Option<String> {
        let message: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => return Some(error(Value::Null, code::PARSE_ERROR, "Parse error")),
        };
        if message.is_array() {
            // Batches left the protocol in 2025-06-18, and one call at a time is
            // what keeps the log and the limits in order.
            return Some(error(
                Value::Null,
                code::INVALID_REQUEST,
                "Invalid Request: batches are not supported",
            ));
        }
        let Some(object) = message.as_object() else {
            return Some(error(Value::Null, code::INVALID_REQUEST, "Invalid Request"));
        };
        let id = object.get("id").cloned();
        let id_ok = matches!(id, Some(Value::String(_) | Value::Number(_)));
        if id.is_some() && !id_ok {
            return Some(error(
                Value::Null,
                code::INVALID_REQUEST,
                "Invalid Request: an id is a string or a number",
            ));
        }
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return id.map(|id| {
                error(
                    id,
                    code::INVALID_REQUEST,
                    "Invalid Request: jsonrpc must be \"2.0\"",
                )
            });
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            // A response to a request this server never sends: nothing to say.
            if object.contains_key("result") || object.contains_key("error") {
                return None;
            }
            return Some(error(
                id.unwrap_or(Value::Null),
                code::INVALID_REQUEST,
                "Invalid Request: no method",
            ));
        };
        let params = match object.get("params") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(map)) => map.clone(),
            Some(_) => {
                return id.map(|id| {
                    error(
                        id,
                        code::INVALID_REQUEST,
                        "Invalid Request: params must be an object",
                    )
                });
            }
        };
        let Some(id) = id else {
            // Notifications: initialized, cancelled, roots changed. None needs an
            // answer, and none changes what this server does.
            return None;
        };
        Some(match self.dispatch(method, &params) {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string(),
            Err((code, message)) => error(id, code, &message),
        })
    }

    fn dispatch(
        &mut self,
        method: &str,
        params: &Map<String, Value>,
    ) -> Result<Value, (i64, String)> {
        match method {
            "initialize" => Ok(self.initialize(params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tools_list()})),
            "tools/call" => self.call(params),
            "resources/list" => {
                let resources: Vec<Value> = self
                    .handler
                    .resources()
                    .into_iter()
                    .map(|r| {
                        json!({"uri": r.uri, "name": r.name, "description": r.description, "mimeType": r.mime})
                    })
                    .collect();
                Ok(json!({"resources": resources}))
            }
            "resources/templates/list" => Ok(json!({"resourceTemplates": []})),
            "resources/read" => {
                let uri = params.get("uri").and_then(Value::as_str).ok_or((
                    code::INVALID_PARAMS,
                    "resources/read needs a uri".to_string(),
                ))?;
                match self.handler.read(uri) {
                    Some((mime, text)) => {
                        Ok(json!({"contents": [{"uri": uri, "mimeType": mime, "text": text}]}))
                    }
                    None => Err((
                        code::RESOURCE_NOT_FOUND,
                        format!("Resource not found: {}", clean(uri)),
                    )),
                }
            }
            "prompts/list" => Ok(json!({"prompts": []})),
            other => Err((
                code::METHOD_NOT_FOUND,
                format!("Method not found: {}", clean(other)),
            )),
        }
    }

    fn initialize(&self, params: &Map<String, Value>) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = asked
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": {
                "tools": {"listChanged": false},
                "resources": {"subscribe": false, "listChanged": false},
                "prompts": {"listChanged": false},
            },
            "serverInfo": {"name": self.name, "version": self.version},
            "instructions": INSTRUCTIONS,
        })
    }

    fn call(&mut self, params: &Map<String, Value>) -> Result<Value, (i64, String)> {
        let started = Instant::now();
        let name = params.get("name").and_then(Value::as_str).ok_or((
            code::INVALID_PARAMS,
            "tools/call needs a tool name".to_string(),
        ))?;
        if let Some(refused) = manifest::refused(name) {
            self.log
                .record(name, "refuse", "refused", started.elapsed());
            return Err((
                code::INVALID_PARAMS,
                format!(
                    "refused: {} is never exposed - {}",
                    clean(name),
                    refused.instead
                ),
            ));
        }
        let Some(tool) = manifest::find(name) else {
            self.log.record(name, "-", "unknown", started.elapsed());
            return Err((
                code::INVALID_PARAMS,
                format!("Unknown tool: {}", clean(name)),
            ));
        };
        let tier = tool.tier.label();
        let args = schema::arguments(params);
        if let Err(why) = schema::validate(&manifest::canonical_schema(tool), &args) {
            self.log.record(name, tier, "invalid", started.elapsed());
            return Ok(tool_error(&format!("invalid arguments: {why}")));
        }
        if let Some(wait) = self.spend(tool) {
            self.log.record(name, tier, "limited", started.elapsed());
            return Ok(tool_error(&format!(
                "{} is limited to {}; try again in {} s{}",
                tool.name,
                tool.limit.describe(),
                wait.as_secs().max(1),
                if tool.tier == Tier::Propose {
                    " - the window may still be open in front of the person"
                } else {
                    ""
                }
            )));
        }
        let args = args.as_object().cloned().unwrap_or_default();
        let outcome = self.handler.call(tool, &args);
        match outcome {
            Ok(reply) => {
                self.log.record(name, tier, "ok", started.elapsed());
                let mut result =
                    json!({"content": [{"type": "text", "text": reply.text}], "isError": false});
                if let Some(structured) = reply.structured {
                    result["structuredContent"] = if structured.is_object() {
                        structured
                    } else {
                        json!({"result": structured})
                    };
                }
                Ok(result)
            }
            Err(why) => {
                self.log.record(name, tier, "error", started.elapsed());
                Ok(tool_error(&why))
            }
        }
    }

    /// Spend one use of `tool`'s limit. `Some(wait)` when it is spent.
    fn spend(&mut self, tool: &'static ToolSpec) -> Option<Duration> {
        let now = Instant::now();
        let (window, allowed) = match tool.limit {
            Limit::None => return None,
            Limit::PerMinute(n) => (Duration::from_secs(60), n as usize),
            Limit::Cooldown(s) => (Duration::from_secs(u64::from(s)), 1),
        };
        let calls = self.calls.entry(tool.name).or_default();
        while calls
            .front()
            .is_some_and(|t| now.duration_since(*t) >= window)
        {
            calls.pop_front();
        }
        if calls.len() >= allowed {
            let oldest = *calls.front().expect("a spent limit has calls");
            return Some(window.saturating_sub(now.duration_since(oldest)));
        }
        calls.push_back(now);
        None
    }

    /// The end of the session.
    pub fn finish(&mut self) {
        self.handler.finish();
    }
}

/// What `initialize` tells the client, in the one paragraph a model reads
/// first. The full contract is the instructions file the image ships.
pub const INSTRUCTIONS: &str = "LazyOS desktop tools. read tools change nothing; act tools open \
a window or change something visible and reversible; propose tools only open the real gated \
window (install, format, swap, power) where the person decides - you never see their answer and \
nothing happens without it. There is no tool that runs a shell command, formats a disk, ends the \
session or writes a partition: point the person at the menu instead. Prefer a window that holds \
the answer over a notification.";

fn tools_list() -> Vec<Value> {
    manifest::TOOLS
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": format!("[{}] {}", tool.tier.label(), tool.description),
                "inputSchema": manifest::canonical_schema(tool),
                "annotations": {
                    "title": tool.title,
                    "readOnlyHint": tool.tier == Tier::Read,
                    "destructiveHint": false,
                    "idempotentHint": tool.tier == Tier::Read,
                    "openWorldHint": false,
                },
            })
        })
        .collect()
}

fn error(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

fn tool_error(text: &str) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": true})
}

/// Read messages from `input` until it ends, answer each on `output`, then let
/// the handler clean up. A line over [`MAX_LINE`] is answered with an error and
/// skipped without being kept in memory.
pub fn serve<H: Handler>(
    server: &mut Server<H>,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let read = Read::take(&mut *input, MAX_LINE as u64 + 1).read_until(b'\n', &mut buffer)?;
        if read == 0 {
            break;
        }
        if buffer.len() > MAX_LINE && buffer.last() != Some(&b'\n') {
            // Drain the rest of the oversized line, then refuse it.
            let mut sink = Vec::new();
            loop {
                sink.clear();
                let more = Read::take(&mut *input, MAX_LINE as u64).read_until(b'\n', &mut sink)?;
                if more == 0 || sink.last() == Some(&b'\n') {
                    break;
                }
            }
            writeln!(
                output,
                "{}",
                error(
                    Value::Null,
                    code::INVALID_REQUEST,
                    "Invalid Request: the message is too long"
                )
            )?;
            output.flush()?;
            continue;
        }
        let line = String::from_utf8_lossy(&buffer);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(answer) = server.handle(line) {
            writeln!(output, "{answer}")?;
            output.flush()?;
        }
    }
    server.finish();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder {
        calls: Vec<(String, Map<String, Value>)>,
        finished: bool,
    }

    impl Handler for Recorder {
        fn call(
            &mut self,
            tool: &'static ToolSpec,
            args: &Map<String, Value>,
        ) -> Result<Reply, String> {
            self.calls.push((tool.name.to_string(), args.clone()));
            match tool.name {
                "theme_set" => Err("no theme called that".to_string()),
                "desktop_status" => Ok(Reply::data(json!({"host": "lazyos"}))),
                _ => Ok(Reply::text(format!("proposed: {}", tool.name))),
            }
        }

        fn resources(&mut self) -> Vec<Resource> {
            vec![Resource {
                uri: "lazyos://manual/01-welcome.md".into(),
                name: "01-welcome.md".into(),
                description: "welcome".into(),
                mime: "text/markdown".into(),
            }]
        }

        fn read(&mut self, uri: &str) -> Option<(String, String)> {
            (uri == "lazyos://manual/01-welcome.md")
                .then(|| ("text/markdown".into(), "# Welcome".into()))
        }

        fn finish(&mut self) {
            self.finished = true;
        }
    }

    fn scratch_log(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lazyos-mcp-log-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("mcp.log")
    }

    fn server(log: Option<PathBuf>) -> Server<Recorder> {
        Server::new(Recorder::default(), CallLog::new(log), "0.4.0")
    }

    fn ask(server: &mut Server<Recorder>, request: Value) -> Value {
        let answer = server
            .handle(&request.to_string())
            .expect("a request is answered");
        serde_json::from_str(&answer).unwrap()
    }

    fn call(server: &mut Server<Recorder>, id: i64, name: &str, args: Value) -> Value {
        ask(
            server,
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}}),
        )
    }

    #[test]
    fn initialize_negotiates_the_version_and_names_the_server() {
        let mut s = server(None);
        let r = ask(
            &mut s,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}),
        );
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(r["result"]["serverInfo"]["name"], "lazyos");
        assert_eq!(r["result"]["serverInfo"]["version"], "0.4.0");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        let r = ask(
            &mut s,
            json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}),
        );
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn tools_list_is_the_table() {
        let mut s = server(None);
        let r = ask(
            &mut s,
            json!({"jsonrpc":"2.0","id":"a","method":"tools/list"}),
        );
        assert_eq!(r["id"], "a", "a string id comes back as it was sent");
        let tools = r["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        let table: Vec<&str> = manifest::TOOLS.iter().map(|t| t.name).collect();
        assert_eq!(names, table);
        for tool in tools {
            assert!(tool["inputSchema"].is_object());
            assert!(
                tool["description"].as_str().unwrap().starts_with('['),
                "the tier leads the description"
            );
        }
    }

    #[test]
    fn json_rpc_framing() {
        let mut s = server(None);
        // A notification is never answered, even an unknown one.
        assert_eq!(
            s.handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            None
        );
        assert_eq!(s.handle(r#"{"jsonrpc":"2.0","method":"no/such"}"#), None);
        // A response from the client (we send no requests) is ignored.
        assert_eq!(s.handle(r#"{"jsonrpc":"2.0","id":9,"result":{}}"#), None);
        let codes = [
            ("{not json", code::PARSE_ERROR),
            ("[]", code::INVALID_REQUEST),
            (
                r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#,
                code::INVALID_REQUEST,
            ),
            ("42", code::INVALID_REQUEST),
            (
                r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
                code::INVALID_REQUEST,
            ),
            (
                r#"{"jsonrpc":"2.0","id":{"x":1},"method":"ping"}"#,
                code::INVALID_REQUEST,
            ),
            (r#"{"jsonrpc":"2.0","id":1}"#, code::INVALID_REQUEST),
            (
                r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":[1]}"#,
                code::INVALID_REQUEST,
            ),
            (
                r#"{"jsonrpc":"2.0","id":1,"method":"sampling/createMessage"}"#,
                code::METHOD_NOT_FOUND,
            ),
            (
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{}}"#,
                code::INVALID_PARAMS,
            ),
            (
                r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"file:///etc/shadow"}}"#,
                code::RESOURCE_NOT_FOUND,
            ),
        ];
        for (line, wanted) in codes {
            let r: Value = serde_json::from_str(&s.handle(line).expect(line)).unwrap();
            assert_eq!(r["error"]["code"], wanted, "{line} -> {r}");
            assert_eq!(r["jsonrpc"], "2.0");
        }
        let r: Value = serde_json::from_str(
            &s.handle(r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(r, json!({"jsonrpc":"2.0","id":7,"result":{}}));
    }

    #[test]
    fn a_refused_name_is_an_error_the_log_keeps_and_the_server_survives() {
        let log = scratch_log("refused");
        let mut s = server(Some(log.clone()));
        let r = call(&mut s, 1, "format_apply", json!({"device": "/dev/mmcblk0"}));
        assert_eq!(r["error"]["code"], code::INVALID_PARAMS);
        let message = r["error"]["message"].as_str().unwrap();
        assert!(message.starts_with("refused: format_apply"), "{message}");
        assert!(
            message.contains("format_propose"),
            "it names the way that exists: {message}"
        );
        let r = call(&mut s, 2, "profile_export", json!({}));
        assert!(
            r["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Developer")
        );
        let r = call(&mut s, 3, "rm_rf\twith\ttabs", json!({}));
        assert_eq!(r["error"]["code"], code::INVALID_PARAMS);
        assert!(
            s.handler().calls.is_empty(),
            "the handler never saw any of them"
        );
        // Still alive and answering.
        assert_eq!(
            ask(&mut s, json!({"jsonrpc":"2.0","id":4,"method":"ping"}))["result"],
            json!({})
        );
        let text = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(
            lines[0].contains("\tformat_apply\trefuse\trefused\t"),
            "{}",
            lines[0]
        );
        assert!(
            lines[2].contains("\trm_rf?with?tabs\t-\tunknown\t"),
            "a client's name is cleaned: {}",
            lines[2]
        );
        assert!(!text.contains("mmcblk0"), "arguments never reach the log");
    }

    #[test]
    fn a_bad_call_is_a_tool_error_the_model_can_correct() {
        let mut s = server(None);
        let r = call(&mut s, 1, "power_propose", json!({"action": "format"}));
        assert_eq!(r["result"]["isError"], true);
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("must be one of")
        );
        assert!(s.handler().calls.is_empty());
    }

    #[test]
    fn a_good_call_reaches_the_handler_with_its_arguments() {
        let mut s = server(None);
        let r = call(&mut s, 1, "desktop_status", json!({}));
        assert_eq!(r["result"]["isError"], false);
        assert_eq!(r["result"]["structuredContent"]["host"], "lazyos");
        let r = call(&mut s, 2, "power_propose", json!({"action": "reboot"}));
        assert_eq!(r["result"]["content"][0]["text"], "proposed: power_propose");
        assert_eq!(s.handler().calls[1].1["action"], "reboot");
        let r = call(&mut s, 3, "theme_set", json!({"name": "nope"}));
        assert_eq!(
            r["result"]["isError"], true,
            "a handler failure is a tool error, not a protocol one"
        );
    }

    #[test]
    fn a_propose_tool_cools_down_and_notify_is_capped() {
        let mut s = server(None);
        assert_eq!(
            call(&mut s, 1, "power_propose", json!({"action": "reboot"}))["result"]["isError"],
            false
        );
        let again = call(&mut s, 2, "power_propose", json!({"action": "reboot"}));
        assert_eq!(again["result"]["isError"], true);
        assert!(
            again["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("still be open")
        );
        for id in 0..3 {
            assert_eq!(
                call(&mut s, 10 + id, "notify_send", json!({"title": "t"}))["result"]["isError"],
                false
            );
        }
        assert_eq!(
            call(&mut s, 20, "notify_send", json!({"title": "t"}))["result"]["isError"],
            true
        );
        let names: Vec<&str> = s.handler().calls.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec!["power_propose", "notify_send", "notify_send", "notify_send"]
        );
    }

    #[test]
    fn resources_list_and_read() {
        let mut s = server(None);
        let r = ask(
            &mut s,
            json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}),
        );
        assert_eq!(
            r["result"]["resources"][0]["uri"],
            "lazyos://manual/01-welcome.md"
        );
        let r = ask(
            &mut s,
            json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"lazyos://manual/01-welcome.md"}}),
        );
        assert_eq!(r["result"]["contents"][0]["text"], "# Welcome");
        let r = ask(
            &mut s,
            json!({"jsonrpc":"2.0","id":3,"method":"prompts/list"}),
        );
        assert_eq!(r["result"]["prompts"], json!([]));
    }

    #[test]
    fn serve_answers_line_by_line_and_cleans_up_at_the_end() {
        let mut s = server(None);
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        );
        let mut output = Vec::new();
        serve(&mut s, &mut io::Cursor::new(input), &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert_eq!(text.lines().count(), 2, "two requests, two answers: {text}");
        assert!(
            s.handler().finished,
            "the end of input is the end of the session"
        );
    }

    #[test]
    fn an_oversized_line_is_refused_without_keeping_it() {
        let mut s = server(None);
        let mut input = "x".repeat(MAX_LINE + 10);
        input.push('\n');
        input.push_str(r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#);
        input.push('\n');
        let mut output = Vec::new();
        serve(&mut s, &mut io::Cursor::new(input), &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].contains("too long"));
        assert!(lines[1].contains(r#""id":2"#));
    }

    #[test]
    fn utc_is_the_civil_calendar() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(utc(1_793_234_096), "2026-10-29T00:34:56Z");
    }
}
