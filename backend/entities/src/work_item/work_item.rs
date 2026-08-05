//! `work_item`：正式待办及处理结果（数据模型 §6.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::WorkItemId;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 业务对象类型代码最大长度。
const OBJECT_TYPE_MAX_LEN: usize = 64;
/// 业务对象 ID 最大长度。
const OBJECT_ID_MAX_LEN: usize = 128;
/// 版本标识最大长度。
const SUBJECT_VERSION_MAX_LEN: usize = 64;
/// 角色标识最大长度。
const ROLE_MAX_LEN: usize = 128;
/// 用户 ID 最大长度。
const USER_ID_MAX_LEN: usize = 128;
/// 原因代码最大长度。
const REASON_CODE_MAX_LEN: usize = 64;
/// 影响摘要最大长度。
const IMPACT_SUMMARY_MAX_LEN: usize = 512;
/// 完成动作标识最大长度。
const COMPLETION_ACTION_MAX_LEN: usize = 128;

/// 任务类型（数据模型 §6.1：当前两期固定 15 类，禁止临时创造同义代码）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemType {
    /// 采购二次确认。
    ProcurementConfirmation,
    /// 低毛利上级确认。
    LowMarginManagerConfirmation,
    /// 采购单财务审核。
    PurchaseOrderReview,
    /// 卡券票款复核。
    CardFundsReview,
    /// 卡券票款差异复核。
    CardFundsDeltaReview,
    /// 卡券销售领导审批。
    CardSalesManagerApproval,
    /// 卡券运营审批。
    CardSalesOperationApproval,
    /// 归属迁移销售确认。
    OwnershipMigrationSalesConfirmation,
    /// 归属迁移财务确认。
    OwnershipMigrationFinanceConfirmation,
    /// 库存调整复核。
    InventoryAdjustmentReview,
    /// 财务纠错复核。
    FinanceCorrectionReview,
    /// 供应商结算复核。
    SupplierSettlementReview,
    /// 导入业务确认。
    ImportBusinessConfirmation,
    /// 集成结果未知。
    IntegrationResultUnknown,
    /// 业务异常。
    BusinessException,
}

impl WorkItemType {
    /// 返回任务类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ProcurementConfirmation => "采购二次确认",
            Self::LowMarginManagerConfirmation => "低毛利上级确认",
            Self::PurchaseOrderReview => "采购单财务审核",
            Self::CardFundsReview => "卡券票款复核",
            Self::CardFundsDeltaReview => "卡券票款差异复核",
            Self::CardSalesManagerApproval => "卡券销售领导审批",
            Self::CardSalesOperationApproval => "卡券运营审批",
            Self::OwnershipMigrationSalesConfirmation => "归属迁移销售确认",
            Self::OwnershipMigrationFinanceConfirmation => "归属迁移财务确认",
            Self::InventoryAdjustmentReview => "库存调整复核",
            Self::FinanceCorrectionReview => "财务纠错复核",
            Self::SupplierSettlementReview => "供应商结算复核",
            Self::ImportBusinessConfirmation => "导入业务确认",
            Self::IntegrationResultUnknown => "集成结果未知",
            Self::BusinessException => "业务异常",
        }
    }

    /// 返回任务类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProcurementConfirmation => "PROCUREMENT_CONFIRMATION",
            Self::LowMarginManagerConfirmation => "LOW_MARGIN_MANAGER_CONFIRMATION",
            Self::PurchaseOrderReview => "PURCHASE_ORDER_REVIEW",
            Self::CardFundsReview => "CARD_FUNDS_REVIEW",
            Self::CardFundsDeltaReview => "CARD_FUNDS_DELTA_REVIEW",
            Self::CardSalesManagerApproval => "CARD_SALES_MANAGER_APPROVAL",
            Self::CardSalesOperationApproval => "CARD_SALES_OPERATION_APPROVAL",
            Self::OwnershipMigrationSalesConfirmation => "OWNERSHIP_MIGRATION_SALES_CONFIRMATION",
            Self::OwnershipMigrationFinanceConfirmation => "OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION",
            Self::InventoryAdjustmentReview => "INVENTORY_ADJUSTMENT_REVIEW",
            Self::FinanceCorrectionReview => "FINANCE_CORRECTION_REVIEW",
            Self::SupplierSettlementReview => "SUPPLIER_SETTLEMENT_REVIEW",
            Self::ImportBusinessConfirmation => "IMPORT_BUSINESS_CONFIRMATION",
            Self::IntegrationResultUnknown => "INTEGRATION_RESULT_UNKNOWN",
            Self::BusinessException => "BUSINESS_EXCEPTION",
        }
    }

    /// 判断任务类型是否允许人工关闭。
    ///
    /// 数据模型 §6.1：审批、确认、结果未知和未完成补偿任务不得人工关闭；
    /// 只有重复、误派或已有替代正式任务时允许关闭。本方法按任务类型给出
    /// 保守默认（审批/确认/结果未知/异常补偿一律不允许人工关闭），
    /// 每类任务注册时的最终关闭策略由 P3 服务层核对。
    ///
    /// # 返回
    /// 不允许人工关闭时返回 `false`。
    pub fn is_manually_closable(self) -> bool {
        !matches!(
            self,
            Self::ProcurementConfirmation
                | Self::LowMarginManagerConfirmation
                | Self::PurchaseOrderReview
                | Self::CardFundsReview
                | Self::CardFundsDeltaReview
                | Self::CardSalesManagerApproval
                | Self::CardSalesOperationApproval
                | Self::OwnershipMigrationSalesConfirmation
                | Self::OwnershipMigrationFinanceConfirmation
                | Self::InventoryAdjustmentReview
                | Self::FinanceCorrectionReview
                | Self::SupplierSettlementReview
                | Self::ImportBusinessConfirmation
                | Self::IntegrationResultUnknown
                | Self::BusinessException
        )
    }
}

