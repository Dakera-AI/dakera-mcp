//! Readable explanations for the error answers of a Dakera v0.12 server.
//!
//! v0.12 answers every error with a JSON body (`{"error", "code", "status",
//! "details"}`), puts `Retry-After` on every `503`, and uses `403` for a key
//! that lacks the scope or is pinned to namespaces on a node-wide route, `413`
//! for a body over a limit or a hard quota, and `501` for a feature that is
//! switched off. The tool result keeps the server's text and adds one `Hint:`
//! line saying what the agent (or its operator) can do about it.

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
const HINT_NAMESPACE: &str =
    "This API key cannot reach that namespace; use a key whose namespace list includes it.";
const HINT_SUPER_ADMIN: &str = "This needs a global super_admin key: since Dakera v0.12 backup \
     download, upload and restore are refused to admin keys.";
const HINT_SCOPE: &str = "This API key's scope is too low; `required` in the details names the \
     scope the route needs.";
const HINT_AUTH: &str = "Authentication failed: set DAKERA_API_KEY to a valid, unexpired API key.";
const HINT_QUOTA: &str =
    "A hard namespace quota is exceeded (quotas are enforced since Dakera v0.12).";
const HINT_TOO_LARGE: &str = "The request body is larger than the server accepts (for an \
     attachment, the limit is attachments.max_bytes in dakera_capabilities).";
const HINT_DISABLED: &str = "This feature is switched off on the server; the message names the \
     environment variable that turns it on. dakera_capabilities shows what is enabled.";
const HINT_NO_ROUTE: &str = "This server has no such route: it probably predates Dakera v0.12 \
     (dakera_health shows its version).";
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
        (501, "FEATURE_DISABLED") => HINT_DISABLED,
        (501, _) => HINT_UNSUPPORTED,
        _ => return None,
    };
    Some(hint.to_string())
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
    fn unauthenticated() {
        let hint = error_hint(401, "{}", None).unwrap();
        assert!(hint.contains("DAKERA_API_KEY"));
    }
}
