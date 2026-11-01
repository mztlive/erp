//! 采购变更单视图。

use erp_core::money::Quantity;

use super::*;

/// 采购变更单视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeOrderView {
    /// 变更单主键。
    pub id: String,
    /// 原采购单。
    pub purchase_order_id: String,
    /// 基准版本。
    pub base_revision_id: String,
    /// 变更原因。
    pub reason: String,
    /// 状态。
    pub status: String,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 生效后形成的新采购版本。
    pub effective_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
    /// 在途改单按当前提交内容预判的销售覆盖变化；列表行与终态为空。
    pub coverage_effects: Vec<PurchaseChangeCoverageEffectView>,
}

/// 采购改单一行数量变化对销售覆盖的影响（审批前预判，生效时按当时事实复核）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeCoverageEffectView {
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 采购行号。
    pub line_no: u32,
    /// 商品名称。
    pub item_name: String,
    /// 采购当前生效数量。
    pub base_quantity: Quantity,
    /// 改后数量。
    pub target_quantity: Quantity,
    /// 销售当前目标数量。
    pub sales_quantity: Quantity,
    /// 其他来源覆盖数量。
    pub other_covered_quantity: Quantity,
    /// 改后销售行仍待覆盖数量；生效后进入供给分配。
    pub gap_quantity: Quantity,
    /// 改后销售行超出目标数量。
    pub excess_quantity: Quantity,
}
