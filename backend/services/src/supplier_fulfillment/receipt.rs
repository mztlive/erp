use std::collections::HashMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::dto::SupplierOrderResolution;
use crate::errors::{Error, Result};

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

pub(super) fn parse_positive_version(value: &str, field: &str) -> Result<u64> {
    let version = value
        .trim()
        .parse::<u64>()
        .map_err(|_| Error::ValidationError(format!("{field}必须为正整数字符串")))?;
    if version == 0 {
        return Err(Error::ValidationError(format!("{field}必须为正整数字符串")));
    }
    Ok(version)
}

pub(super) fn serialized_fingerprint<T: Serialize>(command: &T) -> Result<String> {
    let bytes = serde_json::to_vec(command)
        .map_err(|error| Error::Internal(format!("命令指纹序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub(super) fn stable_evidence_id(prefix: &str, audit_id: &str) -> String {
    format!("{prefix}-{}", stable_digest(audit_id))
}

pub(super) fn stable_internal_idempotency_key(prefix: &str, audit_id: &str) -> String {
    format!("{prefix}:{}", stable_digest(audit_id))
}

pub(super) fn stable_digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
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
