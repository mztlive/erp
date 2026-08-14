//! `work_item`：审批与独立人工任务的当前责任事实。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::WorkItemId;
use crate::validation::{normalize_optional_text, normalize_required_text};

const OBJECT_TYPE_MAX_LEN: usize = 64;
const OBJECT_ID_MAX_LEN: usize = 128;
const RESPONSIBILITY_KEY_MAX_LEN: usize = 128;
const SUBJECT_VERSION_MAX_LEN: usize = 128;
const APPROVAL_STEP_INSTANCE_ID_MAX_LEN: usize = 128;
const ROLE_MAX_LEN: usize = 128;
const ORGANIZATION_ID_MAX_LEN: usize = 128;
const USER_ID_MAX_LEN: usize = 128;
const REASON_CODE_MAX_LEN: usize = 64;
const IMPACT_SUMMARY_MAX_LEN: usize = 512;
const CLOSE_REASON_MAX_LEN: usize = 512;

/// 当前代码注册的任务类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemType {
    /// 采购二次确认。
    ProcurementConfirmation,
    /// 低毛利上级确认。
    LowMarginManagerConfirmation,
    /// 采购单财务审核。
    PurchaseOrderReview,
    /// 销售变更履约影响复核。
    SalesChangeImpactReview,
    /// 销售变更财务影响复核。
    SalesChangeFinanceReview,
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
    /// 返回面向用户的任务类型标签。
    ///
    /// # 返回
    /// 返回稳定中文展示名。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ProcurementConfirmation => "采购二次确认",
            Self::LowMarginManagerConfirmation => "低毛利上级确认",
            Self::PurchaseOrderReview => "采购单财务审核",
            Self::SalesChangeImpactReview => "销售变更履约影响复核",
            Self::SalesChangeFinanceReview => "销售变更财务影响复核",
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

    /// 返回任务类型的持久化代码。
    ///
    /// # 返回
    /// 返回 `SCREAMING_SNAKE_CASE` 稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProcurementConfirmation => "PROCUREMENT_CONFIRMATION",
            Self::LowMarginManagerConfirmation => "LOW_MARGIN_MANAGER_CONFIRMATION",
            Self::PurchaseOrderReview => "PURCHASE_ORDER_REVIEW",
            Self::SalesChangeImpactReview => "SALES_CHANGE_IMPACT_REVIEW",
            Self::SalesChangeFinanceReview => "SALES_CHANGE_FINANCE_REVIEW",
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

    /// 返回当前任务类型是否允许由通用管理动作关闭。
    ///
    /// 当前注册表内均为审批、确认、复核或异常补偿任务，必须由强类型策略决定
    /// 关闭原因，因此通用入口一律保守拒绝。
    ///
    /// # 返回
    /// 当前注册类型均返回 `false`。
    pub fn is_manually_closable(self) -> bool {
        false
    }
}

/// 任务生命周期状态；个人责任是否形成由 `owner_user_id` 独立表达。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemStatus {
    /// 当前仍需处理。
    #[default]
    Open,
    /// 已由强类型领域命令原子完成。
    Completed,
    /// 已按受控原因关闭。
    Closed,
}

impl WorkItemStatus {
    /// 返回面向用户的状态标签。
    ///
    /// # 返回
    /// 返回稳定中文展示名。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open => "待处理",
            Self::Completed => "已完成",
            Self::Closed => "已关闭",
        }
    }

    /// 返回状态的持久化代码。
    ///
    /// # 返回
    /// 返回 `OPEN`、`COMPLETED` 或 `CLOSED`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Completed => "COMPLETED",
            Self::Closed => "CLOSED",
        }
    }
}

impl DocumentState for WorkItemStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Open => &[Self::Completed, Self::Closed],
            Self::Completed | Self::Closed => &[],
        }
    }
}

