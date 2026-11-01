//! 供应链各命令的强类型原结果；回执不恢复展示文案。

use erp_core::money::Amount;
use serde::{Deserialize, Serialize};

use crate::dto::supplier_fulfillment::SupplierOrderResolution;
use crate::dto::supplier_settlement::SettlementReviewDecisionStatus;

/// W26 调查原结果引用及版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationReceipt {
    pub evidence_id: String,
    pub order_version: u64,
    pub task_version: Option<u64>,
}

/// W26 完成原结果引用及终态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionReceipt {
    pub terminal_action_id: String,
    pub order_version: u64,
    pub task_version: u64,
    pub resolution: SupplierOrderResolution,
}

/// 结算刷新原请求、来源快照和明细计数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshReceipt {
    pub request_id: String,
    pub statement_version: u64,
    pub source_snapshot_hash: String,
    pub item_count: usize,
    pub difference_count: usize,
}

impl RefreshReceipt {
    /// 构造刷新命令的原结果并一次登记版本与计数。
    ///
    /// # 参数
    /// * `request_id` - 原请求编号。
    /// * `source_snapshot_hash` - 不可变来源快照摘要。
    /// # 返回
    /// 返回待填充版本与计数的结果。
    /// # 错误
    /// 无；持久化边界统一校验。
    pub fn new(request_id: String, source_snapshot_hash: String) -> Self {
        Self { request_id, statement_version: 1, source_snapshot_hash, item_count: 0, difference_count: 0 }
    }

    /// 登记实际结算版本及明细、差异计数。
    ///
    /// # 参数
    /// * `statement_version` - 本次结果版本。
    /// * `item_count` - 结算项数量。
    /// * `difference_count` - 差异数量。
    /// # 返回
    /// 返回完整结果。
    /// # 错误
    /// 无；持久化边界统一校验。
    pub fn with_counts(mut self, statement_version: u64, item_count: usize, difference_count: usize) -> Self {
        self.statement_version = statement_version;
        self.item_count = item_count;
        self.difference_count = difference_count;
        self
    }
}

/// 结算差异决定结果及原版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DifferenceDecisionReceipt {
    pub operation_id: String,
    pub statement_id: String,
    pub statement_version: u64,
    pub difference_version: u64,
}

/// 结算复核提交的冻结结果及原任务引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSubmissionReceipt {
    pub operation_id: String,
    pub statement_version: u64,
    pub work_item_id: String,
    pub task_version: u64,
}

/// 结算复核决定的原终态、版本、应付引用与精确成本差额。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewDecisionReceipt {
    pub operation_id: String,
    pub result_status: SettlementReviewDecisionStatus,
    pub statement_version: u64,
    pub task_version: u64,
    pub payable_account_id: Option<String>,
    pub cost_delta: Option<Amount>,
}

/// 供应停止任务的不可变核对来源与决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplyExceptionReceipt {
    pub work_item_id: String,
    pub offering_id: String,
    pub subject_version: String,
    pub task_version: u64,
    pub evidence_reference: String,
    pub comment: String,
}

/// 本域命令结果目录；当前视图型入口仍由领域事实恢复当前视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum SupplyCommandResult {
    Investigation(InvestigationReceipt),
    Completion(CompletionReceipt),
    Refresh(RefreshReceipt),
    DifferenceDecision(DifferenceDecisionReceipt),
    ReviewSubmission(ReviewSubmissionReceipt),
    ReviewDecision(ReviewDecisionReceipt),
    SupplyException(SupplyExceptionReceipt),
    FulfillmentHandover,
    SettlementHandover,
    DifferenceHandlerReassigned,
    CapabilitiesUpdated,
}

