//! Readable explanations for the error answers of a Dakera v0.12 server.
//!
//! v0.12 answers every error with a JSON body (`{"error", "code", "status",
//! "details"}`), puts `Retry-After` on every `503`, and uses `403` for a key
//! that lacks the scope or is pinned to namespaces on a node-wide route, `413`
//! for a body over a limit or a hard quota, and `501` for a feature that is
//! switched off. The tool result keeps the server's text and adds one `Hint:`
//! line saying what the agent (or its operator) can do about it.
//!
//! Since v0.12.2 a refused field gets `400 INVALID_REQUEST` with an `error` of
//! the form `"<field>: <rule>"` (`content: content exceeds maximum of 100000
//! bytes …`, `memories[1].content: …`, `tags[1]: the tag 'dakera-curated' is
//! reserved …`); the hint repeats the field and the rule so the agent can fix
//! that argument and call again.

use serde_json::Value;

fn field(body: &Value, name: &str) -> String {
    match body.get(name).and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => String::new(),
    }
}

const HINT_PINNED: &str = "This API key is pinned to namespaces; since Dakera v0.12 such a key \
     is refused on node-wide /admin routes (backups, encryption, quotas, config). Use a key that \
     is not pinned to a namespace.";
const HINT_NAMESPACE: &str = "This API key cannot reach that namespace (an agent's memories \
     live in _dakera_agent_<agent_id>); use a key whose namespace list includes it. dakera_whoami \
     shows the key's namespaces (Dakera v0.12.2+).";
const HINT_SUPER_ADMIN: &str = "This needs a global super_admin key: since Dakera v0.12 backup \
     download, upload and restore are refused to admin keys.";
const HINT_SCOPE: &str = "This API key's scope is too low; `required` in the details names the \
     scope the route needs (dakera_whoami shows the key's scope, Dakera v0.12.2+).";
const HINT_AUTH: &str = "Authentication failed: set DAKERA_API_KEY to a valid, unexpired API key.";
const HINT_QUOTA: &str =
    "A hard namespace quota is exceeded (quotas are enforced since Dakera v0.12).";
const HINT_TOO_LARGE: &str = "The request body is larger than the server accepts (for an \
     attachment, the limit is attachments.max_bytes in dakera_capabilities).";
const HINT_DISABLED: &str = "This feature is switched off on the server; the message names the \
     environment variable that turns it on. dakera_capabilities shows what is enabled.";
const HINT_NO_ROUTE: &str = "This server has no such route: it probably predates Dakera v0.12 \
     (dakera_health shows its version).";
const HINT_NO_ROUTE_CODED: &str = "This server has no such route or method: it predates the \
     Dakera version this tool needs (dakera_health shows its version).";
const HINT_CONTENT_LIMIT: &str = "The text is over the server's content limit, which counts UTF-8 \
     bytes (not characters): split it into several smaller memories, or shorten it.";
const HINT_RESERVED: &str = "That tag, metadata key or id is reserved for what the server derives \
     itself; leave it out and call again.";
const HINT_NEW_KEY: &str = "Dakera servers before v0.12 require new_key: pass a passphrase or a \
     64-char hex key.";
const HINT_UNSUPPORTED: &str = "The server's configuration cannot serve this request; the \
     details name the settings to change.";

