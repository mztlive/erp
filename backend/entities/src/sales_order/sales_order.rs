//! `sales_order` 与 `sales_order_line`（数据模型 §6.4 / W05）。
//!
//! `sales_order` 保存当前状态与当前版本指针，生效内容写不可变 `sales_order_revision`
//! （§4.4）；正式主状态 `commercial_status` 仅保存 `DRAFT / PENDING_REVIEW /
//! EFFECTIVE / VOIDED` 4 值（§7.1），审核环节值一律在 `review_status` 审核轨。
//! 卡券销售单与实物及服务销售单必须使用本表，不得增加平行销售单主表。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{ContractId, CustomerAccountId, PartyId, SalesOrderId, SalesOrderLineId};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::types::{BusinessType, OriginSystem};

/// 销售单号最大长度。
const ORDER_NO_MAX_LEN: usize = 64;
/// 一期来源身份引用最大长度。
const SOURCE_IDENTITY_MAX_LEN: usize = 256;
/// 来源状态代码最大长度。
const SOURCE_STATUS_CODE_MAX_LEN: usize = 64;

/// 商业主状态（数据模型 §7.1：仅 4 值，审核环节走 `review_status` 审核轨）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommercialStatus {
    /// 草稿。
    Draft,
    /// 审核中（进入 `review_status` 审核轨）。
    PendingReview,
    /// 已生效（不可直接编辑，变化通过销售变更单）。
    Effective,
    /// 已作废。
    Voided,
}

impl CommercialStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingReview => "审核中",
            Self::Effective => "已生效",
            Self::Voided => "已作废",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::PendingReview => "PENDING_REVIEW",
            Self::Effective => "EFFECTIVE",
            Self::Voided => "VOIDED",
        }
    }
}

impl DocumentState for CommercialStatus {
    /// 数据模型 §7.1：`DRAFT → PENDING_REVIEW`、`DRAFT → VOIDED`、
    /// `PENDING_REVIEW → DRAFT`（驳回回到草稿）、`PENDING_REVIEW → EFFECTIVE`；
    /// `EFFECTIVE`/`VOIDED` 为终态（生效后变化走销售变更单）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::PendingReview, Self::Voided],
            Self::PendingReview => &[Self::Draft, Self::Effective],
            Self::Effective | Self::Voided => &[],
        }
    }
}

/// 审核轨状态（数据模型 §6.4：未提交、待采购确认、待低毛利上级确认、待销售领导、
/// 待运营、已通过、已驳回；§7.1/§7.3 审核环节值禁止写回主状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewStatus {
    /// 未提交。
    NotSubmitted,
    /// 待采购确认。
    PendingProcurementConfirmation,
    /// 待低毛利上级确认。
    PendingLowMarginSuperior,
    /// 待销售领导。
    PendingSalesLeader,
    /// 待运营。
    PendingOperations,
    /// 已通过。
    Approved,
    /// 已驳回。
    Rejected,
    /// 审批中。
    InApproval,
}

impl ReviewStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotSubmitted => "未提交",
            Self::PendingProcurementConfirmation => "待采购确认",
            Self::PendingLowMarginSuperior => "待低毛利上级确认",
            Self::PendingSalesLeader => "待销售领导",
            Self::PendingOperations => "待运营",
            Self::Approved => "已通过",
            Self::Rejected => "已驳回",
            Self::InApproval => "审批中",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotSubmitted => "NOT_SUBMITTED",
            Self::PendingProcurementConfirmation => "PENDING_PROCUREMENT_CONFIRMATION",
            Self::PendingLowMarginSuperior => "PENDING_LOW_MARGIN_SUPERIOR",
            Self::PendingSalesLeader => "PENDING_SALES_LEADER",
            Self::PendingOperations => "PENDING_OPERATIONS",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::InApproval => "IN_APPROVAL",
        }
    }
}

