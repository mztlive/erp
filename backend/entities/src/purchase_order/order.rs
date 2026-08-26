//! `purchase_order` 采购主表（数据模型 §6.6）与 §7.4 固定状态机。

use serde::{Deserialize, Serialize};

use entity_core::BaseModel;
use entity_macros::Entity;

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::errors::{Error, Result};
use crate::ids::{
    PurchaseOrderId, PurchaseOrderRevisionId, PurchaseOrderSubmissionId, SalesOrderId, SalesOrderRevisionId,
    SupplierAccountId, WarehouseId,
};
use crate::purchase_order::types::{FulfillmentResponsibility, PurchaseType};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 采购单号最大长度。
const PURCHASE_NO_MAX_LEN: usize = 64;
/// 付款条件代码最大长度。
const PAYMENT_TERM_MAX_LEN: usize = 64;
/// 精确采购创建依据最大长度。
const CREATION_BASIS_ID_MAX_LEN: usize = 192;
/// 采购单当前责任人账号 ID 最大长度。
const OWNER_USER_ID_MAX_LEN: usize = 128;

/// 采购单状态（数据模型 §6.6/§7.4，审批合同 §4.4.2 已收敛）。
///
/// 目标状态机：`DRAFT → IN_APPROVAL → EFFECTIVE → PARTIALLY_EXECUTED →
/// COMPLETED`；撤回回到 `DRAFT`；草稿且无下游事实可作废（`DRAFT → VOIDED`）。
/// `PENDING_FINANCE_REVIEW` 仅为旧数据反序列化保留，新写入不得进入。
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

    /// 判断当前状态是否允许发起采购变更。
    ///
    /// # 返回
    /// 已生效或部分执行时返回 `true`，其余状态返回 `false`。
    pub fn allows_change(self) -> bool {
        matches!(self, Self::Effective | Self::PartiallyExecuted)
    }
}

