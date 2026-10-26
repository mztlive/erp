//! 任务上下文只公开固定业务引用，禁止回传上传内容、命令和授权依据。
use serde::Serialize;
use serde_json::{Value, json};

use super::TransferKind;

/// 后台执行状态之外的业务续办状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferResumeState {
    NotApplicable,
    Pending,
    Ready,
    Confirmed,
    Invalid,
    Stale,
    Expired,
    Unavailable,
}

/// 本人任务的受控业务上下文；不包含任意导航地址或原命令载荷。
#[derive(Serialize)]
pub struct TransferContext {
    pub kind: TransferKind,
    pub context: Value,
    pub can_cancel: bool,
    pub can_recover: bool,
    pub resume_state: TransferResumeState,
    pub resume_reason: Option<String>,
    pub preview_expires_at: Option<i64>,
}

impl TransferContext {
    pub(super) fn from_input(kind: TransferKind, input: &Value) -> Self {
        let mut context = json!({});
        for key in ["inquiry_id", "customer_id", "task_id", "expected_version", "task_version", "file_name"] {
            if let Some(value) = input.get(key).filter(|value| value.is_string() || value.is_u64()) {
                context[key] = value.clone();
            }
        }
        for key in ["mode", "resource", "view"] {
            let value = if kind == TransferKind::PortalBatch {
                input.get(key).or_else(|| input["request"].get(key))
            } else {
                input.get(key)
            };
            if let Some(value) = value.filter(|value| value.is_string()) {
                context[key] = value.clone();
            }
        }
        if let Some(files) = input["files"].as_array() {
            context["file_names"] =
                json!(files.iter().filter_map(|file| file["file_name"].as_str()).collect::<Vec<_>>());
        }
        if let Some(names) = input["file_names"].as_array() {
            context["file_names"] = json!(names.iter().filter_map(Value::as_str).collect::<Vec<_>>());
        }
        Self {
            kind,
            context,
            can_cancel: false,
            can_recover: false,
            resume_state: TransferResumeState::NotApplicable,
            resume_reason: None,
            preview_expires_at: None,
        }
    }

    pub(super) fn state(&mut self, state: TransferResumeState, reason: &str) {
        self.resume_state = state;
        self.resume_reason = Some(reason.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_allowlist_excludes_file_bytes_commands_and_authorization() {
        let view = TransferContext::from_input(
            TransferKind::SupplierQuotePreview,
            &json!({
                "inquiry_id":"inquiry", "expected_version":3, "task_id":"task", "file_name":"报价.xlsx",
                "files":[{"file_name":"甲.xlsx","content_base64":"SECRET"}],
                "content_base64":"SECRET", "rows":[{"command_key":"SECRET"}],
                "authorization":{"secret":"SECRET"}, "url":"https://example.invalid"
            }),
        );
        assert_eq!(view.context["inquiry_id"], "inquiry");
        assert_eq!(view.context["file_names"], json!(["甲.xlsx"]));
        let encoded = serde_json::to_string(&view).unwrap();
        assert!(!encoded.contains("SECRET"));
        assert!(!encoded.contains("example.invalid"));
        assert_eq!(view.resume_state, TransferResumeState::NotApplicable);
    }

    #[test]
    fn portal_mode_comes_from_server_wrapped_request_without_exposing_private_identity() {
        let view = TransferContext::from_input(
            TransferKind::PortalBatch,
            &json!({
                "request":{"mode":"availability","rows":[{"idempotency_key":"SECRET"}]},
                "session":{"account":"SECRET","binding_version":2}
            }),
        );
        assert_eq!(view.context["mode"], "availability");
        assert!(!serde_json::to_string(&view).unwrap().contains("SECRET"));
    }
}
