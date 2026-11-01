//! Resend 兼容发送协议；禁止重定向及自动重试，未知结果由发件记录保留。
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use config::PurchaseMailConfig;
use erp_procurement::entity::purchase_email::{PurchaseEmail, PurchaseEmailStatus};
use reqwest::Client;
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde_json::json;

use super::PurchaseEmailResult;

#[derive(Deserialize)]
struct Response {
    id: String,
}
pub(super) async fn deliver(
    config: &PurchaseMailConfig,
    record: &PurchaseEmail,
    filename: &str,
    number: &str,
) -> PurchaseEmailResult {
    let unknown = || PurchaseEmailResult { status: PurchaseEmailStatus::Unknown, message_id: None };
    let Ok(client) = Client::builder()
        .timeout(Duration::from_secs(config.timeout_seconds))
        .redirect(Policy::none())
        .build()
    else {
        return unknown();
    };
    let response = client.post(&config.endpoint).bearer_auth(&config.api_key).header("Idempotency-Key", &record.base.id)
        .json(&json!({ "from": config.from, "to": [record.recipient], "subject": format!("采购单 {number}"),
            "html": record.html, "attachments": [{ "filename": filename, "content": STANDARD.encode(record.html.as_bytes()) }] }))
        .send().await;
    let Ok(response) = response else {
        return unknown();
    };
    let status = response.status().as_u16();
    let message_id = if response.status().is_success() {
        response.json::<Response>().await.ok().map(|value| value.id)
    } else {
        None
    };
    classify(status, message_id)
}
fn classify(status: u16, message_id: Option<String>) -> PurchaseEmailResult {
    let status =
        if (200..300).contains(&status) && message_id.as_ref().is_some_and(|id| !id.trim().is_empty()) {
            PurchaseEmailStatus::Accepted
        } else if (400..500).contains(&status) && status != 408 {
            PurchaseEmailStatus::Failed
        } else {
            PurchaseEmailStatus::Unknown
        };
    PurchaseEmailResult {
        status,
        message_id: if status == PurchaseEmailStatus::Accepted { message_id } else { None },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_requires_provider_receipt_and_ambiguous_failures_stay_unknown() {
        assert_eq!(classify(200, Some("provider-1".into())).status, PurchaseEmailStatus::Accepted);
        for status in [200, 201, 301, 408, 500, 503] {
            assert_eq!(classify(status, None).status, PurchaseEmailStatus::Unknown);
        }
        assert_eq!(classify(200, Some(" ".into())).status, PurchaseEmailStatus::Unknown);
        for status in [400, 401, 403, 422, 429] {
            assert_eq!(classify(status, None).status, PurchaseEmailStatus::Failed);
        }
    }
}