/// 人工责任的分派模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssignmentMode {
    /// 激活任务时已解析到唯一个人责任人。
    Direct,
    /// 激活任务时先进入责任池，由合格用户开始处理。
    Pool,
}

impl AssignmentMode {
    /// 返回分派模式的持久化代码。
    ///
    /// # 返回
    /// 返回 `DIRECT` 或 `POOL`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "DIRECT",
            Self::Pool => "POOL",
        }
    }
}

/// 当前个人责任的已注册形成来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssignmentSource {
    /// 审批步骤的固定处理人解析器。
    StepResolver,
    /// 独立任务的固定系统规则。
    SystemRule,
    /// 责任池用户主动开始处理。
    SelfStart,
    /// 管理员受控转交。
    AdminReassign,
    /// 管理员或已授权处理人退回责任池。
    AdminRelease,
    /// 阻塞恢复时重新执行冻结解析器。
    RecoveryResolver,
}

impl AssignmentSource {
    /// 返回责任来源的持久化代码。
    ///
    /// # 返回
    /// 返回已注册的稳定来源代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StepResolver => "STEP_RESOLVER",
            Self::SystemRule => "SYSTEM_RULE",
            Self::SelfStart => "SELF_START",
            Self::AdminReassign => "ADMIN_REASSIGN",
            Self::AdminRelease => "ADMIN_RELEASE",
            Self::RecoveryResolver => "RECOVERY_RESOLVER",
        }
    }
}

/// 待办优先级。
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
    /// 返回面向用户的优先级标签。
    ///
    /// # 返回
    /// 返回稳定中文展示名。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Urgent => "紧急",
            Self::High => "高",
            Self::Normal => "普通",
            Self::Low => "低",
        }
    }

    /// 返回优先级的持久化代码。
    ///
    /// # 返回
    /// 返回小写稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Urgent => "urgent",
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }
}

/// 创建任务所需的责任与业务对象快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemData {
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 审批步骤实例；独立人工任务为空。
    pub approval_step_instance_id: Option<String>,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 被处理的不可变提交或业务版本。
    pub subject_version: String,
    /// 责任分派模式。
    pub assignment_mode: AssignmentMode,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 直接分派的个人责任人；责任池创建时必须为空。
    pub owner_user_id: Option<String>,
    /// 初始责任来源。
    pub assignment_source: AssignmentSource,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 业务影响摘要。
    pub impact_summary: Option<String>,
}

/// 受控关闭任务的数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemCloseData {
    /// 不可为空的关闭原因。
    pub close_reason: String,
}

/// 当前人工责任事实。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct WorkItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 审批步骤实例；独立任务为空。
    pub approval_step_instance_id: Option<String>,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 服务端冻结的可选责任维度；普通任务为空，存在时参与开放任务唯一性。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    responsibility_key: Option<String>,
    /// 被处理的不可变提交或业务版本。
    pub subject_version: String,
    /// 生命周期状态。
    pub status: WorkItemStatus,
    /// 责任分派模式。
    pub assignment_mode: AssignmentMode,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人；责任池未开始处理时为空。
    pub owner_user_id: Option<String>,
    /// 曾形成个人责任的用户 ID；只追加、去重，退回与终态动作均保留。
    pub responsibility_actor_ids: Vec<String>,
    /// 当前或最近一次责任来源。
    pub assignment_source: AssignmentSource,
    /// 首次形成个人责任的时间。
    pub assigned_at: Option<Instant>,
    /// 首次正式处理时间。
    pub started_at: Option<Instant>,
    /// 当前个人责任生效时间。
    pub current_assignment_at: Option<Instant>,
    /// 最近一次非只读活动时间。
    pub last_activity_at: Option<Instant>,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 业务影响摘要。
    pub impact_summary: Option<String>,
    /// 正式完成时间。
    pub completed_at: Option<Instant>,
    /// 正式完成人。
    pub completed_by: Option<String>,
    /// 受控关闭时间。
    pub closed_at: Option<Instant>,
    /// 受控关闭操作人。
    pub closed_by: Option<String>,
    /// 受控关闭原因。
    pub close_reason: Option<String>,
}