impl DocumentState for PurchaseOrderStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::InApproval, Self::Voided],
            Self::InApproval => &[Self::Effective, Self::Draft],
            Self::PendingFinanceReview => &[],
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
    /// 建单时的销售当前版本。
    pub sales_order_revision_id: SalesOrderRevisionId,
    /// 精确创建依据；一条依据只允许创建一张采购单。
    pub creation_basis_id: String,
    /// 唯一供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件。
    pub payment_term_code: String,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 采购单当前责任人；创建时通常继承供给分配任务责任人。
    pub owner_user_id: String,
    /// 入仓采购的目标仓库；其它履约责任必须为空。
    pub target_warehouse_id: Option<WarehouseId>,
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
    /// 建单时的销售当前版本。
    pub sales_order_revision_id: SalesOrderRevisionId,
    /// 精确创建依据；一条依据只允许创建一张采购单。
    pub creation_basis_id: String,
    /// 唯一供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件。
    pub payment_term_code: String,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 采购单当前责任人；旧数据为空时执行失败关闭，不得回退创建人。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    /// 入仓采购目标仓库；兼容旧数据缺少该字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_warehouse_id: Option<WarehouseId>,
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
            && self.sales_order_revision_id == other.sales_order_revision_id
            && self.creation_basis_id == other.creation_basis_id
            && self.supplier_id == other.supplier_id
            && self.purchase_type == other.purchase_type
            && self.payment_term_code == other.payment_term_code
            && self.fulfillment_responsibility == other.fulfillment_responsibility
            && self.owner_user_id == other.owner_user_id
            && self.target_warehouse_id == other.target_warehouse_id
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
    /// 草稿允许空 `purchase_no`；付款条件必须非空。初始主状态为 `Draft`，
    /// `approval_subject_version` 为 0。不得预分配正式号。
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
    /// 采购单号超长或付款条件为空/超长时返回错误。
    pub fn new(id: PurchaseOrderId, data: PurchaseOrderData, created_by: impl Into<String>) -> Result<Self> {
        let purchase_no = normalize_optional_text(Some(data.purchase_no), "采购单号", PURCHASE_NO_MAX_LEN)?
            .unwrap_or_default();
        let payment_term_code = normalize_required_text(
            data.payment_term_code,
            "付款条件不能为空",
            PAYMENT_TERM_MAX_LEN,
            "付款条件过长",
        )?;
        let creation_basis_id = normalize_required_text(
            data.creation_basis_id,
            "采购创建依据不能为空",
            CREATION_BASIS_ID_MAX_LEN,
            "采购创建依据过长",
        )?;
        let owner_user_id = normalize_required_text(
            data.owner_user_id,
            "采购单责任人不能为空",
            OWNER_USER_ID_MAX_LEN,
            "采购单责任人过长",
        )?;
        ensure_target_warehouse(data.fulfillment_responsibility, data.target_warehouse_id.as_ref())?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(PurchaseOrderStatus::Draft, created_by),
            purchase_no,
            sales_order_id: data.sales_order_id,
            sales_order_revision_id: data.sales_order_revision_id,
            creation_basis_id,
            supplier_id: data.supplier_id,
            purchase_type: data.purchase_type,
            payment_term_code,
            fulfillment_responsibility: data.fulfillment_responsibility,
            owner_user_id: Some(owner_user_id),
            target_warehouse_id: data.target_warehouse_id,
            review_status: PurchaseReviewStatus::Pending,
            payment_progress: ProgressStatus::None,
            invoice_progress: ProgressStatus::None,
            fulfillment_progress: ProgressStatus::None,
            current_submission_id: None,
            approval_subject_version: 0,
        })
    }

    /// 校验调用方持有的乐观锁版本。
    ///
    /// # 参数
    /// * `expected` - 调用方读取到的期望版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 期望版本与实体当前版本不一致时返回领域错误。
    pub fn ensure_expected_version(&self, expected: u64) -> Result<()> {
        if self.base.version != expected {
            return Err(Error::from("采购单版本已变化"));
        }
        Ok(())
    }

    /// 校验采购单仍可冻结新的审批提交。
    ///
    /// # 返回
    /// 草稿状态返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿状态返回领域错误。
    pub fn ensure_draft_for_submission(&self) -> Result<()> {
        self.ensure_draft()
    }

    /// 取得可提交审批的当前草稿提交。
    ///
    /// # 返回
    /// 采购单处于草稿且已挂接草稿提交时，返回类型化提交 ID。
    ///
    /// # 错误
    /// 采购单不是草稿，或缺少当前草稿提交时返回领域错误。
    pub fn draft_submission_id(&self) -> Result<PurchaseOrderSubmissionId> {
        self.ensure_draft()?;
        self.current_submission_id
            .as_ref()
            .map(|id| PurchaseOrderSubmissionId::new(id.clone()))
            .ok_or_else(|| Error::from("采购单缺少草稿提交"))
    }

    /// 取得可正式化的冻结提交。
    ///
    /// # 返回
    /// 采购单处于审批中且已冻结提交时，返回类型化提交 ID。
    ///
    /// # 错误
    /// 采购单不是审批中，或缺少冻结提交时返回领域错误。
    pub fn submission_id_for_formalization(&self) -> Result<PurchaseOrderSubmissionId> {
        if self.stable.status != PurchaseOrderStatus::InApproval {
            return Err(Error::from("只有审批中的采购单可以正式化"));
        }
        self.current_submission_id
            .as_ref()
            .map(|id| PurchaseOrderSubmissionId::new(id.clone()))
            .ok_or_else(|| Error::from("采购单缺少待生效提交"))
    }

    /// 取得发起采购变更所需的当前生效版本。
    ///
    /// # 返回
    /// 采购单已生效或部分执行且存在当前版本时，返回类型化版本 ID。
    ///
    /// # 错误
    /// 主状态不允许变更，或缺少当前生效版本时返回领域错误。
    pub fn revision_id_for_change(&self) -> Result<PurchaseOrderRevisionId> {
        if !self.stable.status.allows_change() {
            return Err(Error::from("只有已生效或部分执行的采购单可以发起变更"));
        }
        self.stable
            .current_revision_id
            .as_ref()
            .map(|id| PurchaseOrderRevisionId::new(id.clone()))
            .ok_or_else(|| Error::from("采购单没有生效版本，不能发起变更"))
    }

    /// 挂接创建采购单时形成的首个草稿提交。
    ///
    /// # 参数
    /// * `submission_id` - 新建草稿提交稳定身份
    ///
    /// # 返回
    /// 挂接成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 采购单不是草稿，或已经挂接提交时返回领域错误。
    pub fn attach_draft_submission(&mut self, submission_id: PurchaseOrderSubmissionId) -> Result<()> {
        self.ensure_draft()?;
        if self.current_submission_id.is_some() {
            return Err(Error::from("采购单已经挂接当前提交"));
        }
        self.current_submission_id = Some(submission_id.to_string());
        Ok(())
    }

    /// 最终通过并切换当前生效版本。
    ///
    /// # 参数
    /// * `revision_id` - 本次正式化形成的采购版本
    /// * `updated_by` - 最终通过执行人
    ///
    /// # 返回
    /// 状态和版本指针更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 采购单不是审批中时返回领域错误。
    pub fn formalize_with_revision(
        &mut self,
        revision_id: PurchaseOrderRevisionId,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        self.formalize_approved(updated_by)?;
        self.stable.current_revision_id = Some(revision_id.to_string());
        Ok(())
    }

    /// 在采购变更生效后切换当前版本。
    ///
    /// # 参数
    /// * `revision_id` - 采购变更形成的新生效版本
    /// * `updated_by` - 最终通过执行人
    ///
    /// # 返回
    /// 当前版本指针更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 采购单主状态不允许发起变更时返回领域错误。
    pub fn apply_change_revision(
        &mut self,
        revision_id: PurchaseOrderRevisionId,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if !self.stable.status.allows_change() {
            return Err(Error::from("只有已生效或部分执行的采购单可以应用变更版本"));
        }
        self.stable.current_revision_id = Some(revision_id.to_string());
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 一次性分配不可复用正式采购单号。
    ///
    /// 仅允许空号草稿调用；成功后不得再改写。
    ///
    /// # 参数
    /// * `purchase_no` - 正式采购单号
    ///
    /// # 错误
    /// 已有正式号、编号为空或超长时返回错误。
    pub fn assign_purchase_no(&mut self, purchase_no: impl Into<String>) -> Result<()> {
        if !self.purchase_no.is_empty() {
            return Err(Error::from("采购单号只能分配一次"));
        }
        self.purchase_no = normalize_required_text(
            purchase_no.into(),
            "采购单号不能为空",
            PURCHASE_NO_MAX_LEN,
            "采购单号过长",
        )?;
        Ok(())
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

    /// 旧财务审核提交入口。审批改造后不可达。
    ///
    /// # 错误
    /// 恒返回冲突，不得进入 `PENDING_FINANCE_REVIEW`。
    pub fn submit_for_review(
        &mut self,
        _current_submission_id: impl Into<String>,
        _updated_by: impl Into<String>,
    ) -> Result<()> {
        Err(Error::from("采购单必须走统一审批提交，禁止写入待财务审核"))
    }

    /// 提交并启动审批：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
    ///
    /// 版本使用 checked add，成功后不回退。不得改写 `PurchaseReviewStatus`。
    ///
    /// # 参数
    /// * `current_submission_id` - 本次冻结提交
    /// * `updated_by` - 提交人
    ///
    /// # 返回
    /// 返回冻结后的提交版本。
    ///
    /// # 错误
    /// 非草稿或版本溢出时返回冲突。
    pub fn start_approval(
        &mut self,
        current_submission_id: impl Into<String>,
        updated_by: impl Into<String>,
    ) -> Result<u32> {
        self.ensure_draft()?;
        ensure_transition(self.stable.status, PurchaseOrderStatus::InApproval)?;
        let next = self
            .approval_subject_version
            .checked_add(1)
            .ok_or_else(|| Error::from("审批提交版本溢出"))?;
        self.approval_subject_version = next;
        self.current_submission_id = Some(current_submission_id.into());
        self.stable.status = PurchaseOrderStatus::InApproval;
        self.stable.touch(updated_by);
        Ok(next)
    }

    /// 撤回审批：回到草稿，且 `approval_subject_version` 不回退。
    ///
    /// # 参数
    /// * `updated_by` - 撤回人
    ///
    /// # 错误
    /// 非审批中时返回冲突。
    pub fn cancel_approval(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status != PurchaseOrderStatus::InApproval {
            return Err(Error::from("只有审批中的采购单可以撤回审批"));
        }
        ensure_transition(self.stable.status, PurchaseOrderStatus::Draft)?;
        self.stable.status = PurchaseOrderStatus::Draft;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 最终通过：仅 `IN_APPROVAL` 可进入生效。
    ///
    /// 不改写 `PurchaseReviewStatus`，审批结果取实例投影。
    ///
    /// # 参数
    /// * `updated_by` - 最终通过执行人
    ///
    /// # 错误
    /// 状态不是审批中时返回冲突。
    pub fn formalize_approved(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status != PurchaseOrderStatus::InApproval {
            return Err(Error::from("只有审批中的采购单可以由最终通过动作生效"));
        }
        ensure_transition(self.stable.status, PurchaseOrderStatus::Effective)?;
        self.stable.status = PurchaseOrderStatus::Effective;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 旧财务审核结论入口。审批改造后不可达。
    ///
    /// # 错误
    /// 恒返回冲突，不得经 `PurchaseReviewStatus` 分支。
    pub fn apply_finance_review(&mut self, _approved: bool, _updated_by: impl Into<String>) -> Result<()> {
        Err(Error::from("采购财务审核不得作为审批动作"))
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

    /// 返回采购单当前责任人。
    ///
    /// # 返回
    /// 显式责任人存在时返回账号 ID。
    ///
    /// # 错误
    /// 存量数据缺少责任人时返回错误；调用方必须先补齐责任，不得回退创建人。
    pub fn current_owner_user_id(&self) -> Result<&str> {
        self.owner_user_id
            .as_deref()
            .filter(|owner| !owner.trim().is_empty())
            .ok_or_else(|| Error::from("采购单未指定责任人，请先补齐后再继续履约"))
    }

    /// 取得入库任务必须使用的目标仓库。
    ///
    /// # 返回
    /// 入仓采购且已指定目标仓库时返回仓库 ID。
    ///
    /// # 错误
    /// 非入仓采购或旧数据缺少目标仓库时返回错误，调用方不得选择任意默认仓库。
    pub fn target_warehouse_for_receipt(&self) -> Result<&WarehouseId> {
        if self.fulfillment_responsibility != FulfillmentResponsibility::Warehouse {
            return Err(Error::from("当前采购单不属于入仓履约"));
        }
        self.target_warehouse_id
            .as_ref()
            .ok_or_else(|| Error::from("采购单未指定目标仓库，请先补齐后再生效"))
    }

    /// 变更采购单当前责任人。
    ///
    /// # 参数
    /// * `owner_user_id` - 已由应用层验证资格的具体账号
    /// * `updated_by` - 本次责任变更操作人
    ///
    /// # 返回
    /// 责任人更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标账号为空或过长时返回错误；已完成、已作废采购单禁止变更责任。
    pub fn reassign_owner(
        &mut self,
        owner_user_id: impl Into<String>,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if matches!(
            self.stable.status,
            PurchaseOrderStatus::Completed | PurchaseOrderStatus::Voided
        ) {
            return Err(Error::from("已完成或已作废采购单不能变更责任人"));
        }
        let owner_user_id = normalize_required_text(
            owner_user_id.into(),
            "采购单责任人不能为空",
            OWNER_USER_ID_MAX_LEN,
            "采购单责任人过长",
        )?;
        self.owner_user_id = Some(owner_user_id);
        self.stable.touch(updated_by);
        Ok(())
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

/// 校验履约责任与目标仓库是一一对应的事实。
fn ensure_target_warehouse(
    responsibility: FulfillmentResponsibility,
    target_warehouse_id: Option<&WarehouseId>,
) -> Result<()> {
    match (responsibility, target_warehouse_id) {
        (FulfillmentResponsibility::Warehouse, Some(_)) => Ok(()),
        (FulfillmentResponsibility::Warehouse, None) => Err(Error::from("入仓采购必须指定目标仓库")),
        (_, None) => Ok(()),
        (_, Some(_)) => Err(Error::from("非入仓采购不能指定目标仓库")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProgressStatus, PurchaseOrder, PurchaseOrderData, PurchaseOrderStatus, PurchaseOrderUpdate,
        PurchaseReviewStatus,
    };
    use crate::common::state::ensure_transition;
    use crate::ids::{
        PurchaseOrderId, PurchaseOrderRevisionId, PurchaseOrderSubmissionId, SalesOrderId,
        SalesOrderRevisionId, SupplierAccountId,
    };
    use crate::purchase_order::types::{FulfillmentResponsibility, PurchaseType};

    fn order_data() -> PurchaseOrderData {
        PurchaseOrderData {
            purchase_no: " PO-2026-0001 ".to_string(),
            sales_order_id: SalesOrderId::new("so-1"),
            sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
            creation_basis_id: "basis-1".to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            payment_term_code: " NET-30 ".to_string(),
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            owner_user_id: " buyer-1 ".to_string(),
            target_warehouse_id: Some(crate::ids::WarehouseId::new("wh-1")),
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
        assert_eq!(order.approval_subject_version, 0);
        assert!(order.current_submission_id.is_none());
        assert_eq!(order.current_owner_user_id().unwrap(), "buyer-1");
        assert_eq!(order.target_warehouse_for_receipt().unwrap().as_ref(), "wh-1");

        let mut legacy = order.clone();
        legacy.owner_user_id = None;
        assert!(legacy.current_owner_user_id().is_err());
    }

    #[test]
    fn new_allows_empty_purchase_no_and_rejects_overlong() {
        let empty = PurchaseOrderData {
            purchase_no: "   ".to_string(),
            ..order_data()
        };
        let draft = PurchaseOrder::new(PurchaseOrderId::new("po-2"), empty, "admin-1").unwrap();
        assert!(draft.purchase_no.is_empty());

        let overlong = PurchaseOrderData {
            purchase_no: "p".repeat(65),
            ..order_data()
        };
        assert!(PurchaseOrder::new(PurchaseOrderId::new("po-3"), overlong, "admin-1").is_err());
    }

    #[test]
    fn assign_purchase_no_is_one_shot() {
        let mut order = PurchaseOrder::new(
            PurchaseOrderId::new("po-empty"),
            PurchaseOrderData {
                purchase_no: String::new(),
                ..order_data()
            },
            "admin-1",
        )
        .unwrap();
        order.assign_purchase_no(" PO-1 ").unwrap();
        assert_eq!(order.purchase_no, "PO-1");
        assert!(order.assign_purchase_no("PO-2").is_err());
    }

    #[test]
    fn draft_submission_and_change_revision_guards_are_derived_by_entity() {
        let mut order = new_order();
        assert!(order.draft_submission_id().is_err());
        order
            .attach_draft_submission(PurchaseOrderSubmissionId::new("sub-1"))
            .unwrap();
        assert_eq!(order.draft_submission_id().unwrap().as_ref(), "sub-1");
        assert!(order
            .attach_draft_submission(PurchaseOrderSubmissionId::new("sub-2"))
            .is_err());
        assert!(order.revision_id_for_change().is_err());

        order.start_approval("sub-1", "admin-1").unwrap();
        assert_eq!(order.submission_id_for_formalization().unwrap().as_ref(), "sub-1");
        order
            .formalize_with_revision(PurchaseOrderRevisionId::new("por-1"), "fin-1")
            .unwrap();
        assert_eq!(order.revision_id_for_change().unwrap().as_ref(), "por-1");
        order
            .apply_change_revision(PurchaseOrderRevisionId::new("por-2"), "approver-1")
            .unwrap();
        assert_eq!(order.revision_id_for_change().unwrap().as_ref(), "por-2");
        assert_eq!(order.stable.updated_by, "approver-1");
    }

    #[test]
    fn expected_version_guard_accepts_current_and_rejects_stale() {
        let order = new_order();
        order.ensure_expected_version(order.base.version).unwrap();
        assert!(order
            .ensure_expected_version(order.base.version.saturating_add(1))
            .is_err());
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

        order.start_approval("sub-1", "admin-2").unwrap();
        assert_eq!(order.stable.status(), PurchaseOrderStatus::InApproval);
        assert_eq!(order.approval_subject_version, 1);
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
    fn start_cancel_and_formalize_follow_machine() {
        let mut order = new_order();
        order.start_approval("sub-1", "admin-1").unwrap();
        order.formalize_approved("fin-1").unwrap();
        assert_eq!(order.stable.status(), PurchaseOrderStatus::Effective);
        assert_eq!(order.approval_subject_version, 1);
        assert_eq!(order.review_status, PurchaseReviewStatus::Pending);

        let mut cancelled = new_order();
        cancelled.start_approval("sub-2", "admin-1").unwrap();
        cancelled.cancel_approval("fin-1").unwrap();
        assert_eq!(cancelled.stable.status(), PurchaseOrderStatus::Draft);
        assert_eq!(cancelled.approval_subject_version, 1);
        assert_eq!(cancelled.current_submission_id.as_deref(), Some("sub-2"));
    }

    #[test]
    fn legacy_finance_review_paths_fail_closed() {
        let mut order = new_order();
        assert!(order.submit_for_review("sub-1", "admin-1").is_err());
        assert!(order.apply_finance_review(true, "fin-1").is_err());
        assert!(order.formalize_approved("fin-1").is_err());
        assert!(order.cancel_approval("fin-1").is_err());
    }

    #[test]
    fn status_machine_directional_edges() {
        assert!(ensure_transition(PurchaseOrderStatus::Draft, PurchaseOrderStatus::Draft).is_ok());
        assert!(ensure_transition(PurchaseOrderStatus::Draft, PurchaseOrderStatus::InApproval).is_ok());
        assert!(ensure_transition(PurchaseOrderStatus::Draft, PurchaseOrderStatus::Voided).is_ok());
        assert!(ensure_transition(PurchaseOrderStatus::InApproval, PurchaseOrderStatus::Effective).is_ok());
        assert!(ensure_transition(PurchaseOrderStatus::InApproval, PurchaseOrderStatus::Draft).is_ok());
        assert!(ensure_transition(
            PurchaseOrderStatus::Draft,
            PurchaseOrderStatus::PendingFinanceReview
        )
        .is_err());
        assert!(ensure_transition(
            PurchaseOrderStatus::PendingFinanceReview,
            PurchaseOrderStatus::Effective
        )
        .is_err());
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