/// A hint for an error answer, from its status, JSON body and `Retry-After`
/// header. `None` when the answer has no well-known v0.12 meaning.
pub fn error_hint(status: u16, body: &str, retry_after_secs: Option<u64>) -> Option<String> {
    if status == 503 {
        return Some(match retry_after_secs {
            Some(secs) => format!("The server is busy or starting; retry in {secs}s."),
            None => "The server is busy or starting; retry shortly.".to_string(),
        });
    }
    if status == 429 {
        return Some(match retry_after_secs {
            Some(secs) => {
                format!("Rate limit reached for this key or namespace; retry in {secs}s.")
            }
            None => "Rate limit reached for this key or namespace; retry shortly.".to_string(),
        });
    }
    // No body at all: the server has no such route (an axum 404), which for a
    // tool written for Dakera v0.12 means an older server.
    if status == 404 && body.trim().is_empty() {
        return Some(HINT_NO_ROUTE.to_string());
    }
    // Dakera before v0.12 requires fields v0.12 made optional (rotate-key's new_key).
    if status == 422 && body.contains("missing field `new_key`") {
        return Some(HINT_NEW_KEY.to_string());
    }
    let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let code = field(&parsed, "code");
    let details = field(&parsed, "details");
    if status == 400 && code == "INVALID_REQUEST" {
        return invalid_field_hint(&field(&parsed, "error"));
    }
    let pinned = details.contains("namespace: *");
    let super_admin = details.contains("required: super_admin");
    let hint = match (status, code.as_str()) {
        (401, _) => HINT_AUTH,
        (403, "NAMESPACE_ACCESS_DENIED") if pinned => HINT_PINNED,
        (403, "NAMESPACE_ACCESS_DENIED") => HINT_NAMESPACE,
        (403, "INSUFFICIENT_SCOPE") if super_admin => HINT_SUPER_ADMIN,
        (403, "INSUFFICIENT_SCOPE") => HINT_SCOPE,
        (413, "QUOTA_EXCEEDED") => HINT_QUOTA,
        (413, _) => HINT_TOO_LARGE,
        (404, "ROUTE_NOT_FOUND") | (405, _) => HINT_NO_ROUTE_CODED,
        (501, "FEATURE_DISABLED") => HINT_DISABLED,
        (501, _) => HINT_UNSUPPORTED,
        _ => return None,
    };
    Some(hint.to_string())
}

/// The field a v0.12.2 validation message names (`"content"`,
/// `"memories[1].content"`, `"metadata._dakera_source"`), when it names one.
pub fn refused_field(message: &str) -> Option<&str> {
    let (field, rule) = message.split_once(": ")?;
    let named = !field.is_empty()
        && !rule.trim().is_empty()
        && field
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '[' | ']' | '-'));
    named.then_some(field)
}

