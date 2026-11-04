//! Capability discovery (Dakera v0.12): `GET /v1/capabilities`.
//!
//! The document names the active embedding model, the search mode and scoring
//! strategy, the languages the per-request `lang` is honoured for, and which
//! opt-in features are on (attachments with speech to text, image indexing,
//! records). A Dakera v0.11 server has no such route.
//!
//! The server also uses it to switch tools off gracefully: tools that need a
//! feature the server has not turned on (attachments, image indexing) are left
//! out of `tools/list` and `dakera_discover_tools`, and a direct call answers
//! with the variable that turns the feature on instead of an HTTP error.
//!
//! Tools:
//!   - `dakera_capabilities` — GET /v1/capabilities

use serde_json::{json, Value};

use super::{ok_json, DakeraApiClient};
use crate::protocol::{CallToolResult, ToolDefinition};

/// What the server said about its capabilities.
#[derive(Debug, Clone)]
pub enum CapabilityState {
    /// `/v1/capabilities` answered with this document.
    Known(Value),
    /// The server has no such route (it predates Dakera v0.12).
    NotSupported,
    /// Could not find out (server unreachable, starting, or key refused).
    Unknown,
}

/// Whether `/v1/capabilities` reports `feature` (`attachments`, `vision`,
/// `records`) as enabled; `None` when it cannot be known.
pub fn feature_enabled(state: &CapabilityState, feature: &str) -> Option<bool> {
    match state {
        // Any server that serves /v1/capabilities is v0.12 or later.
        CapabilityState::Known(_) if feature == SERVER_V012 => Some(true),
        // A later v0.12 release: the document's `server_version` says.
        CapabilityState::Known(doc) if minimum_version(feature).is_some() => {
            let version = doc.get("server_version")?.as_str()?;
            Some(parse_version(version)? >= minimum_version(feature)?)
        }
        // `documents` is new in v0.12.3: a v0.12 document without the
        // section is a server that has no document routes.
        CapabilityState::Known(doc) if feature == DOCUMENTS && doc.get(feature).is_none() => {
            Some(false)
        }
        CapabilityState::Known(doc) => {
            let section = doc.get(feature)?;
            section.get("enabled")?.as_bool()
        }
        CapabilityState::NotSupported => Some(false),
        CapabilityState::Unknown => None,
    }
}

/// Not a section of the document: what a tool calling a route that only a
/// v0.12 server has needs (no opt-in variable).
pub const SERVER_V012: &str = "server_v012";

/// Not a section either: a tool that needs Dakera v0.12.3 or later (the
/// restorable-memory trash).
pub const SERVER_V0123: &str = "server_v0123";

/// Not a section either: a tool or argument that needs Dakera v0.12.4 or
/// later (forget by description or request, request detection, dry runs).
pub const SERVER_V0124: &str = "server_v0124";

/// The release a `server_v*` pseudo-feature stands for.
fn minimum_version(feature: &str) -> Option<(u64, u64, u64)> {
    match feature {
        SERVER_V0123 => Some((0, 12, 3)),
        SERVER_V0124 => Some((0, 12, 4)),
        _ => None,
    }
}

/// `(major, minor, patch)` of a version string (`0.12.4`, `0.12.4-rc1`).
pub fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version
        .trim()
        .trim_start_matches('v')
        .split(['.', '-', '+']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// Whether the server is at least the release `feature` stands for
/// ([`SERVER_V0123`], [`SERVER_V0124`]); `None` when it cannot be known.
pub fn server_at_least(state: &CapabilityState, feature: &str) -> Option<bool> {
    feature_enabled(state, feature)
}

/// The capability section of document import (Dakera v0.12.3, on by default).
pub const DOCUMENTS: &str = "documents";

/// The document tools.
const DOCUMENT_TOOLS: &[&str] = &[
    "dakera_import_document",
    "dakera_document_status",
    "dakera_list_documents",
    "dakera_delete_document",
];

/// Tools whose routes are new in Dakera v0.12 but need no opt-in feature.
const V012_ONLY_TOOLS: &[&str] = &["dakera_embed_migration_status", "dakera_encryption_status"];

/// Tools whose routes are new in Dakera v0.12.3.
const V0123_TOOLS: &[&str] = &["dakera_memory_trash", "dakera_memory_restore"];

/// Tools whose routes are new in Dakera v0.12.4.
const V0124_TOOLS: &[&str] = &["dakera_forget_detect"];

/// The capability sections an opt-in tool needs.
pub fn required_features(tool: &str) -> &'static [&'static str] {
    if V012_ONLY_TOOLS.contains(&tool) {
        return &[SERVER_V012];
    }
    if V0123_TOOLS.contains(&tool) {
        return &[SERVER_V0123];
    }
    if V0124_TOOLS.contains(&tool) {
        return &[SERVER_V0124];
    }
    if DOCUMENT_TOOLS.contains(&tool) {
        return &[DOCUMENTS];
    }
    if tool == "dakera_attachment_index_image" {
        return &["attachments", "vision"];
    }
    if tool.starts_with("dakera_attachment_") {
        return &["attachments"];
    }
    &[]
}