impl DocumentState for ReviewStatus {
    /// 审核轨固定邻接。
    ///
    /// `SalesOrder` 与 `VoucherSalesOrder` 目标路径均为
    /// `NOT_SUBMITTED → IN_APPROVAL → APPROVED`，撤回
    /// `IN_APPROVAL → NOT_SUBMITTED`。驳回不改业务状态。
    /// 旧逐节点复核态与 `REJECTED` 不得由新提交写入；邻接仅保留给未删除旧数据。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::NotSubmitted => &[
                Self::PendingProcurementConfirmation,
                Self::PendingSalesLeader,
                Self::InApproval,
            ],
            Self::InApproval => &[Self::Approved, Self::NotSubmitted],
            Self::PendingProcurementConfirmation => {
                &[Self::PendingLowMarginSuperior, Self::Approved, Self::Rejected]
            }
            Self::PendingLowMarginSuperior => &[Self::PendingProcurementConfirmation],
            Self::PendingSalesLeader => &[Self::PendingOperations, Self::Rejected],
            Self::PendingOperations => &[Self::Approved, Self::Rejected],
            Self::Approved => &[],
            Self::Rejected => &[Self::NotSubmitted],
        }
    }
}

/// 履约进度（数据模型 §6.4：未开始、部分履约、已完成；展示复合态不写回主状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentProgress {
    /// 未开始。
    NotStarted,
    /// 部分履约。
    PartiallyFulfilled,
    /// 已完成。
    Completed,
}

impl FulfillmentProgress {
    /// 返回进度的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotStarted => "未开始",
            Self::PartiallyFulfilled => "部分履约",
            Self::Completed => "已完成",
        }
    }

    /// 返回进度的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotStarted => "NOT_STARTED",
            Self::PartiallyFulfilled => "PARTIALLY_FULFILLED",
            Self::Completed => "COMPLETED",
        }
    }
}

/// 回款进度（数据模型 §6.4：未收、部分回款、已结清）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CollectionProgress {
    /// 未收。
    NotCollected,
    /// 部分回款。
    PartiallyCollected,
    /// 已结清。
    Settled,
}

impl CollectionProgress {
    /// 返回进度的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotCollected => "未收",
            Self::PartiallyCollected => "部分回款",
            Self::Settled => "已结清",
        }
    }

    /// 返回进度的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotCollected => "NOT_COLLECTED",
            Self::PartiallyCollected => "PARTIALLY_COLLECTED",
            Self::Settled => "SETTLED",
        }
    }
}

/// 开票进度（数据模型 §6.4：未开、部分开票、已完成）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InvoiceProgress {
    /// 未开。
    NotInvoiced,
    /// 部分开票。
    PartiallyInvoiced,
    /// 已完成。
    Completed,
}

impl InvoiceProgress {
    /// 返回进度的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotInvoiced => "未开",
            Self::PartiallyInvoiced => "部分开票",
            Self::Completed => "已完成",
        }
    }

    /// 返回进度的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotInvoiced => "NOT_INVOICED",
            Self::PartiallyInvoiced => "PARTIALLY_INVOICED",
            Self::Completed => "COMPLETED",
        }
    }
}

/// 关闭状态（数据模型 §6.4：未满足关闭、可关闭、已关闭；进入 `CLOSED` 只表示
/// `close_status` 置为关闭，主状态不保存该展示复合态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CloseStatus {
    /// 未满足关闭。
    NotSatisfied,
    /// 可关闭。
    Closeable,
    /// 已关闭。
    Closed,
}

impl CloseStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotSatisfied => "未满足关闭",
            Self::Closeable => "可关闭",
            Self::Closed => "已关闭",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotSatisfied => "NOT_SATISFIED",
            Self::Closeable => "CLOSEABLE",
            Self::Closed => "CLOSED",
        }
    }
}

