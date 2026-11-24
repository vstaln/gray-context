//! gray-context — one sidecar for keeping the context window in budget and
//! putting the right text into it.
//!
//! Merges gray-truncate, gray-compact, gray-cacheopt, gray-header,
//! gray-context-pin, gray-resources, gray-rules, gray-include,
//! gray-dirty-guard and gray-gh-issues. Each source lives verbatim in `mods/`
//! with its own state files and slash commands — only the wire plumbing is
//! shared.
//!
//! Hook fan-out:
//! - `prompt/context`  → header + pin + include + dirty + cacheopt: `{text}`
//!   results are joined with blank lines (each module contributes or abstains).
//! - `tool/after`      → truncate (cap oversized tool results).
//! - `context/build`   → chain: compact (may replace the oldest 70% with a
//!   summary) → resources (prepend pinned files) → cacheopt observes the list
//!   that actually goes out.
//! - `agent/before_start` → rules (rule-file injection).
//! - `input/submit`    → ghissues (rewrite `#N` mentions with issue titles).
//! - `event/notify`    → cacheopt (`turn_end` usage → cache stats).
//! - `command/run`     → routed by `params.name` to the owning module.
//!
//! compact calls back into the host (`host/run`), so the wire loop uses its
//! reader-thread shape: string-id replies route to pending waiters, every
//! other frame queues for the main loop.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex, mpsc};

use serde_json::{Value, json};

mod parts;

#[allow(dead_code)]
#[path = "mods/cacheopt.rs"]
mod cacheopt;
#[allow(dead_code)]
#[path = "mods/compact.rs"]
mod compact;
#[allow(dead_code)]
#[path = "mods/dirty.rs"]
mod dirty;
#[allow(dead_code)]
#[path = "mods/ghissues.rs"]
mod ghissues;
#[allow(dead_code)]
#[path = "mods/header.rs"]
mod header;
#[allow(dead_code)]
#[path = "mods/include.rs"]
mod include;
#[allow(dead_code)]
#[path = "mods/pin.rs"]
mod pin;
#[allow(dead_code)]
#[path = "mods/resources.rs"]
mod resources;
#[allow(dead_code)]
#[path = "mods/rules.rs"]
mod rules;
#[allow(dead_code)]
#[path = "mods/truncate.rs"]
mod truncate;

const PARTS: &[parts::Part] = &[
    parts::Part {
        name: "truncate",
        default: true,
        description: "cap oversized tool results before they enter context",
    },
    parts::Part {
        name: "compact",
        default: true,
        description: "summarize older context to shrink it",
    },
    parts::Part {
        name: "cacheopt",
        default: true,
        description: "prompt-cache hit rate and prefix-break warnings",
    },
    parts::Part {
        name: "header",
        default: true,
        description: "a banner file injected into every turn",
    },
    parts::Part {
        name: "context-pin",
        default: true,
        description: "pinned files and notes in every turn",
    },
    parts::Part {
        name: "resources",
        default: true,
        description: "always-on files as a synthetic message",
    },
    parts::Part {
        name: "rules",
        default: true,
        description: "repo and home rule files at agent start",
    },
    parts::Part {
        name: "include",
        default: true,
        description: "@path expansion in agent context files",
    },
    parts::Part {
        name: "dirty-guard",
        default: true,
        description: "uncommitted-changes note at turn start",
    },
    parts::Part {
        name: "gh-issues",
        default: true,
        description: "React to #N mentions by appending GitHub issue titles",
    },
];