/// The server environment variable that turns a feature on.
pub fn feature_variable(feature: &str) -> &'static str {
    match feature {
        "attachments" => "DAKERA_ATTACHMENTS",
        "vision" => "DAKERA_VISION",
        "records" => "DAKERA_RECORDS",
        "documents" => "DAKERA_DOCUMENTS",
        _ => "the feature's DAKERA_* variable",
    }
}

/// Why `tool` cannot work against this server, when it cannot.
pub fn unavailable_reason(tool: &str, state: &CapabilityState) -> Option<String> {
    for feature in required_features(tool) {
        if feature_enabled(state, feature) == Some(false) {
            return Some(reason_for(tool, feature, state));
        }
    }
    None
}

fn reason_for(tool: &str, feature: &str, state: &CapabilityState) -> String {
    if matches!(state, CapabilityState::NotSupported) {
        return format!(
            "{tool} needs Dakera server v0.12 or later: this server has no /v1/capabilities, \
             so it predates v0.12."
        );
    }
    if let Some((major, minor, patch)) = minimum_version(feature) {
        let version = match state {
            CapabilityState::Known(doc) => doc
                .get("server_version")
                .and_then(|v| v.as_str())
                .unwrap_or("older"),
            _ => "older",
        };
        return format!(
            "{tool} needs Dakera server v{major}.{minor}.{patch} or later; this server is \
             {version}."
        );
    }
    if feature == DOCUMENTS {
        if let CapabilityState::Known(doc) = state {
            if doc.get(DOCUMENTS).is_none() {
                return format!(
                    "{tool} needs Dakera server v0.12.3 or later: this server's /v1/capabilities \
                     has no documents section, so it has no document import routes."
                );
            }
        }
    }
    let variable = feature_variable(feature);
    format!(
        "The '{feature}' feature is off on this Dakera server. Set {variable}=1 on the server \
         and restart it, then retry. dakera_capabilities shows what is enabled."
    )
}

/// Whether `tool` should be offered given what the server reported.
pub fn is_available(tool: &str, state: &CapabilityState) -> bool {
    unavailable_reason(tool, state).is_none()
}

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "dakera_capabilities".into(),
        description: "Discover what this Dakera server supports (v0.12+): active embedding model, \
            search mode, scoring strategy, accepted lang values, and which opt-in features are on \
            (attachments, speech to text, image indexing, records). Check before using attachment \
            tools. A pre-v0.12 server reports capabilities_available=false."
            .into(),
        input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
    }]
}

pub async fn execute(
    client: &DakeraApiClient,
    name: &str,
    _args: &serde_json::Value,
) -> Option<CallToolResult> {
    match name {
        "dakera_capabilities" => Some(tool_capabilities(client).await),
        _ => None,
    }
}

