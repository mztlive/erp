//! Rig 提取 DTO 与本地准入校验；Schema 和参数反序列化由 SDK 生成及执行。
use std::collections::BTreeMap;

use erp_contract::entity::recognition::{ContractExtraction, ContractField, ExtractedField};
use rig_core::completion::{CompletionResponse, FinishReason};
use rig_core::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::PROMPT_VERSION;

// 列表保留模型返回的重复字段，转换到领域 BTreeMap 前必须拒绝重复。
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(crate = "rig_core::schemars")]
pub(super) struct Output {
    fields: Vec<Field>,
    conflicts: Vec<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(crate = "rig_core::schemars")]
struct Field {
    field: FieldName,
    value: String,
    page: u32,
    quote: String,
}

// 协议枚举仅用于派生 Schema，领域无需依赖 AI SDK 或 schemars。
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(crate = "rig_core::schemars")]
enum FieldName {
    ContractNo,
    CustomerName,
    CustomerCreditCode,
    CompanyName,
    CompanyCreditCode,
    SettlementName,
    SettlementCreditCode,
    PaymentTerms,
    InvoiceType,
    TaxPoint,
    SignedAt,
    ValidFrom,
    ValidTo,
    BusinessScope,
}

impl From<FieldName> for ContractField {
    fn from(value: FieldName) -> Self {
        match value {
            FieldName::ContractNo => Self::ContractNo,
            FieldName::CustomerName => Self::CustomerName,
            FieldName::CustomerCreditCode => Self::CustomerCreditCode,
            FieldName::CompanyName => Self::CompanyName,
            FieldName::CompanyCreditCode => Self::CompanyCreditCode,
            FieldName::SettlementName => Self::SettlementName,
            FieldName::SettlementCreditCode => Self::SettlementCreditCode,
            FieldName::PaymentTerms => Self::PaymentTerms,
            FieldName::InvoiceType => Self::InvoiceType,
            FieldName::TaxPoint => Self::TaxPoint,
            FieldName::SignedAt => Self::SignedAt,
            FieldName::ValidFrom => Self::ValidFrom,
            FieldName::ValidTo => Self::ValidTo,
            FieldName::BusinessScope => Self::BusinessScope,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum DecodeError {
    #[error("finish_reason: expected ToolCalls, received {0:?}")]
    FinishReason(Option<FinishReason>),
    #[error("response_state: status={status}, error={error}, incomplete_details={incomplete}")]
    ResponseState { status: Box<Value>, error: Box<Value>, incomplete: Box<Value> },
    #[error("response_shape: {0}")]
    Shape(&'static str),
    #[error("output_item: index={index}, type={kind}, status={status}")]
    OutputItem { index: usize, kind: Box<Value>, status: Box<Value> },
    #[error("json_encode: {0}")]
    Json(#[from] serde_json::Error),
    #[error("reported_model: invalid model {0:?}")]
    Model(String),
    #[error("output_limit: {path}={actual}, maximum={maximum}")]
    Limit { path: &'static str, actual: usize, maximum: usize },
    #[error("invalid_field: field={field:?}, reason={reason}")]
    Field { field: ContractField, reason: &'static str },
    #[error("duplicate_field: {0:?}")]
    Duplicate(ContractField),
}

type DecodeResult<T> = std::result::Result<T, DecodeError>;

// Rig 负责读取 submit 参数；hook 仅拒绝截断、额外输出及多个提交，不重写结果。
pub(super) fn validate_response(response: &CompletionResponse) -> DecodeResult<()> {
    if response.finish_reason() != Some(FinishReason::ToolCalls) {
        return Err(DecodeError::FinishReason(response.finish_reason()));
    }
    let raw = &response.raw;
    if raw["status"] != "completed" || !raw["error"].is_null() || !raw["incomplete_details"].is_null() {
        return Err(DecodeError::ResponseState {
            status: Box::new(raw["status"].clone()),
            error: Box::new(raw["error"].clone()),
            incomplete: Box::new(raw["incomplete_details"].clone()),
        });
    }
    let output = raw["output"].as_array().ok_or(DecodeError::Shape("output must be an array"))?;
    let mut submitted = false;
    for (index, item) in output.iter().enumerate() {
        match item["type"].as_str() {
            Some("reasoning") if item["status"].is_null() || item["status"] == "completed" => {},
            Some("function_call")
                if !submitted
                    && item["name"] == "submit"
                    && (item["status"].is_null() || item["status"] == "completed") =>
            {
                let arguments = item["arguments"]
                    .as_str()
                    .ok_or(DecodeError::Shape("submit arguments must be a string"))?;
                limit("submit_argument_bytes", arguments.len(), 256_000)?;
                submitted = true;
            },
            _ => {
                return Err(DecodeError::OutputItem {
                    index,
                    kind: Box::new(item["type"].clone()),
                    status: Box::new(item["status"].clone()),
                });
            },
        }
    }
    if !submitted {
        return Err(DecodeError::Shape("output is missing a submit call"));
    }
    Ok(())
}

pub(super) fn convert(
    output: Output,
    requested: &str,
    reported: &str,
    provider_id: &str,
) -> DecodeResult<ContractExtraction> {
    if reported.is_empty() || reported.len() > 96 || !reported.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(DecodeError::Model(reported.to_owned()));
    }
    let version =
        format!("protocol=responses;requested={requested};reported={reported};prompt={PROMPT_VERSION}");
    limit("fields_count", output.fields.len(), 14)?;
    limit("conflicts_count", output.conflicts.len(), 32)?;
    for conflict in &output.conflicts {
        limit("conflict_bytes", conflict.len(), 2048)?;
    }
    let mut fields = BTreeMap::new();
    for field in output.fields {
        let name = ContractField::from(field.field);
        if field.value.trim().is_empty() {
            return Err(DecodeError::Field { field: name, reason: "value is empty" });
        }
        limit("field.value_bytes", field.value.len(), 4096)?;
        limit("field.quote_bytes", field.quote.len(), 8192)?;
        if !(1..=200).contains(&field.page) {
            return Err(DecodeError::Field { field: name, reason: "page must be within 1..=200" });
        }
        let evidence = ExtractedField { value: field.value, page: field.page, quote: field.quote };
        if fields.insert(name, evidence).is_some() {
            return Err(DecodeError::Duplicate(name));
        }
    }
    let extraction =
        ContractExtraction { provider: provider_id.into(), version, fields, conflicts: output.conflicts };
    limit("extraction_bytes", serde_json::to_vec(&extraction)?.len(), 256_000)?;
    Ok(extraction)
}

fn limit(path: &'static str, actual: usize, maximum: usize) -> DecodeResult<()> {
    if actual > maximum {
        return Err(DecodeError::Limit { path, actual, maximum });
    }
    Ok(())
}
