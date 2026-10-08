//! 供应商 JSON 的协议约束；业务准入继续使用合同领域规则。
use std::collections::BTreeMap;

use erp_contract::entity::recognition::{ContractExtraction, ContractField, ExtractedField};
use rig_core::completion::{CompletionResponse, FinishReason};
use rig_core::schemars::Schema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Result, invalid_output};

const FIELDS: [&str; 14] = [
    "contract_no",
    "customer_name",
    "customer_credit_code",
    "company_name",
    "company_credit_code",
    "settlement_name",
    "settlement_credit_code",
    "payment_terms",
    "invoice_type",
    "tax_point",
    "signed_at",
    "valid_from",
    "valid_to",
    "business_scope",
];

// 字段列表是 wire 形状，转换时拒绝重复键；领域继续使用 BTreeMap。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    fields: Vec<Field>,
    conflicts: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    field: ContractField,
    value: String,
    page: u32,
    quote: String,
}

pub(super) fn schema() -> Result<Schema> {
    // 微调模型仅支持结构化输出的基础子集；范围与大小限制由 convert 和领域校验执行。
    Schema::try_from(json!({
        "title": "contract_extraction_v1", "type": "object", "additionalProperties": false,
        "required": ["fields", "conflicts"],
        "properties": {
            "fields": {"type": "array", "items": {
                "type": "object", "additionalProperties": false,
                "required": ["field", "value", "page", "quote"],
                "properties": {
                    "field": {"type": "string", "enum": FIELDS},
                    "value": {"type": "string"},
                    "page": {"type": "integer"},
                    "quote": {"type": "string"}
                }
            }},
            "conflicts": {"type": "array", "items": {"type": "string"}}
        }
    }))
    .map_err(|_| invalid_output())
}

pub(super) fn decode(
    response: CompletionResponse,
    requested_model: &str,
    provider_id: &str,
) -> Result<ContractExtraction> {
    if response.finish_reason() != Some(FinishReason::Stop) || response.tool_calls().next().is_some() {
        return Err(invalid_output());
    }
    let text = response_text(&response.raw)?;
    if text.len() > 256_000 {
        return Err(invalid_output());
    }
    let output: Output = serde_json::from_str(&text).map_err(|_| invalid_output())?;
    let model = response.model.as_deref().unwrap_or("unreported");
    if model.is_empty() || model.len() > 96 || !model.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(invalid_output());
    }
    convert(
        output,
        format!("protocol=responses;requested={requested_model};reported={model};prompt=contract-v1"),
        provider_id,
    )
}

fn response_text(response: &Value) -> Result<String> {
    if response["status"] != "completed"
        || !response["error"].is_null()
        || !response["incomplete_details"].is_null()
    {
        return Err(invalid_output());
    }
    let output = response["output"].as_array().ok_or_else(invalid_output)?;
    let mut message = None;
    for item in output {
        match item["type"].as_str() {
            // 推理内容不作为提取结果；正文必须是唯一且已完成的 assistant 消息。
            Some("reasoning") if item["status"].is_null() || item["status"] == "completed" => {},
            Some("message") if message.is_none() => message = Some(item),
            _ => return Err(invalid_output()),
        }
    }
    message_text(message.ok_or_else(invalid_output)?)
}

fn message_text(message: &Value) -> Result<String> {
    if message["role"] != "assistant" || message["status"] != "completed" {
        return Err(invalid_output());
    }
    let content = message["content"].as_array().ok_or_else(invalid_output)?;
    let mut text = String::new();
    for part in content {
        if part["type"] != "output_text" {
            return Err(invalid_output());
        }
        text.push_str(part["text"].as_str().ok_or_else(invalid_output)?);
    }
    Ok(text)
}

fn convert(output: Output, version: String, provider_id: &str) -> Result<ContractExtraction> {
    if output.fields.len() > 14
        || output.conflicts.len() > 32
        || output.conflicts.iter().any(|text| text.len() > 2048)
    {
        return Err(invalid_output());
    }
    let mut fields = BTreeMap::new();
    for field in output.fields {
        if field.value.trim().is_empty()
            || field.value.len() > 4096
            || field.quote.len() > 8192
            || !(1..=200).contains(&field.page)
        {
            return Err(invalid_output());
        }
        let evidence = ExtractedField { value: field.value, page: field.page, quote: field.quote };
        if fields.insert(field.field, evidence).is_some() {
            return Err(invalid_output());
        }
    }
    let extraction =
        ContractExtraction { provider: provider_id.into(), version, fields, conflicts: output.conflicts };
    if serde_json::to_vec(&extraction).map_or(true, |bytes| bytes.len() > 256_000) {
        return Err(invalid_output());
    }
    Ok(extraction)
}