/// 销售单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderData {
    /// 销售单号（唯一，创建后不可修改）。
    pub order_no: String,
    /// 业务性质（创建后永久不变）。
    pub business_type: BusinessType,
    /// 最初创建入口（创建后永久不变）。
    pub origin_system: OriginSystem,
    /// 一期商城来源键映射；ERP 新建单为空。
    pub source_identity_id: Option<String>,
    /// 客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 合同稳定身份（无合同时为空）。
    pub contract_id: Option<ContractId>,
    /// 结算主体。
    pub settlement_party_id: PartyId,
    /// 一期商城原始状态代码，只用于追溯。
    pub source_status_code: Option<String>,
}

/// 销售单更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SalesOrderUpdate {
    /// 客户稳定身份；`None` 表示不修改。
    pub customer_id: Option<CustomerAccountId>,
    /// 合同稳定身份；`None` 表示不修改。
    pub contract_id: Option<ContractId>,
    /// 结算主体；`None` 表示不修改。
    pub settlement_party_id: Option<PartyId>,
    /// 来源状态代码；`None` 表示不修改。
    pub source_status_code: Option<String>,
}

/// 销售单实体（稳定主表，数据模型 §6.4）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SalesOrder {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<CommercialStatus>,
    /// 销售单号（创建后不可修改）。
    pub order_no: String,
    /// 业务性质（创建后永久不变）。
    pub business_type: BusinessType,
    /// 最初创建入口（创建后永久不变）。
    pub origin_system: OriginSystem,
    /// 一期商城来源键映射。
    pub source_identity_id: Option<String>,
    /// 客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 合同稳定身份。
    pub contract_id: Option<ContractId>,
    /// 结算主体。
    pub settlement_party_id: PartyId,
    /// 商业主状态（§7.1 仅 4 值）。
    pub commercial_status: CommercialStatus,
    /// 审核轨状态（§7.1 审核环节值，禁止写回主状态）。
    pub review_status: ReviewStatus,
    /// 履约进度。
    pub fulfillment_progress: FulfillmentProgress,
    /// 回款进度。
    pub collection_progress: CollectionProgress,
    /// 开票进度。
    pub invoice_progress: InvoiceProgress,
    /// 关闭状态。
    pub close_status: CloseStatus,
    /// 一期商城原始状态，只用于追溯。
    pub source_status_code: Option<String>,
    /// 生效时间。
    pub effective_at: Option<Instant>,
    /// ERP 关闭时间。
    pub closed_at: Option<Instant>,
}

impl PartialEq for SalesOrder {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.order_no == other.order_no
            && self.business_type == other.business_type
            && self.origin_system == other.origin_system
            && self.source_identity_id == other.source_identity_id
            && self.customer_id == other.customer_id
            && self.contract_id == other.contract_id
            && self.settlement_party_id == other.settlement_party_id
            && self.commercial_status == other.commercial_status
            && self.review_status == other.review_status
            && self.fulfillment_progress == other.fulfillment_progress
            && self.collection_progress == other.collection_progress
            && self.invoice_progress == other.invoice_progress
            && self.close_status == other.close_status
            && self.source_status_code == other.source_status_code
            && self.effective_at == other.effective_at
            && self.closed_at == other.closed_at
    }
}

impl Eq for SalesOrder {}

