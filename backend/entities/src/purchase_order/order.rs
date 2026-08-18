//! `purchase_order` 采购主表（数据模型 §6.6）与 §7.4 固定状态机。

use serde::{Deserialize, Serialize};

use entity_core::BaseModel;
use entity_macros::Entity;

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::errors::{Error, Result};
use crate::ids::{PurchaseOrderId, SalesOrderId, SupplierAccountId};
use crate::purchase_order::types::{FulfillmentResponsibility, PurchaseType};
use crate::validation::normalize_required_text;

/// 采购单号最大长度。
const PURCHASE_NO_MAX_LEN: usize = 64;
/// 付款条件代码最大长度。
const PAYMENT_TERM_MAX_LEN: usize = 64;

/// 采购单状态（数据模型 §6.6/§7.4）。
///
/// 状态机（§7.4）：`DRAFT → PENDING_FINANCE_REVIEW → EFFECTIVE →
/// PARTIALLY_EXECUTED → COMPLETED`；财务驳回返回 `DRAFT`；草稿且无下游事实可作废
/// （`DRAFT → VOIDED`）；`COMPLETED` / `VOIDED` 是终态。生效后变化走采购变更单，
/// 不允许直接编辑或退回。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PurchaseOrderStatus {
    /// 草稿。
    Draft,
    /// 待财务审核。
    #[serde(rename = "PENDING_FINANCE_REVIEW")]
    PendingFinanceReview,
    /// 已生效。
    Effective,
    /// 部分执行。
    #[serde(rename = "PARTIALLY_EXECUTED")]
    PartiallyExecuted,
    /// 已完成。
    Completed,
    /// 已作废。
    Voided,
    /// 审批中。
    #[serde(rename = "IN_APPROVAL")]
    InApproval,
}

impl PurchaseOrderStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingFinanceReview => "待财务审核",
            Self::Effective => "已生效",
            Self::PartiallyExecuted => "部分执行",
            Self::Completed => "已完成",
            Self::Voided => "已作废",
            Self::InApproval => "审批中",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::PendingFinanceReview => "PENDING_FINANCE_REVIEW",
            Self::Effective => "EFFECTIVE",
            Self::PartiallyExecuted => "PARTIALLY_EXECUTED",
            Self::Completed => "COMPLETED",
            Self::Voided => "VOIDED",
            Self::InApproval => "IN_APPROVAL",
        }
    }
}

impl DocumentState for PurchaseOrderStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::PendingFinanceReview, Self::Voided, Self::InApproval],
            Self::InApproval => &[Self::Effective, Self::Draft],
            Self::PendingFinanceReview => &[Self::Effective, Self::Draft],
            Self::Effective => &[Self::PartiallyExecuted],
            Self::PartiallyExecuted => &[Self::Completed],
            Self::Completed => &[],
            Self::Voided => &[],
        }
    }
}

/// 采购财务审核状态（§6.6：待审核、通过、驳回；独立于主状态审核轨）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PurchaseReviewStatus {
    /// 待审核。
    Pending,
    /// 通过。
    Approved,
    /// 驳回。
    Rejected,
}

impl PurchaseReviewStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待审核",
            Self::Approved => "通过",
            Self::Rejected => "驳回",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
        }
    }
}

/// 独立进度（§6.6 付款/收票/履约三条独立进度：未开始、部分、已完成）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProgressStatus {
    /// 未开始。
    None,
    /// 部分完成。
    Partial,
    /// 已完成。
    Completed,
}

impl ProgressStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "未开始",
            Self::Partial => "部分",
            Self::Completed => "已完成",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Partial => "PARTIAL",
            Self::Completed => "COMPLETED",
        }
    }
}

/// 采购单创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseOrderData {
    /// 采购单号。
    pub purchase_no: String,
    /// 来源实物及服务销售单。
    pub sales_order_id: SalesOrderId,
    /// 唯一供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件。
    pub payment_term_code: String,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
}