fn manifest(parts: &parts::Parts) -> Value {
    let mut commands: Vec<&str> = Vec::new();
    let mut descriptions = serde_json::Map::new();
    if parts.on("truncate") {
        commands.push("/truncate");
        descriptions.insert("/truncate".into(), json!("cap oversized tool results"));
    }
    if parts.on("compact") {
        commands.push("/compact");
        descriptions.insert(
            "/compact".into(),
            json!("summarize older context to shrink it"),
        );
    }
    if parts.on("cacheopt") {
        commands.push("/cachestats");
        descriptions.insert(
            "/cachestats".into(),
            json!("prompt-cache hit rate and prefix churn"),
        );
    }
    if parts.on("header") {
        commands.push("/header");
        descriptions.insert("/header".into(), json!("show the injected header file"));
    }
    if parts.on("context-pin") {
        commands.extend(["/pin", "/unpin", "/pins"]);
        descriptions.insert("/pin".into(), json!("pin a file or note into every turn"));
        descriptions.insert("/unpin".into(), json!("drop a pin"));
        descriptions.insert("/pins".into(), json!("list pins"));
    }
    if parts.on("resources") {
        commands.push("/resources");
        descriptions.insert("/resources".into(), json!("pin files as always-on context"));
    }
    if parts.on("rules") {
        commands.push("/rules");
        descriptions.insert("/rules".into(), json!("show which rule files get injected"));
    }
    if parts.on("include") {
        commands.push("/include");
        descriptions.insert(
            "/include".into(),
            json!("expand @path includes in AGENTS.md"),
        );
    }
    if parts.on("dirty-guard") {
        commands.push("/dirty");
        descriptions.insert("/dirty".into(), json!("toggle dirty work-tree warnings"));
    }
    if parts.on("gh-issues") {
        commands.push("/ghissues");
        descriptions.insert(
            "/ghissues".into(),
            json!("toggle GitHub issue lookup for #refs"),
        );
    }
    let mut completion: Vec<&str> = Vec::new();
    if parts.on("cacheopt") {
        completion.push("reset");
    }
    let mut hooks: Vec<&str> = Vec::new();
    if [
        "header",
        "context-pin",
        "include",
        "dirty-guard",
        "cacheopt",
    ]
    .iter()
    .any(|p| parts.on(p))
    {
        hooks.push("prompt/context");
    }
    if parts.on("truncate") {
        hooks.push("tool/after");
    }
    if parts.on("compact") || parts.on("cacheopt") || parts.on("resources") {
        hooks.push("context/build");
    }
    if parts.on("rules") {
        hooks.push("agent/before_start");
    }
    if parts.on("gh-issues") {
        hooks.push("input/submit");
    }
    let mut capabilities: Vec<&str> = Vec::new();
    if parts.on("compact") {
        capabilities.push("host.turn");
    }
    json!({
        "name": "context",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "2.0",
        "sidecar_args": ["sidecar"],
        "tools": [],
        "description": "context budget, compaction, cache stats and context injection",
        "commands": commands,
        "command_descriptions": descriptions,
        "completion": completion,
        "hooks": hooks,
        "capabilities": capabilities,
        "parts": parts.manifest(PARTS),
    })
}

type Handler = fn(&Value) -> (Option<Value>, bool);

/// Commands each module owns, routed by `command/run`'s `params.name`.
fn command_owner(name: &str, parts: &parts::Parts) -> Option<Handler> {
    match name {
        "/truncate" if parts.on("truncate") => Some(truncate::handle_truncate),
        "/header" if parts.on("header") => Some(header::handle_header),
        "/pin" | "/unpin" | "/pins" if parts.on("context-pin") => Some(pin::handle_pin),
        "/resources" if parts.on("resources") => Some(resources::handle_resources),
        "/rules" if parts.on("rules") => Some(rules::handle_rules),
        "/include" if parts.on("include") => Some(include::handle_include),
        "/dirty" if parts.on("dirty-guard") => Some(dirty::handle_dirty),
        "/ghissues" if parts.on("gh-issues") => Some(ghissues_shim),
        _ => None,
    }
}

/// ghissues' handler carries plugin state; `command/run` doesn't need it.
fn ghissues_shim(req: &Value) -> (Option<Value>, bool) {
    let mut p = ghissues::GhIssues::new();
    ghissues::handle_ghissues(&mut p, req)
}

/// Unwrap a module reply to its `result` (or `{}`), for re-wrapping.
fn result_of(reply: Option<Value>) -> Value {
    reply
        .and_then(|r| r.get("result").cloned())
        .unwrap_or_else(|| json!({}))
}

/// `prompt/context` fan-out: every contributing module returns `{text}`;
/// join the non-empty ones.
fn prompt_context(req: &Value, watch: &cacheopt::Watch, parts: &parts::Parts) -> Value {
    let mut texts: Vec<String> = Vec::new();
    let mut steps: Vec<Handler> = Vec::new();
    if parts.on("header") {
        steps.push(header::handle_header);
    }
    if parts.on("context-pin") {
        steps.push(pin::handle_pin);
    }
    if parts.on("include") {
        steps.push(include::handle_include);
    }
    if parts.on("dirty-guard") {
        steps.push(dirty::handle_dirty);
    }
    for h in steps {
        if let (Some(reply), _) = h(req)
            && let Some(t) = reply
                .get("result")
                .and_then(|r| r.get("text"))
                .and_then(Value::as_str)
        {
            let t = t.trim_end();
            if !t.is_empty() {
                texts.push(t.to_string());
            }
        }
    }
    if parts.on("cacheopt") {
        let (reply, _) = cacheopt::handle_cacheopt(watch, req);
        if let Some(t) = result_of(reply).get("text").and_then(Value::as_str) {
            let t = t.trim_end();
            if !t.is_empty() {
                texts.push(t.to_string());
            }
        }
    }
    if texts.is_empty() {
        json!({})
    } else {
        json!({ "text": texts.join("\n\n") })
    }
}