impl SalesOrder {
    /// 创建销售单（草稿态；明细与正式内容在销售审批提交时形成）。
    ///
    /// 完成 order_no 等文本字段的校验与规范化（去首尾空白、非空、长度上限），
    /// 一期来源单 `source_identity_id`/`source_status_code` 只作追溯不参与业务判定。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的销售单实体（`Draft`、`NotSubmitted`）。
    ///
    /// # 错误
    /// 当 order_no 为空/超长或可选字段超长时返回错误。
    pub fn new(id: SalesOrderId, data: SalesOrderData, created_by: impl Into<String>) -> Result<Self> {
        let order_no = normalize_required_text(
            data.order_no,
            "销售单号不能为空",
            ORDER_NO_MAX_LEN,
            "销售单号过长",
        )?;
        let source_identity_id =
            normalize_optional_text(data.source_identity_id, "来源身份引用", SOURCE_IDENTITY_MAX_LEN)?;
        let source_status_code = normalize_optional_text(
            data.source_status_code,
            "来源状态代码",
            SOURCE_STATUS_CODE_MAX_LEN,
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(CommercialStatus::Draft, created_by),
            order_no,
            business_type: data.business_type,
            origin_system: data.origin_system,
            source_identity_id,
            customer_id: data.customer_id,
            contract_id: data.contract_id,
            settlement_party_id: data.settlement_party_id,
            commercial_status: CommercialStatus::Draft,
            review_status: ReviewStatus::NotSubmitted,
            fulfillment_progress: FulfillmentProgress::NotStarted,
            collection_progress: CollectionProgress::NotCollected,
            invoice_progress: InvoiceProgress::NotInvoiced,
            close_status: CloseStatus::NotSatisfied,
            source_status_code,
            effective_at: None,
            closed_at: None,
        })
    }

    /// 更新销售单。
    ///
    /// 复用 `new` 的校验规则；`order_no`/`business_type`/`origin_system` 是关键
    /// 身份字段，不允许在通用更新中修改（§6.4 约束）。`EFFECTIVE` 后不可直接编辑
    /// （§7.1：变化通过 `sales_change_order`），`VOIDED` 后同样锁定。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 已生效/已作废，或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: SalesOrderUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.ensure_editable()?;
        if let Some(customer_id) = update.customer_id {
            self.customer_id = customer_id;
        }
        if let Some(contract_id) = update.contract_id {
            self.contract_id = Some(contract_id);
        }
        if let Some(settlement_party_id) = update.settlement_party_id {
            self.settlement_party_id = settlement_party_id;
        }
        if let Some(source_status_code) = update.source_status_code {
            self.source_status_code = normalize_optional_text(
                Some(source_status_code),
                "来源状态代码",
                SOURCE_STATUS_CODE_MAX_LEN,
            )?;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 提交进入审核（主状态 `DRAFT → PENDING_REVIEW`）。
    ///
    /// 实物及服务与卡券目标路径均进入 `IN_APPROVAL`。旧逐节点复核态不得由本方法写入。
    ///
    /// # 参数
    /// * `updated_by` - 提交人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿态或审核轨非未提交态时返回 [`Error::InvalidStateTransition`]。
    pub fn submit_for_review(&mut self, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.commercial_status, CommercialStatus::PendingReview)?;
        ensure_transition(self.review_status, ReviewStatus::NotSubmitted)?;
        self.transition_commercial_status(CommercialStatus::PendingReview)?;
        self.review_status = ReviewStatus::InApproval;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 销售单提交并进入统一审批。
    ///
    /// 商业主状态变为 `PENDING_REVIEW`，审核轨只允许 `IN_APPROVAL`。
    /// `GoodsService` 与 `Voucher` 共用本端口，不得再写入卡券两级复核态。
    ///
    /// # 参数
    /// * `updated_by` - 提交人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿或审核轨非未提交时返回冲突或非法迁移。
    pub fn start_approval_submission(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.submit_for_review(updated_by)
    }

    /// 撤回统一审批提交，回到可修正草稿。
    ///
    /// 只允许从 `IN_APPROVAL` 撤回；`subject_version` 不回退。
    /// 不得经 `REJECTED` 或逐节点复核态。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 审核轨不是审批中时返回冲突。
    pub fn cancel_approval_submission(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.review_status != ReviewStatus::InApproval {
            return Err(Error::from("只有审批中的销售单可以撤回审批提交"));
        }
        self.return_to_draft(updated_by)
    }

    /// 驳回后回到可处理草稿（主状态 `PENDING_REVIEW → DRAFT`，§7.1）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非审核中态时返回 [`Error::InvalidStateTransition`]。
    pub fn return_to_draft(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.transition_commercial_status(CommercialStatus::Draft)?;
        self.review_status = ReviewStatus::NotSubmitted;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 审批通过并生效（主状态 `PENDING_REVIEW → EFFECTIVE`）。
    ///
    /// 审核轨同时推进到 `Approved`。目标路径只接受 `IN_APPROVAL`。
    /// 生效时间由调用方以服务端时间给出。
    ///
    /// # 参数
    /// * `effective_at` - 生效时间
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非审核中态或审核轨不允许直接通过时返回
    /// [`Error::InvalidStateTransition`]。
    pub fn approve(&mut self, effective_at: Instant, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.commercial_status, CommercialStatus::Effective)?;
        ensure_transition(self.review_status, ReviewStatus::Approved)?;
        self.transition_commercial_status(CommercialStatus::Effective)?;
        self.review_status = ReviewStatus::Approved;
        self.effective_at = Some(effective_at);
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 作废草稿（主状态 `DRAFT → VOIDED`）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿态时返回 [`Error::InvalidStateTransition`]。
    pub fn void(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.transition_commercial_status(CommercialStatus::Voided)?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 推进唯一商业主状态并保持 `StableBase` 的通用状态镜像一致。
    ///
    /// 业务查询和索引只使用 `commercial_status`；`StableBase.status` 仅是实体基元
    /// 随带的同类型镜像，任何状态迁移都必须在本方法内原子同步，禁止形成两个
    /// 不同的销售主状态。
    fn transition_commercial_status(&mut self, to: CommercialStatus) -> Result<()> {
        ensure_transition(self.commercial_status, to)?;
        self.commercial_status = to;
        self.stable.status = to;
        Ok(())
    }

    /// 推进审核轨状态（不改变主状态）。
    ///
    /// # 参数
    /// * `to` - 目标审核轨状态
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 主状态非审核中，或迁移非法时返回 [`Error::InvalidStateTransition`]。
    pub fn transition_review(&mut self, to: ReviewStatus, updated_by: impl Into<String>) -> Result<()> {
        if self.commercial_status != CommercialStatus::PendingReview {
            return Err(Error::InvalidStateTransition {
                from: format!("{:?}", self.commercial_status),
                to: "review_transition".to_string(),
            });
        }
        ensure_transition(self.review_status, to)?;
        self.review_status = to;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 绑定当前生效版本（审批通过或同步应用后切换版本指针）。
    ///
    /// # 参数
    /// * `revision_id` - 新销售版本主键
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 无返回值；更新当前版本指针并记录更新人。
    pub fn attach_revision(&mut self, revision_id: impl Into<String>, updated_by: impl Into<String>) {
        self.stable.current_revision_id = Some(revision_id.into());
        self.stable.touch(updated_by);
    }

    /// 校验是否允许直接编辑商业字段。
    ///
    /// # 返回
    /// 可编辑时返回 `Ok(())`。
    ///
    /// # 错误
    /// `EFFECTIVE`/`VOIDED` 时返回错误（§7.1：生效后不直接编辑）。
    fn ensure_editable(&self) -> Result<()> {
        match self.commercial_status {
            CommercialStatus::Draft | CommercialStatus::PendingReview => Ok(()),
            CommercialStatus::Effective | CommercialStatus::Voided => {
                Err(Error::from("已生效或已作废的销售单不允许直接编辑"))
            }
        }
    }
}

/// 稳定明细行状态（数据模型 §6.4：有效、被后续版本移除）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LineStatus {
    /// 有效。
    Active,
    /// 被后续版本移除。
    Removed,
}

impl LineStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "有效",
            Self::Removed => "已移除",
        }
    }
}