async fn tool_capabilities(client: &DakeraApiClient) -> CallToolResult {
    match client.probe_capabilities().await {
        Ok(CapabilityState::Known(doc)) => ok_json(&doc),
        Ok(_) => ok_json(&json!({
            "capabilities_available": false,
            "reason": "This server has no /v1/capabilities: it predates Dakera v0.12, so \
                       attachments, speech to text, image indexing and records are not available.",
        })),
        Err(e) => CallToolResult::error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> CapabilityState {
        CapabilityState::Known(json!({
            "attachments": {"enabled": true},
            "vision": {"enabled": false},
            "records": {"enabled": false}
        }))
    }

    #[test]
    fn server_versions_gate_the_newer_tools() {
        let v0123 = CapabilityState::Known(json!({"server_version": "0.12.3"}));
        let v0124 = CapabilityState::Known(json!({"server_version": "0.12.4"}));
        let unnamed = CapabilityState::Known(json!({}));
        assert_eq!(server_at_least(&v0123, SERVER_V0124), Some(false));
        assert_eq!(server_at_least(&v0124, SERVER_V0124), Some(true));
        assert_eq!(server_at_least(&v0123, SERVER_V0123), Some(true));
        assert_eq!(server_at_least(&unnamed, SERVER_V0124), None);
        let reason = unavailable_reason("dakera_forget_detect", &v0123).unwrap();
        assert!(
            reason.contains("v0.12.4") && reason.contains("0.12.3"),
            "{reason}"
        );
        assert!(is_available("dakera_forget_detect", &v0124));
        assert!(is_available("dakera_memory_trash", &v0123));
        assert!(!is_available(
            "dakera_memory_trash",
            &CapabilityState::NotSupported
        ));
        assert!(is_available(
            "dakera_forget_detect",
            &CapabilityState::Unknown
        ));
        assert_eq!(parse_version("v0.13.0-rc1"), Some((0, 13, 0)));
    }

    #[test]
    fn feature_enabled_reads_the_sections() {
        assert_eq!(feature_enabled(&doc(), "attachments"), Some(true));
        assert_eq!(feature_enabled(&doc(), "vision"), Some(false));
        assert_eq!(feature_enabled(&doc(), "nope"), None);
    }

    #[test]
    fn feature_enabled_for_the_other_states() {
        assert_eq!(
            feature_enabled(&CapabilityState::NotSupported, "attachments"),
            Some(false)
        );
        let unknown = CapabilityState::Unknown;
        assert_eq!(feature_enabled(&unknown, "attachments"), None);
    }

    #[test]
    fn required_features_per_tool() {
        let upload = required_features("dakera_attachment_upload");
        assert_eq!(upload, ["attachments"]);
        assert_eq!(
            required_features("dakera_attachment_index_image"),
            ["attachments", "vision"]
        );
        assert!(required_features("dakera_store").is_empty());
    }

    #[test]
    fn index_image_needs_vision_even_with_attachments_on() {
        let reason = unavailable_reason("dakera_attachment_index_image", &doc());
        assert!(reason.unwrap().contains("DAKERA_VISION"));
        assert!(unavailable_reason("dakera_attachment_upload", &doc()).is_none());
    }

    #[test]
    fn a_v011_server_has_no_attachment_tools() {
        let state = CapabilityState::NotSupported;
        let reason = unavailable_reason("dakera_attachment_list", &state).unwrap();
        assert!(reason.contains("v0.12"));
        assert!(is_available("dakera_store", &state));
    }

    #[test]
    fn v012_only_tools_are_dropped_on_a_v011_server_only() {
        for tool in ["dakera_embed_migration_status", "dakera_encryption_status"] {
            assert!(is_available(tool, &doc()));
            assert!(is_available(tool, &CapabilityState::Unknown));
            let reason = unavailable_reason(tool, &CapabilityState::NotSupported).unwrap();
            assert!(reason.contains("v0.12"), "{reason}");
        }
        // Rotation works on v0.11 too (with new_key).
        assert!(is_available(
            "dakera_encryption_rotate_key",
            &CapabilityState::NotSupported
        ));
    }

    #[test]
    fn document_tools_need_the_documents_section() {
        let on = CapabilityState::Known(json!({"documents": {"enabled": true}}));
        let off = CapabilityState::Known(json!({"documents": {"enabled": false}}));
        let v0122 = CapabilityState::Known(json!({"attachments": {"enabled": true}}));
        assert!(is_available("dakera_import_document", &on));
        let reason = unavailable_reason("dakera_list_documents", &off).unwrap();
        assert!(reason.contains("DAKERA_DOCUMENTS"), "{reason}");
        let reason = unavailable_reason("dakera_delete_document", &v0122).unwrap();
        assert!(reason.contains("v0.12.3"), "{reason}");
        assert!(is_available(
            "dakera_document_status",
            &CapabilityState::Unknown
        ));
        assert!(!is_available(
            "dakera_document_status",
            &CapabilityState::NotSupported
        ));
    }

    #[test]
    fn unknown_state_keeps_every_tool() {
        assert!(is_available(
            "dakera_attachment_upload",
            &CapabilityState::Unknown
        ));
    }

    #[test]
    fn definitions_name_the_tool() {
        let defs = definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "dakera_capabilities");
    }
}