/// `context/build` chain: compact → resources → cacheopt; cacheopt is
/// observe-only.
fn context_build(
    req: &Value,
    watch: &cacheopt::Watch,
    link: Option<&compact::HostLink>,
    parts: &parts::Parts,
) -> Value {
    let mut messages: Option<Value> = None;
    let mut chain = req.clone();
    if parts.on("compact") {
        let (reply, _) = compact::handle_compact(&chain, link);
        if let Some(m) = result_of(reply).get("messages").cloned() {
            chain["params"]["messages"] = m.clone();
            messages = Some(m);
        }
    }
    if parts.on("resources") {
        let (reply, _) = resources::handle_resources(&chain);
        if let Some(m) = result_of(reply).get("messages").cloned() {
            chain["params"]["messages"] = m.clone();
            messages = Some(m);
        }
    }
    if parts.on("cacheopt") {
        let _ = cacheopt::handle_cacheopt(watch, &chain);
    }
    match messages {
        Some(m) => json!({ "messages": m }),
        None => json!({}),
    }
}

fn handle(
    req: &Value,
    gh: &mut ghissues::GhIssues,
    watch: &cacheopt::Watch,
    link: Option<&compact::HostLink>,
    parts: &parts::Parts,
) -> (Option<Value>, bool) {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let Some(id) = id else {
        // Notifications: event/notify turn_end feeds cacheopt's stats.
        if method == "event/notify" && parts.on("cacheopt") {
            let _ = cacheopt::handle_cacheopt(watch, req);
        }
        return (None, method == "plugin/shutdown");
    };
    let result = match method {
        "plugin/manifest" => manifest(parts),
        "prompt/context" => {
            if ![
                "header",
                "context-pin",
                "include",
                "dirty-guard",
                "cacheopt",
            ]
            .iter()
            .any(|p| parts.on(p))
            {
                return (Some(json!({ "id": id, "result": {} })), false);
            }
            prompt_context(req, watch, parts)
        }
        "tool/after" => {
            if !parts.on("truncate") {
                return (Some(json!({ "id": id, "result": {} })), false);
            }
            let (reply, _) = truncate::handle_truncate(req);
            return (reply, false);
        }
        "context/build" => {
            if !parts.on("compact") && !parts.on("cacheopt") && !parts.on("resources") {
                return (Some(json!({ "id": id, "result": {} })), false);
            }
            context_build(req, watch, link, parts)
        }
        "agent/before_start" => {
            if !parts.on("rules") {
                return (Some(json!({ "id": id, "result": {} })), false);
            }
            let (reply, _) = rules::handle_rules(req);
            return (reply, false);
        }
        "input/submit" => {
            if !parts.on("gh-issues") {
                return (Some(json!({ "id": id, "result": {} })), false);
            }
            let (reply, _) = ghissues::handle_ghissues(gh, req);
            return (reply, false);
        }
        "command/run" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            match name {
                "/compact" if parts.on("compact") => {
                    result_of(compact::handle_compact(req, link).0)
                }
                "/cachestats" if parts.on("cacheopt") => {
                    result_of(cacheopt::handle_cacheopt(watch, req).0)
                }
                _ => match command_owner(name, parts) {
                    Some(owner) => result_of(owner(req).0),
                    None => json!({ "text": format!("unknown command {name}") }),
                },
            }
        }
        "plugin/shutdown" => return (Some(json!({ "id": id, "result": {} })), true),
        _ => {
            let error = json!({ "code": -32601, "message": "method not found" });
            return (Some(json!({ "id": id, "error": error })), false);
        }
    };
    (Some(json!({ "id": id, "result": result })), false)
}

fn main() {
    let parts = parts::Parts::load("context", PARTS);
    if std::env::args().nth(1).as_deref() == Some("manifest") {
        println!("{}", manifest(&parts));
        return;
    }
    let stdout = Arc::new(Mutex::new(std::io::stdout()));
    let pending: compact::Pending = Arc::new(Mutex::new(HashMap::new()));
    let counter = Arc::new(Mutex::new(0u64));
    let watch = cacheopt::Watch::default();
    let mut gh = ghissues::GhIssues::new();

    // Reader thread: `host/run` replies (string id, no method) route to
    // pending waiters; everything else queues for the main loop.
    let (work_tx, work_rx) = mpsc::channel::<Value>();
    let reader_pending = pending.clone();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(id) = v.get("id").and_then(|i| i.as_str())
                && v.get("method").is_none()
                && let Some(tx) = reader_pending.lock().expect("pending").remove(id)
            {
                let _ = tx.send(v);
                continue;
            }
            if work_tx.send(v).is_err() {
                break;
            }
        }
    });

    for req in work_rx {
        let link = compact::HostLink {
            out: stdout.clone(),
            pending: pending.clone(),
            counter: counter.clone(),
        };
        let (reply, exit) = handle(&req, &mut gh, &watch, Some(&link), &parts);
        if let Some(reply) = reply {
            let mut o = stdout.lock().expect("stdout");
            let _ = writeln!(o, "{reply}");
            let _ = o.flush();
        }
        if exit {
            break;
        }
    }
}

#[cfg(test)]
mod test_env {
    pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}