/// 任务状态（数据模型 §6.1：`UNCLAIMED` / `IN_PROGRESS` / `COMPLETED` / `CLOSED`）。
///
/// 固定状态机（无运行时扩展）：
/// `UNCLAIMED → IN_PROGRESS`（领取）、`IN_PROGRESS → UNCLAIMED`（暂挂/转交返回）、
/// `IN_PROGRESS → COMPLETED`（正式完成）、`UNCLAIMED | IN_PROGRESS → CLOSED`（关闭）。
/// `COMPLETED` / `CLOSED` 是终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemStatus {
    /// 待领取。
    #[default]
    Unclaimed,
    /// 处理中。
    InProgress,
    /// 已完成。
    Completed,
    /// 已关闭。
    Closed,
}

impl WorkItemStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Unclaimed => "待领取",
            Self::InProgress => "处理中",
            Self::Completed => "已完成",
            Self::Closed => "已关闭",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unclaimed => "UNCLAIMED",
            Self::InProgress => "IN_PROGRESS",
            Self::Completed => "COMPLETED",
            Self::Closed => "CLOSED",
        }
    }
}

impl DocumentState for WorkItemStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Unclaimed => &[Self::InProgress, Self::Closed],
            Self::InProgress => &[Self::Unclaimed, Self::Completed, Self::Closed],
            Self::Completed | Self::Closed => &[],
        }
    }
}

/// 优先级（数据模型 §6.1：优先级和时限）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemPriority {
    /// 紧急。
    Urgent,
    /// 高。
    High,
    /// 普通。
    Normal,
    /// 低。
    Low,
}

impl WorkItemPriority {
    /// 返回优先级的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Urgent => "紧急",
            Self::High => "高",
            Self::Normal => "普通",
            Self::Low => "低",
        }
    }

    /// 返回优先级的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Urgent => "urgent",
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }
}

/// 任务创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemData {
    /// 任务类型。
    pub work_item_type: WorkItemType,
    /// 业务对象类型代码（跨域开放目录，按稳定代码形态校验）。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 任务针对的对象版本（适用时）。
    pub subject_version: Option<String>,
    /// 责任角色。
    pub owner_role: Option<String>,
    /// 当前责任人（创建时不领取，由领取动作写入）。
    pub owner_user_id: Option<String>,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限（适用时）。
    pub due_at: Option<Instant>,
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 业务影响摘要。
    pub impact_summary: Option<String>,
    /// 该任务唯一允许的完成动作。
    pub completion_action: String,
}

/// 任务关闭数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemCloseData {
    /// 关闭原因代码（结构化原因，必填）。
    pub close_reason_code: String,
    /// 关闭原因说明。
    pub close_reason_text: Option<String>,
}

