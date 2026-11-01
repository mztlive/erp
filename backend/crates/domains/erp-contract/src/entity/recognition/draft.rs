//! 尽量预填；缺失、冲突和不可用的字段留空，归档前由用户确认。
use std::collections::BTreeMap;

use ContractField::*;
use serde::{Deserialize, Serialize};

use super::{ContractExtraction, ContractField, ImportFailure, OcrDocument, ValidatedFields};

const FIELDS: [ContractField; 14] = [
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
];

#[derive(Clone, Serialize)]
pub struct RecognitionDraft {
    pub fields: BTreeMap<ContractField, Option<String>>,
    pub warnings: Vec<String>,
}

/// 不带原文证据的业务输入，用于预填转换及用户确认校验。
pub struct ContractValues {
    pub fields: BTreeMap<ContractField, String>,
}

impl From<&ContractExtraction> for ContractValues {
    fn from(extraction: &ContractExtraction) -> Self {
        Self { fields: extraction.fields.iter().map(|(key, field)| (*key, field.value.clone())).collect() }
    }
}

/// 确认命令与原始识别结果分开保存，版本用于拒绝并发和异载荷重放。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmImport {
    pub version: u64,
    pub fields: BTreeMap<ContractField, Option<String>>,
    /// 用户确认允许在归档事务内补建缺失的客户身份。
    #[serde(default, skip_serializing_if = "is_false")]
    pub create_customer: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl ContractExtraction {
    /// 尽量转换成可编辑字段；单字段失败不阻止返回其余结果。
    /// # 参数
    /// * `document` - 本次 OCR 全文。
    /// # 返回
    /// 固定字段集合；无法预填的字段显式为 null，并保留冲突提示。
    /// # 错误
    /// 无；原始提取证据保持不变。
    pub fn draft(&self, document: &OcrDocument) -> RecognitionDraft {
        let mut warnings = self.conflicts.clone();
        let values = ContractValues::from(self);
        let fields = FIELDS
            .into_iter()
            .map(|key| {
                let value = self.fields.get(&key).and_then(|field| {
                    let name = serde_json::to_value(key).ok()?.as_str()?.to_owned();
                    if self.conflicts.iter().any(|conflict| conflict.contains(&name)) {
                        return None;
                    }
                    if field.value.trim().is_empty()
                        || field.quote.trim().is_empty()
                        || !document
                            .pages
                            .iter()
                            .any(|page| page.number == field.page && page.text.contains(&field.quote))
                    {
                        warnings.push(format!("{name} 的原文依据不可用，请核对后填写"));
                        return None;
                    }
                    values.suggestion(key)
                });
                (key, value)
            })
            .collect();
        RecognitionDraft { fields, warnings }
    }
}

impl ContractValues {
    fn suggestion(&self, key: ContractField) -> Option<String> {
        let raw = self.fields.get(&key)?.trim();
        match key {
            SignedAt | ValidFrom => self.date(key).ok().map(|date| date.to_string()),
            ValidTo if raw != "长期" => self.date(key).ok().map(|date| date.to_string()),
            InvoiceType => match raw {
                "专票" | "增值税专票" | "增值税专用发票" => Some("增值税专用发票".into()),
                "普票" | "增值税普票" | "增值税普通发票" => Some("增值税普通发票".into()),
                "不开发票" => Some(raw.into()),
                _ => None,
            },
            TaxPoint => {
                let value = raw.trim_end_matches(['%', '％']).trim();
                ["0", "1", "3", "6", "9", "13"].contains(&value).then(|| value.into())
            },
            PaymentTerms => self.payment().ok().map(|(_, name)| name),
            _ => Some(raw.into()),
        }
    }
}

impl ConfirmImport {
    /// 校验用户确认内容；不要求用户修改值逐字出现在原文中。
    /// # 参数
    /// 无。
    /// # 返回
    /// 可归档业务值以及用于主数据查找的用户输入。
    /// # 错误
    /// 必填项、长度、日期或受控业务选项非法。
    pub fn validate(&self) -> Result<(ValidatedFields, ContractValues), ImportFailure> {
        if self.version == 0 || self.fields.values().flatten().any(|value| value.len() > 4096) {
            return Err(ImportFailure::new("INVALID_CONFIRMATION", "确认内容无效，请检查字段或刷新重试"));
        }
        let values = ContractValues {
            fields: self
                .fields
                .iter()
                .filter_map(|(key, value)| {
                    let value = value.as_ref()?.trim();
                    (!value.is_empty()).then(|| (*key, value.into()))
                })
                .collect(),
        };
        Ok((values.validate_values()?, values))
    }
}
