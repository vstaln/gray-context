//! gray-context-inject — one sidecar for every "put text into context" plugin.
//!
//! Merges gray-header, gray-context-pin, gray-resources, gray-rules,
//! gray-include, gray-dirty-guard and gray-gh-issues. Each source lives
//! verbatim in `mods/` with its own state files and slash commands — only
//! the wire plumbing is shared.
//!
//! Hook fan-out:
//! - `prompt/context`  → header + pin + include + dirty: `{text}` results are
//!   joined with blank lines (each module contributes or abstains).
//! - `context/build`   → resources (message prepend).
//! - `agent/before_start` → rules (rule-file injection).
//! - `input/submit`    → ghissues (rewrite `#N` mentions with issue titles).
//! - `command/run`     → routed by `params.name` to the owning module.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

mod parts;

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

const PARTS: &[parts::Part] = &[
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
    let mut commands: Vec<&str> = vec!["/inject"];
    let mut descriptions = serde_json::Map::new();
    descriptions.insert(
        "/inject".into(),
        json!("list the context-injection modules"),
    );
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
    let mut hooks: Vec<&str> = Vec::new();
    if ["header", "context-pin", "include", "dirty-guard"]
        .iter()
        .any(|p| parts.on(p))
    {
        hooks.push("prompt/context");
    }
    if parts.on("resources") {
        hooks.push("context/build");
    }
    if parts.on("rules") {
        hooks.push("agent/before_start");
    }
    if parts.on("gh-issues") {
        hooks.push("input/submit");
    }
    json!({
        "name": "context-inject",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "2.0",
        "tools": [],
        "description": "inject",
        "commands": commands,
        "command_descriptions": descriptions,
        "hooks": hooks,
        "parts": parts.manifest(PARTS),
    })
}

type Handler = fn(&Value) -> (Option<Value>, bool);

/// Commands each module owns, routed by `command/run`'s `params.name`.
fn command_owner(name: &str, parts: &parts::Parts) -> Option<Handler> {
    match name {
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

/// `prompt/context` fan-out: every contributing module returns `{text}`;
/// join the non-empty ones.
fn prompt_context(req: &Value, parts: &parts::Parts) -> Value {
    let mut parts_text: Vec<String> = Vec::new();
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
                parts_text.push(t.to_string());
            }
        }
    }
    if parts_text.is_empty() {
        json!({})
    } else {
        json!({ "text": parts_text.join("\n\n") })
    }
}

/// `/inject` — merged-plugin overview.
fn inject_command(parts: &parts::Parts) -> String {
    let mut lines: Vec<String> = vec![format!(
        "gray-context-inject {} — one sidecar, seven injection sources:",
        env!("CARGO_PKG_VERSION")
    )];
    let rows: [(&str, &str); 7] = [
        (
            "header",
            "\x20 /header     ~/.gray/header.txt banner into every turn",
        ),
        (
            "context-pin",
            "\x20 /pin,/unpin,/pins  pinned files/notes into every turn",
        ),
        (
            "resources",
            "\x20 /resources  always-on files as a synthetic message",
        ),
        ("rules", "\x20 /rules      repo rule files at agent start"),
        (
            "include",
            "\x20 /include    @path expansion in context files",
        ),
        (
            "dirty-guard",
            "\x20 /dirty      uncommitted-changes note at turn start",
        ),
        (
            "gh-issues",
            "\x20 /ghissues   #N → GitHub issue titles in prompts",
        ),
    ];
    for (part, line) in rows {
        if parts.on(part) {
            lines.push(line.to_string());
        }
    }
    lines.join("\n")
}

fn handle(req: &Value, gh: &mut ghissues::GhIssues, parts: &parts::Parts) -> (Option<Value>, bool) {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let Some(id) = id else {
        return (None, method == "plugin/shutdown");
    };
    let result = match method {
        "plugin/manifest" => manifest(parts),
        "prompt/context" => {
            if !["header", "context-pin", "include", "dirty-guard"]
                .iter()
                .any(|p| parts.on(p))
            {
                return (Some(json!({ "id": id, "result": {} })), false);
            }
            prompt_context(req, parts)
        }
        "context/build" => {
            if !parts.on("resources") {
                return (Some(json!({ "id": id, "result": {} })), false);
            }
            let (reply, _) = resources::handle_resources(req);
            return (reply, false);
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
            if name == "/inject" {
                json!({ "text": inject_command(parts) })
            } else if let Some(owner) = command_owner(name, parts) {
                let (reply, _) = owner(req);
                match reply {
                    Some(r) => r.get("result").cloned().unwrap_or(json!({})),
                    None => json!({}),
                }
            } else {
                json!({ "text": format!("unknown command {name} — try /inject") })
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

fn main() -> std::io::Result<()> {
    let parts = parts::Parts::load("context-inject", PARTS);
    if std::env::args().nth(1).as_deref() == Some("manifest") {
        println!("{}", manifest(&parts));
        return Ok(());
    }
    let mut gh = ghissues::GhIssues::new();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        let Ok(req) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let (reply, exit) = handle(&req, &mut gh, &parts);
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
        if exit {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod test_env {
    pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}