/// 采购单更新数据。
///
/// 只允许在草稿状态编辑内容（§7.4：生效后变化走采购变更单）；
/// 关键字段 `purchase_no`、`sales_order_id`、`supplier_id` 创建后不可修改。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PurchaseOrderUpdate {
    /// 付款条件；`None` 表示不修改。
    pub payment_term_code: Option<String>,
    /// 采购类型；`None` 表示不修改。
    pub purchase_type: Option<PurchaseType>,
    /// 履约责任；`None` 表示不修改。
    pub fulfillment_responsibility: Option<FulfillmentResponsibility>,
}

/// 采购单实体（稳定主表/可编辑草稿，数据模型 §6.6）。
///
/// `StableBase` 未派生 `PartialEq`，因此本实体手工实现全字段语义相等。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct PurchaseOrder {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<PurchaseOrderStatus>,
    /// 采购单号（唯一，创建后不可修改）。
    pub purchase_no: String,
    /// 来源实物及服务销售单。
    pub sales_order_id: SalesOrderId,
    /// 唯一供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件。
    pub payment_term_code: String,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 财务审核状态。
    pub review_status: PurchaseReviewStatus,
    /// 付款进度。
    pub payment_progress: ProgressStatus,
    /// 收票进度。
    pub invoice_progress: ProgressStatus,
    /// 履约进度。
    pub fulfillment_progress: ProgressStatus,
    /// 当前待财务审核的不可变提交。
    pub current_submission_id: Option<String>,
    /// 审批提交版本，初值 0。
    #[serde(default)]
    pub approval_subject_version: u32,
}

impl PartialEq for PurchaseOrder {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.purchase_no == other.purchase_no
            && self.sales_order_id == other.sales_order_id
            && self.supplier_id == other.supplier_id
            && self.purchase_type == other.purchase_type
            && self.payment_term_code == other.payment_term_code
            && self.fulfillment_responsibility == other.fulfillment_responsibility
            && self.review_status == other.review_status
            && self.payment_progress == other.payment_progress
            && self.invoice_progress == other.invoice_progress
            && self.fulfillment_progress == other.fulfillment_progress
            && self.current_submission_id == other.current_submission_id
            && self.approval_subject_version == other.approval_subject_version
    }
}

impl Eq for PurchaseOrder {}