/// 正式待办实体（数据模型 §6.1）。
///
/// 领取 = 条件更新（行锁）原子完成（P2/P3 实现条件更新，实体层保证状态前置）；
/// 完成或关闭任务本身不修改正式业务事实（§6.1）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct WorkItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 任务类型。
    pub work_item_type: WorkItemType,
    /// 业务对象类型代码。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 任务针对的对象版本。
    pub subject_version: Option<String>,
    /// 任务状态。
    pub status: WorkItemStatus,
    /// 责任角色。
    pub owner_role: Option<String>,
    /// 当前责任人。
    pub owner_user_id: Option<String>,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 业务影响摘要。
    pub impact_summary: Option<String>,
    /// 该任务唯一允许的完成动作。
    pub completion_action: String,
    /// 正式完成审计时间。
    pub completed_at: Option<Instant>,
    /// 正式完成执行人。
    pub completed_by: Option<String>,
    /// 关闭原因代码（关闭时必填）。
    pub close_reason_code: Option<String>,
    /// 关闭原因说明。
    pub close_reason_text: Option<String>,
}

impl WorkItem {
    /// 创建待办任务。
    ///
    /// 完成业务对象与动作类字段的校验与规范化（trim、非空、长度上限），
    /// 初始状态为 `UNCLAIMED`，不写完成/关闭审计。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::WorkItemId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的任务实体。
    ///
    /// # 错误
    /// 当业务对象类型/ID 或完成动作为空/超长时返回错误。
    pub fn new(id: WorkItemId, data: WorkItemData) -> Result<Self> {
        let business_object_type = normalize_required_text(
            data.business_object_type,
            "业务对象类型不能为空",
            OBJECT_TYPE_MAX_LEN,
            "业务对象类型过长",
        )?;
        let business_object_id = normalize_required_text(
            data.business_object_id,
            "业务对象ID不能为空",
            OBJECT_ID_MAX_LEN,
            "业务对象ID过长",
        )?;
        let subject_version =
            normalize_optional_text(data.subject_version, "对象版本", SUBJECT_VERSION_MAX_LEN)?;
        let owner_role = normalize_optional_text(data.owner_role, "责任角色", ROLE_MAX_LEN)?;
        let owner_user_id = normalize_optional_text(data.owner_user_id, "责任人", USER_ID_MAX_LEN)?;
        let reason_code = normalize_optional_text(data.reason_code, "原因代码", REASON_CODE_MAX_LEN)?;
        let impact_summary =
            normalize_optional_text(data.impact_summary, "影响摘要", IMPACT_SUMMARY_MAX_LEN)?;
        let completion_action = normalize_required_text(
            data.completion_action,
            "完成动作不能为空",
            COMPLETION_ACTION_MAX_LEN,
            "完成动作过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            work_item_type: data.work_item_type,
            business_object_type,
            business_object_id,
            subject_version,
            status: WorkItemStatus::Unclaimed,
            owner_role,
            owner_user_id,
            priority: data.priority,
            due_at: data.due_at,
            reason_code,
            impact_summary,
            completion_action,
            completed_at: None,
            completed_by: None,
            close_reason_code: None,
            close_reason_text: None,
        })
    }

    /// 领取任务。
    ///
    /// 仅 `UNCLAIMED` 可领取：状态迁移到 `IN_PROGRESS` 并写入当前责任人。
    /// 同一时刻只能被一个用户处理（数据库层以条件更新原子完成，见 §6.1）。
    ///
    /// # 参数
    /// * `user_id` - 领取人
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务不是 `UNCLAIMED` 状态或领取人非法时返回错误。
    pub fn claim(&mut self, user_id: impl Into<String>) -> Result<()> {
        if self.status != WorkItemStatus::Unclaimed {
            return Err(Error::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: format!("{:?}", WorkItemStatus::InProgress),
            });
        }
        let user_id =
            normalize_required_text(user_id.into(), "领取人不能为空", USER_ID_MAX_LEN, "领取人过长")?;
        self.status = WorkItemStatus::InProgress;
        self.owner_user_id = Some(user_id);
        Ok(())
    }

    /// 暂挂任务。
    ///
    /// 暂挂（Defer）后任务回到待领取状态，清除当前责任人（W02 契约）。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务不是 `IN_PROGRESS` 状态时返回错误。
    pub fn defer(&mut self) -> Result<()> {
        if self.status != WorkItemStatus::InProgress {
            return Err(Error::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: format!("{:?}", WorkItemStatus::Unclaimed),
            });
        }
        self.status = WorkItemStatus::Unclaimed;
        self.owner_user_id = None;
        Ok(())
    }

    /// 转交任务。
    ///
    /// 转交到指定责任角色与责任人，任务保持处理中并写入新领取人。
    ///
    /// # 参数
    /// * `role` - 新责任角色
    /// * `user_id` - 新责任人
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务不是 `IN_PROGRESS` 状态或转交对象非法时返回错误。
    pub fn transfer(&mut self, role: impl Into<String>, user_id: impl Into<String>) -> Result<()> {
        if self.status != WorkItemStatus::InProgress {
            return Err(Error::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: format!("{:?}", WorkItemStatus::InProgress),
            });
        }
        let role = normalize_required_text(role.into(), "责任角色不能为空", ROLE_MAX_LEN, "责任角色过长")?;
        let user_id =
            normalize_required_text(user_id.into(), "责任人不能为空", USER_ID_MAX_LEN, "责任人过长")?;
        self.owner_role = Some(role);
        self.owner_user_id = Some(user_id);
        Ok(())
    }

    /// 正式完成任务。
    ///
    /// 仅 `IN_PROGRESS` 可完成；写入完成审计。业务事实与任务 `COMPLETED`
    /// 必须在同一事务返回（§6.1，由 P3 事务编排保证）。
    ///
    /// # 参数
    /// * `completed_by` - 完成执行人
    /// * `at` - 完成时刻
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务不是 `IN_PROGRESS` 状态或执行人非法时返回错误。
    pub fn complete(&mut self, completed_by: impl Into<String>, at: Instant) -> Result<()> {
        if self.status != WorkItemStatus::InProgress {
            return Err(Error::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: format!("{:?}", WorkItemStatus::Completed),
            });
        }
        let completed_by = normalize_required_text(
            completed_by.into(),
            "完成执行人不能为空",
            USER_ID_MAX_LEN,
            "完成执行人过长",
        )?;
        self.status = WorkItemStatus::Completed;
        self.completed_at = Some(at);
        self.completed_by = Some(completed_by);
        Ok(())
    }

    /// 关闭任务。
    ///
    /// 仅非终态任务可关闭（`UNCLAIMED` / `IN_PROGRESS`）；关闭必须记录结构化
    /// 原因（§6.1：只有重复、误派或已有替代正式任务时允许关闭）。任务类型
    /// 是否允许人工关闭由 [`WorkItemType::is_manually_closable`] 给出，由
    /// P3 服务层按任务类型与场景核对后调用；关闭操作人与时间作为
    /// `workflow_action` 关闭记录追加（W02 契约），实体只保存结构化原因。
    ///
    /// # 参数
    /// * `data` - 关闭数据（关闭原因代码必填）
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务已是终态或关闭原因代码为空/超长时返回错误。
    pub fn close(&mut self, data: WorkItemCloseData) -> Result<()> {
        if matches!(self.status, WorkItemStatus::Completed | WorkItemStatus::Closed) {
            return Err(Error::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: format!("{:?}", WorkItemStatus::Closed),
            });
        }
        let close_reason_code = normalize_required_text(
            data.close_reason_code,
            "关闭原因代码不能为空",
            REASON_CODE_MAX_LEN,
            "关闭原因代码过长",
        )?;
        let close_reason_text =
            normalize_optional_text(data.close_reason_text, "关闭原因", IMPACT_SUMMARY_MAX_LEN)?;
        self.status = WorkItemStatus::Closed;
        self.close_reason_code = Some(close_reason_code);
        self.close_reason_text = close_reason_text;
        Ok(())
    }

    /// 判断任务是否已处于终态。
    ///
    /// # 返回
    /// `COMPLETED` / `CLOSED` 时返回 `true`。
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, WorkItemStatus::Completed | WorkItemStatus::Closed)
    }

    /// 判断当前责任人。
    ///
    /// # 参数
    /// * `user_id` - 待核对的用户
    ///
    /// # 返回
    /// 当前责任人与 `user_id` 相同（非终态且已领取）时返回 `true`。
    pub fn is_owned_by(&self, user_id: &str) -> bool {
        self.owner_user_id.as_deref() == Some(user_id) && !self.is_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkItem, WorkItemCloseData, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType};
    use crate::common::state::ensure_transition;
    use crate::common::time::Instant;
    use crate::ids::WorkItemId;

    fn data() -> WorkItemData {
        WorkItemData {
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            business_object_type: " LEGACY_IMPORT_BATCH ".to_string(),
            business_object_id: " batch-1 ".to_string(),
            subject_version: Some(" v3 ".to_string()),
            owner_role: Some("sales".to_string()),
            owner_user_id: None,
            priority: WorkItemPriority::Normal,
            due_at: Some(Instant::from_unix_secs(1_700_086_400)),
            reason_code: Some("IMPORT_READY".to_string()),
            impact_summary: Some(" 待确认导入范围 ".to_string()),
            completion_action: " COMPLETE_IMPORT_BUSINESS_CONFIRMATION ".to_string(),
        }
    }

    /// happy path：字段 trim、初始状态 UNCLAIMED。
    #[test]
    fn new_trims_fields_and_starts_unclaimed() {
        let item = WorkItem::new(WorkItemId::new("wi-1"), data()).unwrap();
        assert_eq!(item.business_object_type, "LEGACY_IMPORT_BATCH");
        assert_eq!(item.business_object_id, "batch-1");
        assert_eq!(item.subject_version.as_deref(), Some("v3"));
        assert_eq!(item.completion_action, "COMPLETE_IMPORT_BUSINESS_CONFIRMATION");
        assert_eq!(item.impact_summary.as_deref(), Some("待确认导入范围"));
        assert_eq!(item.status, WorkItemStatus::Unclaimed);
        assert!(item.owner_user_id.is_none());
    }

    /// 失败路径：必填为空被拒。
    #[test]
    fn new_rejects_empty_business_object_type() {
        let payload = WorkItemData {
            business_object_type: "  ".to_string(),
            ..data()
        };
        assert!(WorkItem::new(WorkItemId::new("wi-1"), payload).is_err());
    }

    /// 失败路径：超长字段被拒。
    #[test]
    fn new_rejects_overlong_completion_action() {
        let payload = WorkItemData {
            completion_action: "A".repeat(129),
            ..data()
        };
        assert!(WorkItem::new(WorkItemId::new("wi-1"), payload).is_err());
    }

    /// 状态机：领取 → 完成全链路合法迁移。
    #[test]
    fn lifecycle_claim_defer_transfer_complete() {
        let mut item = WorkItem::new(WorkItemId::new("wi-1"), data()).unwrap();
        assert!(!item.is_owned_by("alice"));

        item.claim("alice").unwrap();
        assert_eq!(item.status, WorkItemStatus::InProgress);
        assert!(item.is_owned_by("alice"));
        assert!(!item.is_owned_by("bob"));

        item.defer().unwrap();
        assert_eq!(item.status, WorkItemStatus::Unclaimed);
        assert!(item.owner_user_id.is_none());

        item.claim("bob").unwrap();
        item.transfer("finance", "carol").unwrap();
        assert_eq!(item.owner_role.as_deref(), Some("finance"));
        assert_eq!(item.owner_user_id.as_deref(), Some("carol"));

        item.complete("carol", Instant::from_unix_secs(1_700_100_000))
            .unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(item.completed_by.as_deref(), Some("carol"));
        assert!(item.is_terminal());
    }

    /// 状态机：非法迁移被拒（未领取直接完成、领取后重复领取）。
    #[test]
    fn illegal_transitions_are_rejected() {
        let mut item = WorkItem::new(WorkItemId::new("wi-1"), data()).unwrap();
        assert!(item
            .complete("alice", Instant::from_unix_secs(1_700_100_000))
            .is_err());

        item.claim("alice").unwrap();
        assert!(item.claim("bob").is_err(), "已领取任务不能重复领取");
        assert!(item.transfer("sales", "bob").is_ok());

        assert!(ensure_transition(WorkItemStatus::Completed, WorkItemStatus::InProgress).is_err());
        assert!(ensure_transition(WorkItemStatus::Unclaimed, WorkItemStatus::Completed).is_err());
    }

    /// 状态机：逐边定向断言（含不可逆终态，不调用对称闭包辅助）。
    #[test]
    fn directed_edge_assertions() {
        assert!(ensure_transition(WorkItemStatus::Unclaimed, WorkItemStatus::Unclaimed).is_ok());
        assert!(ensure_transition(WorkItemStatus::Unclaimed, WorkItemStatus::InProgress).is_ok());
        assert!(ensure_transition(WorkItemStatus::Unclaimed, WorkItemStatus::Closed).is_ok());
        assert!(ensure_transition(WorkItemStatus::InProgress, WorkItemStatus::Unclaimed).is_ok());
        assert!(ensure_transition(WorkItemStatus::InProgress, WorkItemStatus::Completed).is_ok());
        assert!(ensure_transition(WorkItemStatus::InProgress, WorkItemStatus::Closed).is_ok());
        assert!(ensure_transition(WorkItemStatus::Completed, WorkItemStatus::Completed).is_ok());
        assert!(ensure_transition(WorkItemStatus::Closed, WorkItemStatus::Closed).is_ok());

        for terminal in [WorkItemStatus::Completed, WorkItemStatus::Closed] {
            for &from in &[WorkItemStatus::Unclaimed, WorkItemStatus::InProgress] {
                assert!(ensure_transition(terminal, from).is_err(), "终态不可回退");
            }
        }
        assert!(ensure_transition(WorkItemStatus::Completed, WorkItemStatus::Closed).is_err());
        assert!(ensure_transition(WorkItemStatus::Closed, WorkItemStatus::Completed).is_err());
    }

    /// 关闭：必须记录结构化原因；终态任务不可关闭。
    #[test]
    fn close_requires_reason_and_rejects_terminal() {
        let mut item = WorkItem::new(WorkItemId::new("wi-1"), data()).unwrap();
        assert!(item
            .close(WorkItemCloseData {
                close_reason_code: "  ".to_string(),
                close_reason_text: None,
            })
            .is_err());

        item.close(WorkItemCloseData {
            close_reason_code: "DUPLICATE_TASK".to_string(),
            close_reason_text: Some(" 重复任务 ".to_string()),
        })
        .unwrap();
        assert_eq!(item.status, WorkItemStatus::Closed);
        assert_eq!(item.close_reason_code.as_deref(), Some("DUPLICATE_TASK"));
        assert_eq!(item.close_reason_text.as_deref(), Some("重复任务"));

        assert!(item
            .close(WorkItemCloseData {
                close_reason_code: "DUPLICATE_TASK".to_string(),
                close_reason_text: None,
            })
            .is_err());
    }

    /// 固定任务类型代码与人工关闭策略。
    #[test]
    fn work_item_type_codes_and_manual_close_policy() {
        assert_eq!(
            serde_json::to_string(&WorkItemType::ProcurementConfirmation).unwrap(),
            "\"PROCUREMENT_CONFIRMATION\""
        );
        assert_eq!(
            WorkItemType::IntegrationResultUnknown.as_str(),
            "INTEGRATION_RESULT_UNKNOWN"
        );
        assert_eq!(WorkItemType::BusinessException.label(), "业务异常");
        assert_eq!(WorkItemType::ImportBusinessConfirmation.label(), "导入业务确认");
        assert!(
            !WorkItemType::ImportBusinessConfirmation.is_manually_closable(),
            "确认类任务不得人工关闭"
        );
    }

    /// 优先级与状态代码稳定。
    #[test]
    fn priority_and_status_codes_are_stable() {
        assert_eq!(
            serde_json::to_string(&WorkItemPriority::Urgent).unwrap(),
            "\"urgent\""
        );
        assert_eq!(WorkItemStatus::InProgress.as_str(), "IN_PROGRESS");
        assert_eq!(WorkItemStatus::Unclaimed.label(), "待领取");
        assert_eq!(WorkItemPriority::High.label(), "高");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let item = WorkItem::new(WorkItemId::new("wi-1"), data()).unwrap();
        let roundtrip: WorkItem = bson::from_document(bson::to_document(&item).unwrap()).unwrap();
        assert_eq!(roundtrip, item);
    }
}