impl WorkItem {
    /// 创建任务并建立初始责任事实。
    ///
    /// `DIRECT` 必须给出唯一责任人，并立即形成 `assigned_at` 与
    /// `current_assignment_at`；`POOL` 创建时禁止预填责任人。
    ///
    /// # 错误
    /// 必填字段为空、字段超长或分派模式与个人责任不匹配时返回错误。
    pub fn new(id: WorkItemId, data: WorkItemData) -> Result<Self> {
        Self::new_at_with_optional_responsibility_key(id, data, None, Instant::now())
    }

    /// 创建带服务端责任维度的任务。
    ///
    /// 责任维度在创建时规范化并冻结；后续开始处理、退回团队和转交均不得修改。
    /// 客户端输入不得直接调用本入口，应用服务只能传入已注册的固定维度。
    ///
    /// # 错误
    /// 责任维度为空、字段超长，或任务基础数据无效时返回错误。
    pub fn new_with_responsibility_key(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: impl Into<String>,
    ) -> Result<Self> {
        let responsibility_key = normalize_required_text(
            responsibility_key.into(),
            "责任维度不能为空",
            RESPONSIBILITY_KEY_MAX_LEN,
            "责任维度过长",
        )?;
        Self::new_at_with_optional_responsibility_key(id, data, Some(responsibility_key), Instant::now())
    }

    /// 使用确定时间创建任务，供事务编排和确定性测试使用。
    ///
    /// # 错误
    /// 必填字段为空、字段超长或分派模式与个人责任不匹配时返回错误。
    pub fn new_at(id: WorkItemId, data: WorkItemData, at: Instant) -> Result<Self> {
        Self::new_at_with_optional_responsibility_key(id, data, None, at)
    }

