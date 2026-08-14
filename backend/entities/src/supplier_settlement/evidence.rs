//! W27 结算差异的不可变补证记录。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SupplierSettlementDifferenceId, SupplierSettlementStatementId};
use crate::validation::{normalize_optional_text, normalize_required_text};

const REQUEST_ID_MAX_LEN: usize = 128;
const REFERENCE_MAX_LEN: usize = 256;
const OPINION_MAX_LEN: usize = 64;
const COMMENT_MAX_LEN: usize = 1_024;
const ACTOR_MAX_LEN: usize = 128;
const HASH_LEN: usize = 64;
const MAX_REFERENCES: usize = 20;

/// 结算差异补证创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementDifferenceEvidenceData {
    pub request_id: String,
    pub statement_id: SupplierSettlementStatementId,
    pub difference_id: SupplierSettlementDifferenceId,
    pub evidence_reference_ids: Vec<String>,
    pub opinion_code: Option<String>,
    pub comment: Option<String>,
    pub provided_by: String,
    pub provided_at: Instant,
    pub command_hash: String,
}

/// 采购或协同角色追加的不可变证据与意见。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierSettlementDifferenceEvidence {
    #[serde(flatten)]
    pub base: BaseModel,
    pub request_id: String,
    pub statement_id: SupplierSettlementStatementId,
    pub difference_id: SupplierSettlementDifferenceId,
    pub evidence_reference_ids: Vec<String>,
    pub opinion_code: Option<String>,
    pub comment: Option<String>,
    pub provided_by: String,
    pub provided_at: Instant,
    pub command_hash: String,
}

impl SupplierSettlementDifferenceEvidence {
    /// 创建一条不可变补证记录。
    ///
    /// # 错误
    /// 请求身份、证据引用、意见、记录人或命令摘要非法时返回错误。
    pub fn new(id: impl Into<String>, data: SupplierSettlementDifferenceEvidenceData) -> Result<Self> {
        let request_id = normalize_required_text(
            data.request_id,
            "补证请求ID不能为空",
            REQUEST_ID_MAX_LEN,
            "补证请求ID过长",
        )?;
        if data.evidence_reference_ids.is_empty() || data.evidence_reference_ids.len() > MAX_REFERENCES {
            return Err(Error::from("补证必须包含 1-20 个证据引用"));
        }
        let mut evidence_reference_ids = data.evidence_reference_ids;
        for reference in &mut evidence_reference_ids {
            *reference = normalize_required_text(
                std::mem::take(reference),
                "证据引用不能为空",
                REFERENCE_MAX_LEN,
                "证据引用过长",
            )?;
        }
        evidence_reference_ids.sort();
        evidence_reference_ids.dedup();
        let opinion_code = normalize_optional_text(data.opinion_code, "意见代码", OPINION_MAX_LEN)?;
        let comment = normalize_optional_text(data.comment, "补证说明", COMMENT_MAX_LEN)?;
        let provided_by =
            normalize_required_text(data.provided_by, "补证人不能为空", ACTOR_MAX_LEN, "补证人过长")?;
        let command_hash = data.command_hash.trim().to_ascii_lowercase();
        if command_hash.len() != HASH_LEN || !command_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::from("补证命令摘要必须是64位SHA-256十六进制值"));
        }
        Ok(Self {
            base: BaseModel::new(id.into()),
            request_id,
            statement_id: data.statement_id,
            difference_id: data.difference_id,
            evidence_reference_ids,
            opinion_code,
            comment,
            provided_by,
            provided_at: data.provided_at,
            command_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> SupplierSettlementDifferenceEvidenceData {
        SupplierSettlementDifferenceEvidenceData {
            request_id: "evidence-1".to_string(),
            statement_id: SupplierSettlementStatementId::new("statement-1"),
            difference_id: SupplierSettlementDifferenceId::new("difference-1"),
            evidence_reference_ids: vec!["ticket://T-1".to_string()],
            opinion_code: Some("PROCUREMENT_NOTE".to_string()),
            comment: Some("供应商已确认".to_string()),
            provided_by: "buyer-1".to_string(),
            provided_at: Instant::from_unix_secs(1_700_000_000),
            command_hash: "a".repeat(64),
        }
    }

    #[test]
    fn evidence_requires_formal_reference() {
        assert!(SupplierSettlementDifferenceEvidence::new("evidence-1", data()).is_ok());
        let mut invalid = data();
        invalid.evidence_reference_ids.clear();
        assert!(SupplierSettlementDifferenceEvidence::new("evidence-2", invalid).is_err());
    }
}
