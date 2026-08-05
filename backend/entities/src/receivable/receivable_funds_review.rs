//! `receivable_funds_review` 卡券票款正式复核（数据模型 §6.8）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{FileAssetId, ReceivableAccountId, ReceivableFundsReviewId, WorkItemId};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 证据引用最大长度。
const EVIDENCE_MAX_LEN: usize = 512;
/// 复核人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 复核类型（数据模型 §6.8：`OPENING` 或 `SYNC_DELTA`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundsReviewType {
    /// 卡券期初票款复核。
    Opening,
    /// 商城同步金额差额复核。
    SyncDelta,
}

impl FundsReviewType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Opening => "卡券期初票款复核",
            Self::SyncDelta => "同步差额复核",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::SyncDelta => "sync_delta",
        }
    }
}

/// 复核结果（数据模型 §6.8：通过或驳回）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewResult {
    /// 通过。
    Passed,
    /// 驳回。
    Rejected,
}

impl ReviewResult {
    /// 返回结果的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Passed => "通过",
            Self::Rejected => "驳回",
        }
    }

    /// 返回结果的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Rejected => "rejected",
        }
    }
}

/// 卡券票款复核创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceivableFundsReviewData {
    /// 往来子账。
    pub receivable_account_id: ReceivableAccountId,
    /// 子账内递增复核号（从 1 开始）。
    pub review_no: u32,
    /// 复核类型。
    pub review_type: FundsReviewType,
    /// 对应 `CARD_FUNDS_REVIEW` 或 `CARD_FUNDS_DELTA_REVIEW` 任务。
    pub work_item_id: WorkItemId,
    /// 银行、发票或正式核对证据单据。
    pub evidence_document_id: Option<FileAssetId>,
    /// 证据引用（与证据单据至少提供其一）。
    pub evidence_reference: Option<String>,
    /// 复核结果。
    pub review_result: ReviewResult,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 复核时间。
    pub reviewed_at: Instant,
    /// 同子账被本次复核替代的上一记录（`review_no = 1` 时为空，其后必填）。
    pub supersedes_review_id: Option<ReceivableFundsReviewId>,
}

/// 卡券票款复核实体（正式事实，数据模型 §6.8）。
///
/// `(receivable_account_id, review_no)` 唯一；复核链锁定链尾逐号递增，禁止多根
/// 或分叉：`review_no = 1` 时前驱为空，`review_no > 1` 时必填前驱。历史复核
/// 不可更新或删除；账户上的复核状态仅为可重建的查询缓存。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ReceivableFundsReview {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 往来子账。
    pub receivable_account_id: ReceivableAccountId,
    /// 子账内递增复核号。
    pub review_no: u32,
    /// 复核类型。
    pub review_type: FundsReviewType,
    /// 对应复核任务。
    pub work_item_id: WorkItemId,
    /// 证据单据。
    pub evidence_document_id: Option<FileAssetId>,
    /// 证据引用。
    pub evidence_reference: Option<String>,
    /// 复核结果。
    pub review_result: ReviewResult,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 复核时间。
    pub reviewed_at: Instant,
    /// 被替代的上一记录。
    pub supersedes_review_id: Option<ReceivableFundsReviewId>,
}