    fn new_at_with_optional_responsibility_key(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: Option<String>,
        at: Instant,
    ) -> Result<Self> {
        let normalized = NormalizedWorkItemData::try_from(data)?;
        normalized.ensure_assignment_invariant()?;
        let has_direct_owner = normalized.assignment_mode == AssignmentMode::Direct;
        let responsibility_actor_ids = normalized.owner_user_id.iter().cloned().collect();
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            work_item_type: normalized.work_item_type,
            approval_step_instance_id: normalized.approval_step_instance_id,
            business_object_type: normalized.business_object_type,
            business_object_id: normalized.business_object_id,
            responsibility_key,
            subject_version: normalized.subject_version,
            status: WorkItemStatus::Open,
            assignment_mode: normalized.assignment_mode,
            owner_role: normalized.owner_role,
            owner_organization_id: normalized.owner_organization_id,
            owner_user_id: normalized.owner_user_id,
            responsibility_actor_ids,
            assignment_source: normalized.assignment_source,
            assigned_at: has_direct_owner.then_some(at),
            started_at: None,
            current_assignment_at: has_direct_owner.then_some(at),
            last_activity_at: None,
            priority: normalized.priority,
            due_at: normalized.due_at,
            reason_code: normalized.reason_code,
            impact_summary: normalized.impact_summary,
            completed_at: None,
            completed_by: None,
            closed_at: None,
            closed_by: None,
            close_reason: None,
        })
    }

    /// 返回创建时冻结的责任维度。
    ///
    /// # 返回
    /// 普通任务返回 `None`；采用多责任维度开放唯一性的任务返回固定键。
    pub fn responsibility_key(&self) -> Option<&str> {
        self.responsibility_key.as_deref()
    }

    /// 记录当前责任人的首次处理或后续非终结活动。
    ///
    /// `started_at` 只在第一次调用时写入；后续调用仅推进 `last_activity_at`。
    ///
    /// # 错误
    /// 任务非开放、没有个人责任或操作人不是当前责任人时返回错误。
    pub fn record_activity(&mut self, actor_id: &str, at: Instant) -> Result<()> {
        self.ensure_current_owner(actor_id)?;
        self.started_at.get_or_insert(at);
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 将开放的责任池任务退回团队。
    ///
    /// 保留首次分派与首次处理时间，只清空当前个人责任和当前责任时间。
    /// 调用方必须在应用层完成权限、原因和不可变审计校验。
    ///
    /// # 错误
    /// 任务不是开放的 `POOL` 任务或尚未形成个人责任时返回错误。
    pub fn release_to_pool(&mut self, at: Instant) -> Result<()> {
        self.ensure_open()?;
        if self.assignment_mode != AssignmentMode::Pool || self.owner_user_id.is_none() {
            return Err(Error::from("只有已形成个人责任的开放POOL任务可以退回团队"));
        }
        self.owner_user_id = None;
        self.current_assignment_at = None;
        self.assignment_source = AssignmentSource::AdminRelease;
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 转交开放任务的当前个人责任。
    ///
    /// 首次分派时间只在此前从未形成个人责任时写入；首次处理时间保持不变。
    /// 调用方必须在应用层重新校验目标任职、角色、数据范围与岗位分离。
    ///
    /// # 错误
    /// 任务非开放或目标用户为空、超长时返回错误。
    pub fn reassign(&mut self, target_user_id: impl Into<String>, at: Instant) -> Result<()> {
        self.assign_to(target_user_id, AssignmentSource::AdminReassign, at)
    }

    /// 在审批阻塞恢复时重新形成或校正个人责任。
    ///
    /// 本动作保留首次分派与首次处理时间，并以 `RECOVERY_RESOLVER` 区分管理员
    /// 主动转交；调用方必须先证明原阻塞原因已经消除。
    ///
    /// # 错误
    /// 任务非开放或解析出的用户为空、超长时返回错误。
    pub fn recover_assignment(&mut self, target_user_id: impl Into<String>, at: Instant) -> Result<()> {
        self.assign_to(target_user_id, AssignmentSource::RecoveryResolver, at)
    }

    /// 在审批阻塞恢复时把失效的责任池个人责任清回团队。
    ///
    /// 保留首次时间，仅清空当前个人责任；调用方必须先证明当前责任人已经失效。
    ///
    /// # 错误
    /// 任务非开放或不是 `POOL` 模式时返回错误。
    pub fn recover_to_pool(&mut self, at: Instant) -> Result<()> {
        self.ensure_open()?;
        if self.assignment_mode != AssignmentMode::Pool {
            return Err(Error::from("只有开放POOL任务可以在恢复时退回团队"));
        }
        self.owner_user_id = None;
        self.current_assignment_at = None;
        self.assignment_source = AssignmentSource::RecoveryResolver;
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 由强类型领域命令完成当前开放任务。
    ///
    /// 本方法只形成任务事实；调用方必须把正式领域事实、审批推进和本实体写入
    /// 放在同一事务。完成动作同时按 `if_null` 语义形成首次处理时间。
    ///
    /// # 错误
    /// 任务非开放、没有个人责任或执行人不是当前责任人时返回错误。
    pub fn complete_by_domain_command(&mut self, completed_by: impl Into<String>, at: Instant) -> Result<()> {
        let completed_by = normalize_required_text(
            completed_by.into(),
            "完成执行人不能为空",
            USER_ID_MAX_LEN,
            "完成执行人过长",
        )?;
        self.ensure_current_owner(&completed_by)?;
        self.started_at.get_or_insert(at);
        self.status = WorkItemStatus::Completed;
        self.completed_at = Some(at);
        self.completed_by = Some(completed_by);
        Ok(())
    }

    /// 以受控原因关闭开放任务。
    ///
    /// 调用方必须先执行任务类型关闭策略与专门权限校验；关闭不会完成业务动作。
    ///
    /// # 错误
    /// 任务非开放、操作人或关闭原因为空、超长时返回错误。
    pub fn close(
        &mut self,
        closed_by: impl Into<String>,
        data: WorkItemCloseData,
        at: Instant,
    ) -> Result<()> {
        self.ensure_open()?;
        let closed_by = normalize_required_text(
            closed_by.into(),
            "关闭操作人不能为空",
            USER_ID_MAX_LEN,
            "关闭操作人过长",
        )?;
        let close_reason = normalize_required_text(
            data.close_reason,
            "关闭原因不能为空",
            CLOSE_REASON_MAX_LEN,
            "关闭原因过长",
        )?;
        self.status = WorkItemStatus::Closed;
        self.closed_at = Some(at);
        self.closed_by = Some(closed_by);
        self.close_reason = Some(close_reason);
        Ok(())
    }

    /// 返回任务是否已进入不可逆终态。
    ///
    /// # 返回
    /// `COMPLETED` 或 `CLOSED` 时返回 `true`。
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, WorkItemStatus::Completed | WorkItemStatus::Closed)
    }

    /// 判断给定用户是否是开放任务的当前个人责任人。
    ///
    /// # 返回
    /// 任务开放且责任人与给定用户相同时返回 `true`。
    pub fn is_owned_by(&self, user_id: &str) -> bool {
        self.status == WorkItemStatus::Open && self.owner_user_id.as_deref() == Some(user_id)
    }

    fn ensure_open(&self) -> Result<()> {
        if self.status == WorkItemStatus::Open {
            return Ok(());
        }
        Err(Error::from("只有开放任务可以执行责任动作"))
    }

    fn ensure_current_owner(&self, actor_id: &str) -> Result<()> {
        self.ensure_open()?;
        if self.owner_user_id.as_deref() == Some(actor_id) {
            return Ok(());
        }
        Err(Error::from("只有当前责任人可以处理任务"))
    }

    fn assign_to(
        &mut self,
        target_user_id: impl Into<String>,
        source: AssignmentSource,
        at: Instant,
    ) -> Result<()> {
        self.ensure_open()?;
        let target_user_id = normalize_required_text(
            target_user_id.into(),
            "目标责任人不能为空",
            USER_ID_MAX_LEN,
            "目标责任人过长",
        )?;
        self.assigned_at.get_or_insert(at);
        self.record_responsibility_actor(&target_user_id);
        self.owner_user_id = Some(target_user_id);
        self.current_assignment_at = Some(at);
        self.assignment_source = source;
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 追加首次出现的个人责任人，保留稳定的责任形成顺序。
    fn record_responsibility_actor(&mut self, actor_id: &str) {
        if !self.responsibility_actor_ids.iter().any(|id| id == actor_id) {
            self.responsibility_actor_ids.push(actor_id.to_string());
        }
    }
}

struct NormalizedWorkItemData {
    work_item_type: WorkItemType,
    approval_step_instance_id: Option<String>,
    business_object_type: String,
    business_object_id: String,
    subject_version: String,
    assignment_mode: AssignmentMode,
    owner_role: String,
    owner_organization_id: String,
    owner_user_id: Option<String>,
    assignment_source: AssignmentSource,
    priority: WorkItemPriority,
    due_at: Option<Instant>,
    reason_code: Option<String>,
    impact_summary: Option<String>,
}

impl TryFrom<WorkItemData> for NormalizedWorkItemData {
    type Error = Error;

    fn try_from(data: WorkItemData) -> Result<Self> {
        Ok(Self {
            work_item_type: data.work_item_type,
            approval_step_instance_id: normalize_optional_text(
                data.approval_step_instance_id,
                "审批步骤实例ID",
                APPROVAL_STEP_INSTANCE_ID_MAX_LEN,
            )?,
            business_object_type: normalize_required_text(
                data.business_object_type,
                "业务对象类型不能为空",
                OBJECT_TYPE_MAX_LEN,
                "业务对象类型过长",
            )?,
            business_object_id: normalize_required_text(
                data.business_object_id,
                "业务对象ID不能为空",
                OBJECT_ID_MAX_LEN,
                "业务对象ID过长",
            )?,
            subject_version: normalize_required_text(
                data.subject_version,
                "对象版本不能为空",
                SUBJECT_VERSION_MAX_LEN,
                "对象版本过长",
            )?,
            assignment_mode: data.assignment_mode,
            owner_role: normalize_required_text(
                data.owner_role,
                "责任角色不能为空",
                ROLE_MAX_LEN,
                "责任角色过长",
            )?,
            owner_organization_id: normalize_required_text(
                data.owner_organization_id,
                "责任组织不能为空",
                ORGANIZATION_ID_MAX_LEN,
                "责任组织过长",
            )?,
            owner_user_id: normalize_optional_text(data.owner_user_id, "责任人", USER_ID_MAX_LEN)?,
            assignment_source: data.assignment_source,
            priority: data.priority,
            due_at: data.due_at,
            reason_code: normalize_optional_text(data.reason_code, "原因代码", REASON_CODE_MAX_LEN)?,
            impact_summary: normalize_optional_text(data.impact_summary, "影响摘要", IMPACT_SUMMARY_MAX_LEN)?,
        })
    }
}

impl NormalizedWorkItemData {
    fn ensure_assignment_invariant(&self) -> Result<()> {
        match (self.assignment_mode, self.owner_user_id.is_some()) {
            (AssignmentMode::Direct, true) | (AssignmentMode::Pool, false) => Ok(()),
            (AssignmentMode::Direct, false) => Err(Error::from("DIRECT任务必须有唯一个人责任人")),
            (AssignmentMode::Pool, true) => Err(Error::from("POOL任务创建时不得预填个人责任人")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AssignmentMode, AssignmentSource, WorkItem, WorkItemCloseData, WorkItemData, WorkItemPriority,
        WorkItemStatus, WorkItemType,
    };
    use crate::common::state::ensure_transition;
    use crate::common::time::Instant;
    use crate::ids::WorkItemId;

    fn pool_data() -> WorkItemData {
        WorkItemData {
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            approval_step_instance_id: None,
            business_object_type: " LEGACY_IMPORT_BATCH ".to_string(),
            business_object_id: " batch-1 ".to_string(),
            subject_version: " v3 ".to_string(),
            assignment_mode: AssignmentMode::Pool,
            owner_role: " sales ".to_string(),
            owner_organization_id: " org-1 ".to_string(),
            owner_user_id: None,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: Some(Instant::from_unix_secs(1_700_086_400)),
            reason_code: Some("IMPORT_READY".to_string()),
            impact_summary: Some(" 待确认导入范围 ".to_string()),
        }
    }

    #[test]
    fn pool_starts_open_without_personal_responsibility() {
        let item =
            WorkItem::new_at(WorkItemId::new("wi-1"), pool_data(), Instant::from_unix_secs(100)).unwrap();

        assert_eq!(item.status, WorkItemStatus::Open);
        assert_eq!(item.business_object_type, "LEGACY_IMPORT_BATCH");
        assert_eq!(item.subject_version, "v3");
        assert_eq!(item.responsibility_key(), None);
        assert_eq!(item.owner_role, "sales");
        assert!(item.owner_user_id.is_none());
        assert!(item.responsibility_actor_ids.is_empty());
        assert!(item.assigned_at.is_none());
        assert!(item.current_assignment_at.is_none());
    }

    #[test]
    fn server_responsibility_key_is_normalized_and_frozen_at_creation() {
        let mut item =
            WorkItem::new_with_responsibility_key(WorkItemId::new("wi-scope"), pool_data(), " SALES ")
                .unwrap();

        assert_eq!(item.responsibility_key(), Some("SALES"));
        item.reassign("alice", Instant::from_unix_secs(110)).unwrap();
        item.release_to_pool(Instant::from_unix_secs(120)).unwrap();
        assert_eq!(item.responsibility_key(), Some("SALES"));
        assert!(
            WorkItem::new_with_responsibility_key(WorkItemId::new("wi-empty-scope"), pool_data(), "   ",)
                .is_err()
        );
    }

    #[test]
    fn direct_requires_owner_and_records_first_assignment() {
        let at = Instant::from_unix_secs(100);
        let direct = WorkItemData {
            assignment_mode: AssignmentMode::Direct,
            owner_user_id: Some(" alice ".to_string()),
            assignment_source: AssignmentSource::StepResolver,
            ..pool_data()
        };
        let item = WorkItem::new_at(WorkItemId::new("wi-1"), direct, at).unwrap();

        assert_eq!(item.owner_user_id.as_deref(), Some("alice"));
        assert_eq!(item.responsibility_actor_ids, vec!["alice".to_string()]);
        assert_eq!(item.assigned_at, Some(at));
        assert_eq!(item.current_assignment_at, Some(at));
        assert!(item.started_at.is_none());

        let missing_owner = WorkItemData {
            assignment_mode: AssignmentMode::Direct,
            ..pool_data()
        };
        assert!(WorkItem::new_at(WorkItemId::new("wi-2"), missing_owner, at).is_err());
    }

    #[test]
    fn pool_rejects_prefilled_owner() {
        let data = WorkItemData {
            owner_user_id: Some("alice".to_string()),
            ..pool_data()
        };

        assert!(WorkItem::new(WorkItemId::new("wi-1"), data).is_err());
    }

    #[test]
    fn responsibility_changes_preserve_first_times() {
        let first = Instant::from_unix_secs(100);
        let mut item = WorkItem::new_at(WorkItemId::new("wi-1"), pool_data(), first).unwrap();
        item.reassign("alice", first).unwrap();
        let started = Instant::from_unix_secs(110);
        item.record_activity("alice", started).unwrap();
        item.release_to_pool(Instant::from_unix_secs(120)).unwrap();
        item.reassign("bob", Instant::from_unix_secs(130)).unwrap();
        item.reassign("bob", Instant::from_unix_secs(140)).unwrap();
        item.complete_by_domain_command("bob", Instant::from_unix_secs(150))
            .unwrap();

        assert_eq!(item.assigned_at, Some(first));
        assert_eq!(item.started_at, Some(started));
        assert_eq!(item.current_assignment_at, Some(Instant::from_unix_secs(140)));
        assert_eq!(item.owner_user_id.as_deref(), Some("bob"));
        assert_eq!(item.completed_by.as_deref(), Some("bob"));
        assert_eq!(
            item.responsibility_actor_ids,
            vec!["alice".to_string(), "bob".to_string()]
        );
        assert_eq!(item.assignment_source, AssignmentSource::AdminReassign);
    }

    #[test]
    fn recovery_uses_registered_source_without_rewriting_first_times() {
        let first = Instant::from_unix_secs(100);
        let mut item = WorkItem::new_at(WorkItemId::new("wi-1"), pool_data(), first).unwrap();
        item.recover_assignment("alice", Instant::from_unix_secs(110))
            .unwrap();
        item.record_activity("alice", Instant::from_unix_secs(120))
            .unwrap();
        item.recover_to_pool(Instant::from_unix_secs(130)).unwrap();

        assert_eq!(item.assigned_at, Some(Instant::from_unix_secs(110)));
        assert_eq!(item.started_at, Some(Instant::from_unix_secs(120)));
        assert!(item.owner_user_id.is_none());
        assert!(item.current_assignment_at.is_none());
        assert_eq!(item.responsibility_actor_ids, vec!["alice".to_string()]);
        assert_eq!(item.assignment_source, AssignmentSource::RecoveryResolver);
    }

    #[test]
    fn complete_requires_current_owner_and_is_terminal() {
        let mut item = WorkItem::new_at(
            WorkItemId::new("wi-1"),
            WorkItemData {
                assignment_mode: AssignmentMode::Direct,
                owner_user_id: Some("alice".to_string()),
                assignment_source: AssignmentSource::StepResolver,
                ..pool_data()
            },
            Instant::from_unix_secs(100),
        )
        .unwrap();

        assert!(item
            .complete_by_domain_command("bob", Instant::from_unix_secs(110))
            .is_err());
        item.complete_by_domain_command("alice", Instant::from_unix_secs(110))
            .unwrap();

        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(item.started_at, Some(Instant::from_unix_secs(110)));
        assert_eq!(item.completed_by.as_deref(), Some("alice"));
        assert_eq!(item.responsibility_actor_ids, vec!["alice".to_string()]);
        assert!(item.is_terminal());
    }

    #[test]
    fn close_records_full_audit_and_rejects_terminal() {
        let mut item =
            WorkItem::new_at(WorkItemId::new("wi-1"), pool_data(), Instant::from_unix_secs(100)).unwrap();
        item.reassign("alice", Instant::from_unix_secs(110)).unwrap();
        let at = Instant::from_unix_secs(120);

        item.close(
            "admin",
            WorkItemCloseData {
                close_reason: " 重复任务 ".to_string(),
            },
            at,
        )
        .unwrap();

        assert_eq!(item.status, WorkItemStatus::Closed);
        assert_eq!(item.closed_at, Some(at));
        assert_eq!(item.closed_by.as_deref(), Some("admin"));
        assert_eq!(item.close_reason.as_deref(), Some("重复任务"));
        assert_eq!(item.responsibility_actor_ids, vec!["alice".to_string()]);
        assert!(item
            .close(
                "admin",
                WorkItemCloseData {
                    close_reason: "再次关闭".to_string(),
                },
                Instant::from_unix_secs(130),
            )
            .is_err());
    }

    #[test]
    fn state_machine_has_only_open_to_terminal_edges() {
        assert!(ensure_transition(WorkItemStatus::Open, WorkItemStatus::Completed).is_ok());
        assert!(ensure_transition(WorkItemStatus::Open, WorkItemStatus::Closed).is_ok());
        assert!(ensure_transition(WorkItemStatus::Completed, WorkItemStatus::Open).is_err());
        assert!(ensure_transition(WorkItemStatus::Closed, WorkItemStatus::Open).is_err());
        assert_eq!(WorkItemStatus::Open.as_str(), "OPEN");
        assert_eq!(WorkItemStatus::Open.label(), "待处理");
    }

    #[test]
    fn codes_and_bson_shape_are_stable() {
        assert_eq!(AssignmentMode::Pool.as_str(), "POOL");
        assert_eq!(AssignmentSource::SelfStart.as_str(), "SELF_START");
        assert_eq!(
            WorkItemType::CardSalesManagerApproval.as_str(),
            "CARD_SALES_MANAGER_APPROVAL"
        );
        assert_eq!(WorkItemPriority::High.label(), "高");

        let item =
            WorkItem::new_at(WorkItemId::new("wi-1"), pool_data(), Instant::from_unix_secs(100)).unwrap();
        let document = bson::to_document(&item).unwrap();
        assert_eq!(document.get_str("status").unwrap(), "OPEN");
        assert_eq!(document.get_str("assignment_mode").unwrap(), "POOL");
        assert!(!document.contains_key("responsibility_key"));
        assert!(document.get_array("responsibility_actor_ids").unwrap().is_empty());
        let roundtrip: WorkItem = bson::from_document(document).unwrap();
        assert_eq!(roundtrip, item);
    }
}
