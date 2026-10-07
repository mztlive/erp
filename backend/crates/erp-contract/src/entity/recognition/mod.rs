//! 合同识别的供应商无关事实、原文依据与严格校验。
mod pdf;
mod rules;
pub use pdf::pdf_page_count;
mod task;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

pub use rules::{ValidatedFields, match_identity};
use serde::{Deserialize, Serialize};
pub use task::{ContractImport, ImportCommand, ImportSource, ImportStatus, ImportView, RevisionTarget};

/// 所有业务字段只能来自原文，供应商不得返回 ERP 主键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractField {
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

/// 保留页码和原文片段，value 必须是 quote 的原文子串。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractedField {
    pub value: String,
    pub page: u32,
    pub quote: String,
}

/// 页号从 1 起连续，空白页也必须由 OCR 明确报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrPage {
    pub number: u32,
    pub text: String,
    pub blank: bool,
    pub readable: bool,
}

/// 识别结果冻结供应商与模型版本，禁止缺页和无界文本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrDocument {
    pub provider: String,
    pub version: String,
    pub pages: Vec<OcrPage>,
}

/// AI 的输出不代表通过，必须再经原文、规则和主数据校验。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractExtraction {
    pub provider: String,
    pub version: String,
    pub fields: BTreeMap<ContractField, ExtractedField>,
    pub conflicts: Vec<String>,
}

/// 业务失败可持久化并展示；供应商原始错误不得透传。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ImportFailure {
    pub code: String,
    pub message: String,
    pub field: Option<ContractField>,
    pub page: Option<u32>,
}

impl ImportFailure {
    /// 创建不包含外部响应或凭据的失败原因。
    /// # 参数
    /// * `code` / `message` - 稳定代码及可操作提示。
    /// # 返回
    /// 导入失败。
    /// # 错误
    /// 无。
    pub fn new(code: &str, message: &str) -> Self {
        Self { code: code.into(), message: message.into(), field: None, page: None }
    }
}

/// 精确匹配后冻结的主数据版本；变更后须重新匹配。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchedIdentity {
    pub id: String,
    pub version: u64,
    pub revision_id: Option<String>,
    pub legal_name: String,
    pub credit_code: Option<String>,
}

/// 归档证据持久化在不可变合同修订中。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecognitionProof {
    pub import_id: String,
    pub source_sha256: String,
    pub customer_id: String,
    pub customer_version: u64,
    pub customer_party: MatchedIdentity,
    pub company: MatchedIdentity,
    pub settlement: MatchedIdentity,
    pub extraction: ContractExtraction,
}
