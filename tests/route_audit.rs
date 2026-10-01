//! Every server path a tool calls must be a route of the Dakera v0.12.0 server.
//!
//! The route list (`tests/server_routes_v0.12.txt`) comes from the server's
//! router (`crates/api/src/lib.rs`); the paths the tools call are the string
//! literals in `src/tools/` that start with a route prefix (`/v1/`, `/admin/`,
//! `/ops/`, `/health`), with `{}` / `{x}` as a path parameter.

use std::collections::HashSet;
use std::fs;

fn server_paths() -> HashSet<String> {
    let text = include_str!("server_routes_v0.12.txt");
    let mut paths = HashSet::new();
    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }
        if let Some((_, path)) = line.split_once(' ') {
            paths.insert(path.to_string());
        }
    }
    paths
}

/// `{...}` becomes `*`; the query string is dropped.
fn normalize(literal: &str) -> String {
    let path = literal.split('?').next().unwrap_or("");
    let mut out = String::new();
    let mut depth = 0;
    for c in path.chars() {
        match c {
            '{' => {
                depth += 1;
                if depth == 1 {
                    out.push('*');
                }
            }
            '}' => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn route_literals(source: &str) -> Vec<String> {
    let production = source.split("#[cfg(test)]").next().unwrap_or("");
    let mut found = Vec::new();
    for (i, part) in production.split('"').enumerate() {
        if i % 2 == 0 {
            continue;
        }
        let routed = part.starts_with("/v1/")
            || part.starts_with("/admin/")
            || part.starts_with("/ops/")
            || part.starts_with("/health");
        if routed {
            found.push(normalize(part));
        }
    }
    found
}

fn sources() -> Vec<(String, String)> {
    let mut files = Vec::new();
    for dir in ["src/tools"] {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let text = fs::read_to_string(&path).unwrap();
                files.push((path.display().to_string(), text));
            }
        }
    }
    files
}

#[test]
fn every_route_a_tool_calls_exists_on_the_v012_server() {
    let server = server_paths();
    let mut missing = Vec::new();
    let mut checked = 0;
    for (file, text) in sources() {
        for path in route_literals(&text) {
            checked += 1;
            if !server.contains(&path) {
                missing.push(format!("{file}: {path}"));
            }
        }
    }
    assert!(checked > 30, "found only {checked} route literals");
    assert!(
        missing.is_empty(),
        "routes the server does not serve: {missing:#?}"
    );
}

#[test]
fn normalize_turns_parameters_into_stars() {
    assert_eq!(
        normalize("/v1/namespaces/{}/attachments"),
        "/v1/namespaces/*/attachments"
    );
    assert_eq!(normalize("/v1/sessions?agent_id={}"), "/v1/sessions");
    assert_eq!(
        normalize("/admin/keys/{key_id}/usage"),
        "/admin/keys/*/usage"
    );
}