impl SupplyCommandResult {
    /// 动作目录显式限定每一种结果及其领域对象类型。
    ///
    /// # 参数
    /// * `action` - 命令动作代码。
    /// * `resource_type` - 命令目标类型。
    /// # 返回
    /// 动作、目标类型与结果目录完全一致时返回真。
    /// # 错误
    /// 无；不合法组合返回假。
    pub(super) fn matches_action(&self, action: &str, resource_type: &str) -> bool {
        let (resource, matched) = match self {
            Self::Investigation(_) => (
                "SUPPLIER_FULFILLMENT_ORDER",
                matches!(
                    action,
                    "supplier_fulfillment.investigate" | "supplier_fulfillment.task_investigate"
                ),
            ),
            Self::Completion(_) => {
                ("SUPPLIER_FULFILLMENT_ORDER", action == "supplier_fulfillment.task_complete")
            },
            Self::Refresh(_) => ("supplier_settlement_statement", action == "supplier_settlement.refresh"),
            Self::DifferenceDecision(_) => {
                ("supplier_settlement_difference", action == "supplier_settlement.difference_decision")
            },
            Self::ReviewSubmission(_) => {
                ("supplier_settlement_statement", action == "supplier_settlement.submit_review")
            },
            Self::ReviewDecision(_) => (
                "supplier_settlement_statement",
                matches!(action, "supplier_settlement.review_confirm" | "supplier_settlement.review_reject"),
            ),
            Self::SupplyException(_) => {
                ("work_item", action == "supplier_offering.supply_exception.complete")
            },
            Self::FulfillmentHandover => {
                ("supplier_fulfillment_order", action == "supplier_fulfillment.handover")
            },
            Self::SettlementHandover => {
                ("supplier_settlement_statement", action == "supplier_settlement.handover")
            },
            Self::DifferenceHandlerReassigned => {
                ("supplier_settlement_statement", action == "supplier_settlement.reassign_difference_handler")
            },
            Self::CapabilitiesUpdated => {
                ("supplier_api_connection", action == "supplier_api_capability.update")
            },
        };
        matched && resource == resource_type
    }

    /// 原结果引用和版本缺失时失败关闭，不当作未执行命令。
    ///
    /// # 参数
    /// * `resource_id` - 原提交目标编号。
    /// * `action` - 原命令动作代码。
    /// # 返回
    /// 原结果引用、版本及终态合法时返回真。
    /// # 错误
    /// 无；不完整结果返回假。
    pub(super) fn valid_payload(&self, resource_id: &str, action: &str) -> bool {
        match self {
            Self::Investigation(value) => {
                has_text(&value.evidence_id)
                    && value.order_version > 0
                    && value.task_version != Some(0)
                    && (action != "supplier_fulfillment.task_investigate" || value.task_version.is_some())
            },
            Self::Completion(value) => {
                has_text(&value.terminal_action_id) && positive_pair(value.order_version, value.task_version)
            },
            Self::Refresh(value) => {
                has_text(&value.request_id)
                    && value.statement_version > 0
                    && super::digest_is_valid(&value.source_snapshot_hash)
            },
            Self::DifferenceDecision(value) => {
                has_text(&value.operation_id)
                    && has_text(&value.statement_id)
                    && positive_pair(value.statement_version, value.difference_version)
            },
            Self::ReviewSubmission(value) => {
                has_text(&value.operation_id)
                    && has_text(&value.work_item_id)
                    && positive_pair(value.statement_version, value.task_version)
            },
            Self::ReviewDecision(value) => value.valid_for(action),
            Self::SupplyException(value) => {
                value.work_item_id == resource_id
                    && has_text(&value.offering_id)
                    && has_text(&value.subject_version)
                    && value.task_version > 0
                    && has_text(&value.evidence_reference)
                    && has_text(&value.comment)
            },
            _ => true,
        }
    }
}

impl ReviewDecisionReceipt {
    /// 正式复核结果必须与原动作、金额结果和应付引用一致。
    fn valid_for(&self, action: &str) -> bool {
        if !has_text(&self.operation_id)
            || !positive_pair(self.statement_version, self.task_version)
            || self.payable_account_id.as_ref().is_some_and(|id| !has_text(id))
        {
            return false;
        }
        match self.result_status {
            SettlementReviewDecisionStatus::Confirmed => action == "supplier_settlement.review_confirm",
            SettlementReviewDecisionStatus::Rejected => {
                action == "supplier_settlement.review_reject"
                    && self.payable_account_id.is_none()
                    && self.cost_delta.is_none()
            },
        }
    }
}

/// 结果身份拒绝空白值。
fn has_text(value: &str) -> bool {
    !value.trim().is_empty()
}
/// 所有持久化原版本必须为正值。
fn positive_pair(left: u64, right: u64) -> bool {
    left > 0 && right > 0
}