impl ReceivableFundsReview {
    /// 创建卡券票款复核记录。
    ///
    /// 完成复核号从 1 开始、链尾锁定（`review_no = 1` 前驱为空，其后必填）与
    /// 证据非空校验（证据单据与证据引用至少其一）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ReceivableFundsReviewId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的复核实体。
    ///
    /// # 错误
    /// 当复核号/证据不合法或复核链断裂（序号与前驱不匹配）时返回错误。
    pub fn new(id: ReceivableFundsReviewId, data: ReceivableFundsReviewData) -> Result<Self> {
        if data.review_no == 0 {
            return Err(Error::from("复核号必须从 1 开始"));
        }
        if data.review_no == 1 && data.supersedes_review_id.is_some() {
            return Err(Error::from("首条复核不得引用前驱"));
        }
        if data.review_no > 1 && data.supersedes_review_id.is_none() {
            return Err(Error::from("后续复核必须锁定链尾并引用前驱"));
        }
        if data.evidence_document_id.is_none() && data.evidence_reference.is_none() {
            return Err(Error::from("复核证据不能为空"));
        }
        let evidence_reference =
            normalize_optional_text(data.evidence_reference, "证据引用", EVIDENCE_MAX_LEN)?;
        let reviewed_by = normalize_required_text(
            data.reviewed_by,
            "复核人不能为空",
            ACTOR_MAX_LEN,
            "复核人标识过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            receivable_account_id: data.receivable_account_id,
            review_no: data.review_no,
            review_type: data.review_type,
            work_item_id: data.work_item_id,
            evidence_document_id: data.evidence_document_id,
            evidence_reference,
            review_result: data.review_result,
            reviewed_by,
            reviewed_at: data.reviewed_at,
            supersedes_review_id: data.supersedes_review_id,
        })
    }

    /// 更新复核记录。
    ///
    /// 历史复核不可更新或删除（数据模型 §6.8「历史复核不可更新或删除」），
    /// 任何修改都被拒绝。
    ///
    /// # 参数
    /// * `update` - 更新数据（恒被拒绝）
    /// * `updated_by` - 更新执行人（恒被拒绝）
    ///
    /// # 返回
    /// 恒返回错误。
    pub fn update(&mut self, update: ReceivableFundsReviewData, updated_by: impl Into<String>) -> Result<()> {
        let _ = (update, updated_by);
        Err(Error::from("历史复核不可更新或删除"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(review_no: u32, supersedes: Option<ReceivableFundsReviewId>) -> ReceivableFundsReviewData {
        ReceivableFundsReviewData {
            receivable_account_id: ReceivableAccountId::new("ra-1"),
            review_no,
            review_type: FundsReviewType::Opening,
            work_item_id: WorkItemId::new("wi-1"),
            evidence_document_id: None,
            evidence_reference: Some(" bank-evidence-1 ".to_string()),
            review_result: ReviewResult::Passed,
            reviewed_by: " reviewer-1 ".to_string(),
            reviewed_at: Instant::from_unix_secs(1_700_000_000),
            supersedes_review_id: supersedes,
        }
    }

    #[test]
    fn new_trims_and_links_chain_head() {
        let review = ReceivableFundsReview::new(ReceivableFundsReviewId::new("fr-1"), data(1, None)).unwrap();
        assert_eq!(review.review_no, 1);
        assert_eq!(review.reviewed_by, "reviewer-1");
        assert_eq!(review.evidence_reference.as_deref(), Some("bank-evidence-1"));
        assert!(review.supersedes_review_id.is_none());
    }

    #[test]
    fn new_enforces_review_chain_locking() {
        let orphan_chain = data(2, None);
        assert!(ReceivableFundsReview::new(ReceivableFundsReviewId::new("fr-2"), orphan_chain).is_err());

        let head_with_predecessor = data(1, Some(ReceivableFundsReviewId::new("fr-0")));
        assert!(
            ReceivableFundsReview::new(ReceivableFundsReviewId::new("fr-3"), head_with_predecessor).is_err()
        );

        let zero_no = data(0, None);
        assert!(ReceivableFundsReview::new(ReceivableFundsReviewId::new("fr-4"), zero_no).is_err());
    }

    #[test]
    fn new_requires_evidence() {
        let no_evidence = ReceivableFundsReviewData {
            evidence_document_id: None,
            evidence_reference: None,
            ..data(1, None)
        };
        assert!(ReceivableFundsReview::new(ReceivableFundsReviewId::new("fr-5"), no_evidence).is_err());
    }

    #[test]
    fn formal_fact_rejects_update() {
        let mut review =
            ReceivableFundsReview::new(ReceivableFundsReviewId::new("fr-1"), data(1, None)).unwrap();
        assert!(review
            .update(data(2, Some(ReceivableFundsReviewId::new("fr-1"))), "admin-2")
            .is_err());
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&FundsReviewType::SyncDelta).unwrap(),
            "\"sync_delta\""
        );
        assert_eq!(
            serde_json::to_string(&ReviewResult::Rejected).unwrap(),
            "\"rejected\""
        );
        assert_eq!(FundsReviewType::Opening.label(), "卡券期初票款复核");
        assert_eq!(ReviewResult::Passed.label(), "通过");
    }
}
