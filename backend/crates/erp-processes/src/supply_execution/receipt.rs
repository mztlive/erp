use std::collections::HashMap;

use erp_supply::dto::supplier_fulfillment::SupplierOrderResolution;
use services::{Error, Result};

#[derive(Debug, Clone)]
pub(super) struct InvestigationReceipt {
    pub(super) evidence_id: String,
    pub(super) order_version: u64,
    pub(super) task_version: Option<u64>,
}

#[derive(Debug, Clone)]
pub(super) struct CompletionReceipt {
    pub(super) terminal_action_id: String,
    pub(super) order_version: u64,
    pub(super) task_version: u64,
    pub(super) resolution: SupplierOrderResolution,
}

pub(super) fn investigation_receipt_message(fingerprint: &str, receipt: &InvestigationReceipt) -> String {
    format!(
        "fp={fingerprint};e={};o={};t={}",
        receipt.evidence_id,
        receipt.order_version,
        receipt
            .task_version
            .map(|version| version.to_string())
            .unwrap_or_else(|| "-".to_string())
    )
}

pub(super) fn parse_investigation_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<InvestigationReceipt> {
    let fields = receipt_fields(message)?;
    if fields.get("fp").map(String::as_str) != Some(expected_fingerprint) {
        return Err(Error::ConflictError("请求标识已用于不同的调查命令".to_string()));
    }
    let evidence_id = fields
        .get("e")
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| Error::Internal("W26 调查收据缺少证据ID".to_string()))?;
    let order_version = parse_receipt_version(fields.get("o"), "订单版本")?;
    let task_version = match fields.get("t").map(String::as_str) {
        Some("-") => None,
        Some(value) => Some(parse_positive_version(value, "收据任务版本")?),
        None => return Err(Error::Internal("W26 调查收据缺少任务版本".to_string())),
    };
    Ok(InvestigationReceipt {
        evidence_id,
        order_version,
        task_version,
    })
}

pub(super) fn completion_receipt_message(fingerprint: &str, receipt: &CompletionReceipt) -> String {
    format!(
        "fp={fingerprint};a={};o={};t={};r={}",
        receipt.terminal_action_id,
        receipt.order_version,
        receipt.task_version,
        receipt.resolution.as_str()
    )
}

pub(super) fn parse_completion_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<CompletionReceipt> {
    let fields = receipt_fields(message)?;
    if fields.get("fp").map(String::as_str) != Some(expected_fingerprint) {
        return Err(Error::ConflictError(
            "请求标识已用于不同的任务完成命令".to_string(),
        ));
    }
    let terminal_action_id = fields
        .get("a")
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| Error::Internal("W26 完成收据缺少业务证据ID".to_string()))?;
    let order_version = parse_receipt_version(fields.get("o"), "订单版本")?;
    let task_version = parse_receipt_version(fields.get("t"), "任务版本")?;
    let resolution = parse_resolution(
        fields
            .get("r")
            .ok_or_else(|| Error::Internal("W26 完成收据缺少业务结果".to_string()))?,
    )?;
    Ok(CompletionReceipt {
        terminal_action_id,
        order_version,
        task_version,
        resolution,
    })
}

fn receipt_fields(message: &str) -> Result<HashMap<String, String>> {
    let mut fields = HashMap::new();
    for part in message.split(';') {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| Error::Internal("W26 幂等收据格式非法".to_string()))?;
        if fields.insert(key.to_string(), value.to_string()).is_some() {
            return Err(Error::Internal("W26 幂等收据字段重复".to_string()));
        }
    }
    Ok(fields)
}

fn parse_receipt_version(value: Option<&String>, field: &str) -> Result<u64> {
    parse_positive_version(
        value
            .ok_or_else(|| Error::Internal(format!("W26 收据缺少{field}")))?
            .as_str(),
        field,
    )
    .map_err(|_| Error::Internal(format!("W26 收据{field}非法")))
}

fn parse_resolution(value: &str) -> Result<SupplierOrderResolution> {
    match value {
        "ORDER_ACCEPTED" => Ok(SupplierOrderResolution::OrderAccepted),
        "ORDER_REJECTED" => Ok(SupplierOrderResolution::OrderRejected),
        "ORDER_COMPLETED" => Ok(SupplierOrderResolution::OrderCompleted),
        "CANCELED" => Ok(SupplierOrderResolution::Canceled),
        "REFUNDED" => Ok(SupplierOrderResolution::Refunded),
        _ => Err(Error::Internal("W26 完成收据业务结果非法".to_string())),
    }
}

pub(super) use erp_supply::service::supplier_fulfillment::receipt::{
    parse_positive_version, serialized_fingerprint, stable_digest, stable_evidence_id,
    stable_internal_idempotency_key,
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_duplicate_fields_fail_before_payload_identity_and_missing_fields() {
        assert!(
            matches!(parse_investigation_receipt("fp=other;fp=other","expected"),Err(Error::Internal(message)) if message=="W26 幂等收据字段重复")
        );
        assert!(
            matches!(parse_completion_receipt("fp=other;fp=other","expected"),Err(Error::Internal(message)) if message=="W26 幂等收据字段重复")
        );
        assert!(matches!(
            parse_investigation_receipt("fp=other", "expected"),
            Err(Error::ConflictError(_))
        ));
        assert!(matches!(
            parse_completion_receipt("fp=other", "expected"),
            Err(Error::ConflictError(_))
        ));
    }
    #[test]
    fn receipt_forward_fields_and_trimmed_versions_keep_original_wire_behavior() {
        let receipt = parse_investigation_receipt("fp=same;e=evidence;o= 7 ;t=-;extra=x=y", "same").unwrap();
        assert_eq!(receipt.evidence_id, "evidence");
        assert_eq!(receipt.order_version, 7);
        assert_eq!(receipt.task_version, None);
        assert_eq!(
            investigation_receipt_message("same", &receipt),
            "fp=same;e=evidence;o=7;t=-"
        );
        let receipt =
            parse_completion_receipt("fp=same;a=terminal;o= 7 ;t= 3 ;r=REFUNDED;extra=x", "same").unwrap();
        assert_eq!(receipt.order_version, 7);
        assert_eq!(receipt.task_version, 3);
        assert_eq!(receipt.resolution, SupplierOrderResolution::Refunded);
        assert_eq!(
            completion_receipt_message("same", &receipt),
            "fp=same;a=terminal;o=7;t=3;r=REFUNDED"
        );
    }
    #[test]
    fn receipt_version_error_class_keeps_investigation_task_exception() {
        assert!(
            matches!(parse_investigation_receipt("fp=f;e=e;o=0;t=0","f"),Err(Error::Internal(message)) if message=="W26 收据订单版本非法")
        );
        assert!(
            matches!(parse_investigation_receipt("fp=f;e=e;o=1;t=0","f"),Err(Error::ValidationError(message)) if message=="收据任务版本必须为正整数字符串")
        );
        assert!(
            matches!(parse_completion_receipt("fp=f;a=a;o=1;t=0;r=REFUNDED","f"),Err(Error::Internal(message)) if message=="W26 收据任务版本非法")
        );
    }
}