/// A hint for a `400 INVALID_REQUEST` whose message names the refused field.
fn invalid_field_hint(message: &str) -> Option<String> {
    let field = refused_field(message)?;
    let specific = if message.contains("exceeds maximum of") && message.contains("bytes") {
        Some(HINT_CONTENT_LIMIT)
    } else if message.contains("reserved") {
        Some(HINT_RESERVED)
    } else {
        None
    };
    Some(match specific {
        Some(hint) => format!("The server refused `{field}`. {hint}"),
        None => format!(
            "The server refused `{field}` (the message says why); correct that argument and \
             call again."
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn body(code: &str, details: &str) -> String {
        json!({ "error": "refused", "code": code, "details": details }).to_string()
    }

    #[test]
    fn pinned_key_on_a_node_wide_route() {
        let hint = error_hint(403, &body("NAMESPACE_ACCESS_DENIED", "namespace: *"), None);
        let hint = hint.unwrap();
        assert!(hint.contains("pinned to namespaces"));
        assert!(hint.contains("node-wide"));
    }

    #[test]
    fn denied_namespace_is_not_the_pinned_hint() {
        let hint = error_hint(403, &body("NAMESPACE_ACCESS_DENIED", "namespace: a"), None);
        assert!(hint.unwrap().contains("cannot reach that namespace"));
    }

    #[test]
    fn super_admin_needed_for_backup_routes() {
        let details = "required: super_admin, actual: admin";
        let hint = error_hint(403, &body("INSUFFICIENT_SCOPE", details), None);
        assert!(hint.unwrap().contains("global super_admin key"));
    }

    #[test]
    fn low_scope() {
        let details = "required: admin, actual: read";
        let hint = error_hint(403, &body("INSUFFICIENT_SCOPE", details), None);
        assert!(hint.unwrap().contains("scope is too low"));
    }

    #[test]
    fn retry_after_on_503_and_429() {
        let hint = error_hint(503, "{}", Some(5)).unwrap();
        assert!(hint.contains("retry in 5s"));
        let hint = error_hint(429, "{}", None).unwrap();
        assert!(hint.contains("Rate limit"));
        assert!(hint.contains("retry shortly"));
        let hint = error_hint(429, "{}", Some(12)).unwrap();
        assert!(hint.contains("retry in 12s"));
    }

    #[test]
    fn payload_too_large_and_quota() {
        let big = error_hint(413, &body("PAYLOAD_TOO_LARGE", ""), None).unwrap();
        assert!(big.contains("attachments.max_bytes"));
        let quota = error_hint(413, &body("QUOTA_EXCEEDED", ""), None).unwrap();
        assert!(quota.contains("quota"));
    }

    #[test]
    fn feature_disabled_and_unsupported_configuration() {
        let json = body("FEATURE_DISABLED", "set DAKERA_ATTACHMENTS=1");
        let off = error_hint(501, &json, None);
        assert!(off.unwrap().contains("switched off"));
        let cfg = error_hint(501, &body("NOT_IMPLEMENTED", ""), None);
        assert!(cfg.unwrap().contains("configuration"));
    }

    #[test]
    fn unknown_answers_have_no_hint() {
        assert!(error_hint(404, "{}", None).is_none());
        assert!(error_hint(500, "not json", None).is_none());
    }

    #[test]
    fn a_route_an_older_server_lacks() {
        let hint = error_hint(404, "", None).unwrap();
        assert!(hint.contains("predates Dakera v0.12"));
        let body = "Failed to deserialize the JSON body into the target type: missing field `new_key` at line 1 column 2";
        assert!(error_hint(422, body, None).unwrap().contains("new_key"));
        assert!(error_hint(422, "other", None).is_none());
    }

    #[test]
    fn a_refused_field_is_named() {
        let json = json!({
            "error": "ttl_seconds: ttl_seconds must be at most 3153600000 (100 years)",
            "code": "INVALID_REQUEST",
            "status": 400
        })
        .to_string();
        let hint = error_hint(400, &json, None).unwrap();
        assert!(hint.contains("`ttl_seconds`"), "{hint}");
        assert!(hint.contains("correct that argument"), "{hint}");
    }

    #[test]
    fn content_over_the_byte_limit() {
        let json = json!({
            "error": "content: content exceeds maximum of 100000 bytes (100001 bytes)",
            "code": "INVALID_REQUEST"
        })
        .to_string();
        let hint = error_hint(400, &json, None).unwrap();
        assert!(hint.contains("`content`"), "{hint}");
        assert!(hint.contains("UTF-8 bytes"), "{hint}");
    }

    #[test]
    fn a_reserved_marker() {
        let msg = "tags[1]: the tag 'dakera-curated' is reserved for memories the server derives";
        let json = json!({"error": msg, "code": "INVALID_REQUEST"}).to_string();
        let hint = error_hint(400, &json, None).unwrap();
        assert!(hint.contains("`tags[1]`"), "{hint}");
        assert!(hint.contains("reserved"), "{hint}");
    }

    #[test]
    fn a_message_without_a_field_has_no_hint() {
        let msg = "idle_timeout_secs must be at most 2592000 (30 days)";
        let json = json!({"error": msg, "code": "INVALID_REQUEST"}).to_string();
        assert!(error_hint(400, &json, None).is_none());
        assert_eq!(refused_field("Invalid request: bad"), None);
        assert_eq!(
            refused_field("memories[3].content: too long"),
            Some("memories[3].content")
        );
    }

    #[test]
    fn coded_missing_routes() {
        let json = json!({"error": "no route", "code": "ROUTE_NOT_FOUND"}).to_string();
        assert!(error_hint(404, &json, None)
            .unwrap()
            .contains("no such route"));
        let json = json!({"error": "method", "code": "METHOD_NOT_ALLOWED"}).to_string();
        assert!(error_hint(405, &json, None)
            .unwrap()
            .contains("no such route"));
    }

    #[test]
    fn forbidden_hints_point_at_whoami() {
        let denied = error_hint(403, &body("NAMESPACE_ACCESS_DENIED", "namespace: a"), None);
        assert!(denied.unwrap().contains("dakera_whoami"));
        let scope = error_hint(403, &body("INSUFFICIENT_SCOPE", "required: write"), None);
        assert!(scope.unwrap().contains("dakera_whoami"));
    }

    #[test]
    fn unauthenticated() {
        let hint = error_hint(401, "{}", None).unwrap();
        assert!(hint.contains("DAKERA_API_KEY"));
    }
}