impl DocumentState for LineStatus {
    /// 有效行可被后续版本移除；移除行为终态（不复用历史行号，§6.4）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Removed],
            Self::Removed => &[],
        }
    }
}

/// 稳定明细行创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderLineData {
    /// 单内稳定行号（从 1 递增，变更不复用历史行号）。
    pub line_no: u32,
}

/// 稳定明细行实体（数据模型 §6.4：`(sales_order_id, line_no)` 唯一）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属销售单。
    pub sales_order_id: SalesOrderId,
    /// 单内稳定行号。
    pub line_no: u32,
    /// 行状态。
    pub line_status: LineStatus,
}

impl SalesOrderLine {
    /// 创建稳定明细行（初始 `Active`）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderLineId`）
    /// * `sales_order_id` - 所属销售单
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的明细行实体。
    ///
    /// # 错误
    /// 行号为零（越界）时返回错误。
    pub fn new(id: SalesOrderLineId, sales_order_id: SalesOrderId, data: SalesOrderLineData) -> Result<Self> {
        if data.line_no == 0 {
            return Err(Error::from("行号必须为正整数"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            sales_order_id,
            line_no: data.line_no,
            line_status: LineStatus::Active,
        })
    }

    /// 将行标记为被后续版本移除（终态）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 行已移除时返回 [`Error::InvalidStateTransition`]。
    pub fn remove(&mut self) -> Result<()> {
        ensure_transition(self.line_status, LineStatus::Removed)?;
        self.line_status = LineStatus::Removed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> SalesOrderData {
        SalesOrderData {
            order_no: " SO-2026-0001 ".to_string(),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: Some(ContractId::new("contract-1")),
            settlement_party_id: PartyId::new("party-1"),
            source_status_code: None,
        }
    }

    #[test]
    fn new_trims_and_initializes_draft_state() {
        let order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();

        assert_eq!(order.order_no, "SO-2026-0001");
        assert_eq!(order.business_type, BusinessType::GoodsService);
        assert_eq!(order.origin_system, OriginSystem::Erp);
        assert_eq!(order.commercial_status, CommercialStatus::Draft);
        assert_eq!(order.stable.status, order.commercial_status);
        assert_eq!(order.review_status, ReviewStatus::NotSubmitted);
        assert_eq!(order.fulfillment_progress, FulfillmentProgress::NotStarted);
        assert_eq!(order.collection_progress, CollectionProgress::NotCollected);
        assert_eq!(order.invoice_progress, InvoiceProgress::NotInvoiced);
        assert_eq!(order.close_status, CloseStatus::NotSatisfied);
        assert_eq!(order.stable.created_by, "admin-1");
        assert!(order.effective_at.is_none());
        assert!(order.closed_at.is_none());
    }

    #[test]
    fn new_rejects_blank_and_overlong_order_no() {
        let blank = SalesOrderData {
            order_no: "   ".to_string(),
            ..data()
        };
        assert!(SalesOrder::new(SalesOrderId::new("o-1"), blank, "admin-1").is_err());

        let overlong = SalesOrderData {
            order_no: "x".repeat(65),
            ..data()
        };
        assert!(SalesOrder::new(SalesOrderId::new("o-1"), overlong, "admin-1").is_err());
    }

    #[test]
    fn new_rejects_overlong_source_fields() {
        let overlong_identity = SalesOrderData {
            source_identity_id: Some("x".repeat(257)),
            ..data()
        };
        assert!(SalesOrder::new(SalesOrderId::new("o-1"), overlong_identity, "admin-1").is_err());

        let overlong_code = SalesOrderData {
            source_status_code: Some("x".repeat(65)),
            ..data()
        };
        assert!(SalesOrder::new(SalesOrderId::new("o-1"), overlong_code, "admin-1").is_err());
    }

    #[test]
    fn update_keeps_identity_fields_and_touches_auditor() {
        let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();
        order
            .update(
                SalesOrderUpdate {
                    customer_id: Some(CustomerAccountId::new("cust-2")),
                    source_status_code: Some("  PAID ".to_string()),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();

        assert_eq!(order.customer_id, CustomerAccountId::new("cust-2"));
        assert_eq!(order.source_status_code.as_deref(), Some("PAID"));
        assert_eq!(order.order_no, "SO-2026-0001", "单号不可修改");
        assert_eq!(
            order.business_type,
            BusinessType::GoodsService,
            "业务性质不可修改"
        );
        assert_eq!(order.origin_system, OriginSystem::Erp, "创建入口不可修改");
        assert_eq!(order.stable.updated_by, "admin-2");
    }

    #[test]
    fn effective_order_rejects_direct_update() {
        let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();
        order
            .submit_for_review("admin-1")
            .and_then(|()| order.transition_review(ReviewStatus::Approved, "reviewer"))
            .and_then(|()| order.approve(Instant::from_unix_secs(1_800_000_000), "reviewer"))
            .unwrap();

        assert_eq!(order.commercial_status, CommercialStatus::Effective);
        assert_eq!(order.stable.status, order.commercial_status);
        assert!(order.update(SalesOrderUpdate::default(), "admin-2").is_err());
    }

    #[test]
    fn commercial_status_machine_edges_are_directed() {
        // §7.1 主状态逐边定向断言（含不可逆终态，不适用对称闭包辅助）。
        assert!(ensure_transition(CommercialStatus::Draft, CommercialStatus::Draft).is_ok());
        assert!(ensure_transition(CommercialStatus::Draft, CommercialStatus::PendingReview).is_ok());
        assert!(ensure_transition(CommercialStatus::Draft, CommercialStatus::Voided).is_ok());
        assert!(ensure_transition(CommercialStatus::PendingReview, CommercialStatus::Draft).is_ok());
        assert!(ensure_transition(CommercialStatus::PendingReview, CommercialStatus::Effective).is_ok());
        assert!(ensure_transition(CommercialStatus::Draft, CommercialStatus::Effective).is_err());
        assert!(ensure_transition(CommercialStatus::Effective, CommercialStatus::Draft).is_err());
        assert!(ensure_transition(CommercialStatus::Effective, CommercialStatus::Voided).is_err());
        assert!(ensure_transition(CommercialStatus::Voided, CommercialStatus::Draft).is_err());
        assert!(CommercialStatus::Effective.allowed_next().is_empty());
        assert!(CommercialStatus::Voided.allowed_next().is_empty());
    }

    #[test]
    fn full_approval_flow_and_rejection_flow() {
        let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();

        // 实物及服务：提交直接进入统一审批，最终通过生效；撤回回到草稿。
        order.start_approval_submission("admin-1").unwrap();
        assert_eq!(order.commercial_status, CommercialStatus::PendingReview);
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        assert!(order
            .transition_review(ReviewStatus::Rejected, "approver")
            .is_err());
        assert!(order
            .transition_review(ReviewStatus::PendingProcurementConfirmation, "approver")
            .is_err());
        order
            .transition_review(ReviewStatus::Approved, "approver")
            .and_then(|()| order.approve(Instant::from_unix_secs(1_800_000_000), "approver"))
            .unwrap();
        assert_eq!(order.commercial_status, CommercialStatus::Effective);
        assert_eq!(order.review_status, ReviewStatus::Approved);
        assert_eq!(order.effective_at.unwrap().unix_secs(), 1_800_000_000);

        let mut withdrawn = SalesOrder::new(SalesOrderId::new("o-2"), data(), "admin-1").unwrap();
        withdrawn.start_approval_submission("admin-1").unwrap();
        withdrawn.cancel_approval_submission("admin-1").unwrap();
        assert_eq!(withdrawn.commercial_status, CommercialStatus::Draft);
        assert_eq!(withdrawn.stable.status, withdrawn.commercial_status);
        assert_eq!(withdrawn.review_status, ReviewStatus::NotSubmitted);
        assert!(withdrawn.cancel_approval_submission("admin-1").is_err());
    }

    #[test]
    fn voucher_order_enters_unified_in_approval() {
        let voucher_data = SalesOrderData {
            business_type: BusinessType::Voucher,
            ..data()
        };
        let mut order = SalesOrder::new(SalesOrderId::new("o-3"), voucher_data, "admin-1").unwrap();
        order.start_approval_submission("admin-1").unwrap();
        assert_eq!(order.commercial_status, CommercialStatus::PendingReview);
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        assert!(order
            .transition_review(ReviewStatus::Rejected, "approver")
            .is_err());
        assert!(order
            .transition_review(ReviewStatus::PendingSalesLeader, "approver")
            .is_err());
        assert!(order
            .transition_review(ReviewStatus::PendingOperations, "approver")
            .is_err());
        order
            .transition_review(ReviewStatus::Approved, "approver")
            .and_then(|()| order.approve(Instant::from_unix_secs(1_800_000_000), "approver"))
            .unwrap();
        assert_eq!(order.commercial_status, CommercialStatus::Effective);
        assert_eq!(order.review_status, ReviewStatus::Approved);

        let mut withdrawn = SalesOrder::new(
            SalesOrderId::new("o-4"),
            SalesOrderData {
                business_type: BusinessType::Voucher,
                ..data()
            },
            "admin-1",
        )
        .unwrap();
        withdrawn.start_approval_submission("admin-1").unwrap();
        withdrawn.cancel_approval_submission("admin-1").unwrap();
        assert_eq!(withdrawn.commercial_status, CommercialStatus::Draft);
        assert_eq!(withdrawn.review_status, ReviewStatus::NotSubmitted);
    }

    #[test]
    fn void_rejects_from_non_draft() {
        let mut order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();
        order.submit_for_review("admin-1").unwrap();
        assert!(order.void("admin-2").is_err(), "审核中不可作废");
        order.return_to_draft("admin-2").unwrap();
        order.void("admin-2").unwrap();
        assert_eq!(order.commercial_status, CommercialStatus::Voided);
        assert_eq!(order.stable.status, order.commercial_status);
    }

    #[test]
    fn line_new_and_remove() {
        let line = SalesOrderLine::new(
            SalesOrderLineId::new("l-1"),
            SalesOrderId::new("o-1"),
            SalesOrderLineData { line_no: 1 },
        )
        .unwrap();
        assert_eq!(line.line_status, LineStatus::Active);

        let mut line = line;
        line.remove().unwrap();
        assert_eq!(line.line_status, LineStatus::Removed);
        assert!(line.remove().is_ok(), "同态重复移除幂等通过");
        assert!(ensure_transition(LineStatus::Removed, LineStatus::Active).is_err());
    }

    #[test]
    fn line_rejects_zero_line_no() {
        assert!(SalesOrderLine::new(
            SalesOrderLineId::new("l-1"),
            SalesOrderId::new("o-1"),
            SalesOrderLineData { line_no: 0 },
        )
        .is_err());
    }

    #[test]
    fn entities_roundtrip_through_bson() {
        let order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();
        let roundtrip: SalesOrder = bson::from_document(bson::to_document(&order).unwrap()).unwrap();
        assert_eq!(roundtrip, order);

        let line = SalesOrderLine::new(
            SalesOrderLineId::new("l-1"),
            SalesOrderId::new("o-1"),
            SalesOrderLineData { line_no: 1 },
        )
        .unwrap();
        let roundtrip_line: SalesOrderLine = bson::from_document(bson::to_document(&line).unwrap()).unwrap();
        assert_eq!(roundtrip_line, line);
    }

    #[test]
    fn progress_enums_expose_labels() {
        assert_eq!(FulfillmentProgress::PartiallyFulfilled.label(), "部分履约");
        assert_eq!(CollectionProgress::Settled.label(), "已结清");
        assert_eq!(InvoiceProgress::PartiallyInvoiced.label(), "部分开票");
        assert_eq!(CloseStatus::Closeable.label(), "可关闭");
        assert_eq!(ReviewStatus::PendingLowMarginSuperior.label(), "待低毛利上级确认");
        assert_eq!(CommercialStatus::Effective.label(), "已生效");
        assert_eq!(
            serde_json::to_string(&CommercialStatus::PendingReview).unwrap(),
            "\"PENDING_REVIEW\""
        );
    }
}