impl PurchaseOrder {
    /// 创建采购单。
    ///
    /// 完成 `purchase_no`、`payment_term_code` 的完整校验与规范化；
    /// 初始主状态为 `Draft`，审核状态为 `Pending`，三条进度为 `None`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseOrderId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的采购单实体。
    ///
    /// # 错误
    /// 采购单号/付款条件为空或超长时返回错误。
    pub fn new(id: PurchaseOrderId, data: PurchaseOrderData, created_by: impl Into<String>) -> Result<Self> {
        let purchase_no = normalize_required_text(
            data.purchase_no,
            "采购单号不能为空",
            PURCHASE_NO_MAX_LEN,
            "采购单号过长",
        )?;
        let payment_term_code = normalize_required_text(
            data.payment_term_code,
            "付款条件不能为空",
            PAYMENT_TERM_MAX_LEN,
            "付款条件过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(PurchaseOrderStatus::Draft, created_by),
            purchase_no,
            sales_order_id: data.sales_order_id,
            supplier_id: data.supplier_id,
            purchase_type: data.purchase_type,
            payment_term_code,
            fulfillment_responsibility: data.fulfillment_responsibility,
            review_status: PurchaseReviewStatus::Pending,
            payment_progress: ProgressStatus::None,
            invoice_progress: ProgressStatus::None,
            fulfillment_progress: ProgressStatus::None,
            current_submission_id: None,
            approval_subject_version: 0,
        })
    }

    /// 更新采购单内容。
    ///
    /// 只允许在 `Draft` 状态执行（§7.4：生效后变化走采购变更单）；
    /// `purchase_no`/`sales_order_id`/`supplier_id` 是拆单关键字段，不允许修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是草稿，或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: PurchaseOrderUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.ensure_draft()?;
        self.apply_payment_term(update.payment_term_code)?;
        if let Some(purchase_type) = update.purchase_type {
            self.purchase_type = purchase_type;
        }
        if let Some(responsibility) = update.fulfillment_responsibility {
            self.fulfillment_responsibility = responsibility;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 提交财务审核。
    ///
    /// 把草稿提交为待审核，并把审核状态置为待审核；提交后头行冻结（§6.6）。
    /// 提交内容形成不可变 `purchase_order_submission` 后由 P3 调用。
    ///
    /// # 参数
    /// * `current_submission_id` - 形成的不可变提交 ID
    /// * `updated_by` - 提交执行人
    ///
    /// # 返回
    /// 提交成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是草稿时返回错误。
    pub fn submit_for_review(
        &mut self,
        current_submission_id: impl Into<String>,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        self.ensure_draft()?;
        self.current_submission_id = Some(current_submission_id.into());
        self.review_status = PurchaseReviewStatus::Pending;
        self.stable.status = PurchaseOrderStatus::PendingFinanceReview;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 执行财务审核结论（§7.4 分支）。
    ///
    /// 只能对 `PendingFinanceReview` 状态执行：通过 → `Effective` + 审核通过；
    /// 驳回 → 回到 `Draft` + 审核驳回（保留驳回动作，返回采购修改）。
    ///
    /// # 参数
    /// * `approved` - 是否审核通过
    /// * `updated_by` - 审核执行人
    ///
    /// # 返回
    /// 审核成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是待财务审核时返回错误。
    pub fn apply_finance_review(&mut self, approved: bool, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status != PurchaseOrderStatus::PendingFinanceReview {
            return Err(Error::from("只有待财务审核的采购单才能执行财务审核"));
        }
        self.review_status = if approved {
            PurchaseReviewStatus::Approved
        } else {
            PurchaseReviewStatus::Rejected
        };
        let to = if approved {
            PurchaseOrderStatus::Effective
        } else {
            PurchaseOrderStatus::Draft
        };
        self.stable.status = to;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 推进主状态（§7.4 固定状态机）。
    ///
    /// # 参数
    /// * `to` - 目标状态
    /// * `updated_by` - 本次操作执行人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标状态不在邻接矩阵中（含终态）时返回 [`Error::InvalidStateTransition`]。
    pub fn transition(&mut self, to: PurchaseOrderStatus, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.stable.status, to)?;
        self.stable.status = to;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 更新付款进度。
    ///
    /// # 参数
    /// * `progress` - 最新付款进度
    /// * `updated_by` - 本次操作执行人
    ///
    /// # 返回
    /// 无返回值；进度是下游付款事实的投影，由 P3 维护。
    pub fn set_payment_progress(&mut self, progress: ProgressStatus, updated_by: impl Into<String>) {
        self.payment_progress = progress;
        self.stable.touch(updated_by);
    }

    /// 更新收票进度。
    ///
    /// # 参数
    /// * `progress` - 最新收票进度
    /// * `updated_by` - 本次操作执行人
    ///
    /// # 返回
    /// 无返回值；进度是下游发票事实的投影，由 P3 维护。
    pub fn set_invoice_progress(&mut self, progress: ProgressStatus, updated_by: impl Into<String>) {
        self.invoice_progress = progress;
        self.stable.touch(updated_by);
    }

    /// 更新履约进度。
    ///
    /// # 参数
    /// * `progress` - 最新履约进度
    /// * `updated_by` - 本次操作执行人
    ///
    /// # 返回
    /// 无返回值；进度是入库/直发/电子交付/服务履约的投影，由 P3 维护。
    pub fn set_fulfillment_progress(&mut self, progress: ProgressStatus, updated_by: impl Into<String>) {
        self.fulfillment_progress = progress;
        self.stable.touch(updated_by);
    }

    /// 校验当前状态为草稿。
    ///
    /// # 返回
    /// 草稿状态返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿状态时返回错误。
    fn ensure_draft(&self) -> Result<()> {
        if self.stable.status != PurchaseOrderStatus::Draft {
            return Err(Error::from("只有草稿状态的采购单可以编辑"));
        }
        Ok(())
    }

    /// 应用付款条件更新。
    ///
    /// # 参数
    /// * `payment_term_code` - 可选付款条件
    ///
    /// # 错误
    /// 付款条件为空或超长时返回错误。
    fn apply_payment_term(&mut self, payment_term_code: Option<String>) -> Result<()> {
        if let Some(payment_term_code) = payment_term_code {
            self.payment_term_code = normalize_required_text(
                payment_term_code,
                "付款条件不能为空",
                PAYMENT_TERM_MAX_LEN,
                "付款条件过长",
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProgressStatus, PurchaseOrder, PurchaseOrderData, PurchaseOrderStatus, PurchaseOrderUpdate,
        PurchaseReviewStatus,
    };
    use crate::common::state::ensure_transition;
    use crate::ids::{PurchaseOrderId, SalesOrderId, SupplierAccountId};
    use crate::purchase_order::types::{FulfillmentResponsibility, PurchaseType};

    fn order_data() -> PurchaseOrderData {
        PurchaseOrderData {
            purchase_no: " PO-2026-0001 ".to_string(),
            sales_order_id: SalesOrderId::new("so-1"),
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            payment_term_code: " NET-30 ".to_string(),
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
        }
    }

    fn new_order() -> PurchaseOrder {
        PurchaseOrder::new(PurchaseOrderId::new("po-1"), order_data(), "admin-1").unwrap()
    }

    #[test]
    fn new_trims_and_initializes_defaults() {
        let order = new_order();
        assert_eq!(order.purchase_no, "PO-2026-0001");
        assert_eq!(order.payment_term_code, "NET-30");
        assert_eq!(order.stable.status(), PurchaseOrderStatus::Draft);
        assert_eq!(order.review_status, PurchaseReviewStatus::Pending);
        assert_eq!(order.payment_progress, ProgressStatus::None);
        assert!(order.current_submission_id.is_none());
    }

    #[test]
    fn new_rejects_empty_or_overlong_purchase_no() {
        let empty = PurchaseOrderData {
            purchase_no: "   ".to_string(),
            ..order_data()
        };
        assert!(PurchaseOrder::new(PurchaseOrderId::new("po-2"), empty, "admin-1").is_err());

        let overlong = PurchaseOrderData {
            purchase_no: "p".repeat(65),
            ..order_data()
        };
        assert!(PurchaseOrder::new(PurchaseOrderId::new("po-3"), overlong, "admin-1").is_err());
    }

    #[test]
    fn update_works_in_draft_and_is_rejected_after_submit() {
        let mut order = new_order();
        order
            .update(
                PurchaseOrderUpdate {
                    payment_term_code: Some(" PREPAY-30 ".to_string()),
                    purchase_type: Some(PurchaseType::Service),
                    fulfillment_responsibility: Some(FulfillmentResponsibility::Service),
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(order.payment_term_code, "PREPAY-30");
        assert_eq!(order.purchase_type, PurchaseType::Service);
        assert_eq!(order.stable.updated_by, "admin-2");

        order.submit_for_review("sub-1", "admin-2").unwrap();
        assert_eq!(order.stable.status(), PurchaseOrderStatus::PendingFinanceReview);
        assert!(order
            .update(
                PurchaseOrderUpdate {
                    payment_term_code: Some("X".to_string()),
                    ..Default::default()
                },
                "admin-3",
            )
            .is_err());
    }

    #[test]
    fn finance_review_approve_and_reject_follow_machine() {
        let mut order = new_order();
        order.submit_for_review("sub-1", "admin-1").unwrap();
        order.apply_finance_review(true, "fin-1").unwrap();
        assert_eq!(order.stable.status(), PurchaseOrderStatus::Effective);
        assert_eq!(order.review_status, PurchaseReviewStatus::Approved);

        let mut rejected = new_order();
        rejected.submit_for_review("sub-2", "admin-1").unwrap();
        rejected.apply_finance_review(false, "fin-1").unwrap();
        assert_eq!(rejected.stable.status(), PurchaseOrderStatus::Draft);
        assert_eq!(rejected.review_status, PurchaseReviewStatus::Rejected);
    }

    #[test]
    fn finance_review_rejects_wrong_state() {
        let mut order = new_order();
        assert!(
            order.apply_finance_review(true, "fin-1").is_err(),
            "草稿不能直接审核"
        );
    }

    #[test]
    fn status_machine_directional_edges() {
        // §7.4 主链与草稿作废分支：逐边定向断言（含终态，不适用对称闭包）。
        assert!(ensure_transition(PurchaseOrderStatus::Draft, PurchaseOrderStatus::Draft).is_ok());
        assert!(ensure_transition(
            PurchaseOrderStatus::Draft,
            PurchaseOrderStatus::PendingFinanceReview
        )
        .is_ok());
        assert!(ensure_transition(PurchaseOrderStatus::Draft, PurchaseOrderStatus::Voided).is_ok());
        assert!(ensure_transition(
            PurchaseOrderStatus::PendingFinanceReview,
            PurchaseOrderStatus::Effective
        )
        .is_ok());
        assert!(ensure_transition(
            PurchaseOrderStatus::PendingFinanceReview,
            PurchaseOrderStatus::Draft
        )
        .is_ok());
        assert!(ensure_transition(
            PurchaseOrderStatus::Effective,
            PurchaseOrderStatus::PartiallyExecuted
        )
        .is_ok());
        assert!(ensure_transition(
            PurchaseOrderStatus::PartiallyExecuted,
            PurchaseOrderStatus::Completed
        )
        .is_ok());

        // 非法与终态
        assert!(ensure_transition(PurchaseOrderStatus::Draft, PurchaseOrderStatus::Effective).is_err());
        assert!(ensure_transition(PurchaseOrderStatus::Effective, PurchaseOrderStatus::Draft).is_err());
        assert!(ensure_transition(PurchaseOrderStatus::Completed, PurchaseOrderStatus::Draft).is_err());
        assert!(ensure_transition(PurchaseOrderStatus::Voided, PurchaseOrderStatus::Draft).is_err());
        assert!(ensure_transition(
            PurchaseOrderStatus::Completed,
            PurchaseOrderStatus::PartiallyExecuted
        )
        .is_err());
        assert!(ensure_transition(PurchaseOrderStatus::Effective, PurchaseOrderStatus::Completed).is_err());
    }

    #[test]
    fn progress_setters_touch_auditor() {
        let mut order = new_order();
        order.set_payment_progress(ProgressStatus::Partial, "fin-1");
        order.set_invoice_progress(ProgressStatus::Partial, "fin-1");
        order.set_fulfillment_progress(ProgressStatus::Completed, "wh-1");
        assert_eq!(order.payment_progress, ProgressStatus::Partial);
        assert_eq!(order.invoice_progress, ProgressStatus::Partial);
        assert_eq!(order.fulfillment_progress, ProgressStatus::Completed);
        assert_eq!(order.stable.updated_by, "wh-1");
    }

    #[test]
    fn enums_serialize_uppercase_and_expose_labels() {
        assert_eq!(
            serde_json::to_string(&PurchaseOrderStatus::PendingFinanceReview).unwrap(),
            "\"PENDING_FINANCE_REVIEW\""
        );
        assert_eq!(
            serde_json::to_string(&PurchaseOrderStatus::PartiallyExecuted).unwrap(),
            "\"PARTIALLY_EXECUTED\""
        );
        assert_eq!(
            serde_json::to_string(&PurchaseReviewStatus::Rejected).unwrap(),
            "\"REJECTED\""
        );
        assert_eq!(PurchaseOrderStatus::PartiallyExecuted.label(), "部分执行");
        assert_eq!(PurchaseOrderStatus::Voided.label(), "已作废");
        assert_eq!(PurchaseReviewStatus::Pending.label(), "待审核");
        assert_eq!(ProgressStatus::None.label(), "未开始");
    }
}
