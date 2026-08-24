//! `work_item`：审批与独立人工任务的当前责任事实。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::WorkItemId;
use crate::validation::{normalize_optional_text, normalize_required_text};

use bpm::ApprovalNodeExecutionId;

const OBJECT_TYPE_MAX_LEN: usize = 64;
const OBJECT_ID_MAX_LEN: usize = 128;
const RESPONSIBILITY_KEY_MAX_LEN: usize = 128;
const RESPONSIBILITY_SCOPE_MAX_ITEMS: usize = 200;
const SUBJECT_VERSION_MAX_LEN: usize = 128;
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
    /// 销售单生效后的采购建单。
    ProcurementOrderCreation,
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
    /// 通用单据审批任务。
    DocumentApproval,
}

impl WorkItemType {
    /// 返回面向用户的任务类型标签。
    ///
    /// # 返回
    /// 返回稳定中文展示名。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ProcurementOrderCreation => "采购建单",
            Self::PurchaseOrderReview => "采购单财务审核",
            Self::SalesChangeImpactReview => "销售变更履约影响复核",
            Self::SalesChangeFinanceReview => "销售变更财务影响复核",
            Self::CardFundsReview => "卡券票款复核",
            Self::CardFundsDeltaReview => "卡券票款差异复核",
            Self::OwnershipMigrationSalesConfirmation => "归属迁移销售确认",
            Self::OwnershipMigrationFinanceConfirmation => "归属迁移财务确认",
            Self::InventoryAdjustmentReview => "库存调整复核",
            Self::FinanceCorrectionReview => "财务纠错复核",
            Self::SupplierSettlementReview => "供应商结算复核",
            Self::ImportBusinessConfirmation => "导入业务确认",
            Self::IntegrationResultUnknown => "集成结果未知",
            Self::BusinessException => "业务异常",
            Self::DocumentApproval => "单据审批",
        }
    }

    /// 返回任务类型的持久化代码。
    ///
    /// # 返回
    /// 返回 `SCREAMING_SNAKE_CASE` 稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProcurementOrderCreation => "PROCUREMENT_ORDER_CREATION",
            Self::PurchaseOrderReview => "PURCHASE_ORDER_REVIEW",
            Self::SalesChangeImpactReview => "SALES_CHANGE_IMPACT_REVIEW",
            Self::SalesChangeFinanceReview => "SALES_CHANGE_FINANCE_REVIEW",
            Self::CardFundsReview => "CARD_FUNDS_REVIEW",
            Self::CardFundsDeltaReview => "CARD_FUNDS_DELTA_REVIEW",
            Self::OwnershipMigrationSalesConfirmation => "OWNERSHIP_MIGRATION_SALES_CONFIRMATION",
            Self::OwnershipMigrationFinanceConfirmation => "OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION",
            Self::InventoryAdjustmentReview => "INVENTORY_ADJUSTMENT_REVIEW",
            Self::FinanceCorrectionReview => "FINANCE_CORRECTION_REVIEW",
            Self::SupplierSettlementReview => "SUPPLIER_SETTLEMENT_REVIEW",
            Self::ImportBusinessConfirmation => "IMPORT_BUSINESS_CONFIRMATION",
            Self::IntegrationResultUnknown => "INTEGRATION_RESULT_UNKNOWN",
            Self::BusinessException => "BUSINESS_EXCEPTION",
            Self::DocumentApproval => "DOCUMENT_APPROVAL",
        }
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

/// 当前个人责任的已注册形成来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssignmentSource {
    /// 独立任务的固定系统规则。
    SystemRule,
    /// 管理员受控转交。
    AdminReassign,
    /// 审批运行时指定到人。
    ApprovalRuntime,
}

impl AssignmentSource {
    /// 返回责任来源的持久化代码。
    ///
    /// # 返回
    /// 返回已注册的稳定来源代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SystemRule => "SYSTEM_RULE",
            Self::AdminReassign => "ADMIN_REASSIGN",
            Self::ApprovalRuntime => "APPROVAL_RUNTIME",
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

/// 通用单据审批任务的创建数据。责任人、角色、组织和执行 ID 均必填。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentApprovalWorkItemData {
    /// 当前节点执行。
    pub approval_node_execution_id: ApprovalNodeExecutionId,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 被审批的冻结提交版本。
    pub subject_version: String,
    /// 合同签署的责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前实例审批人。
    pub owner_user_id: String,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
}

/// 创建任务所需的责任与业务对象快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemData {
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 被处理的不可变提交或业务版本。
    pub subject_version: String,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人；创建开放任务时必填。
    pub owner_user_id: String,
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
    /// 类型化审批节点执行；审批任务必填。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_node_execution_id: Option<ApprovalNodeExecutionId>,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 服务端冻结的可选责任维度；普通任务为空，存在时参与开放任务唯一性。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    responsibility_key: Option<String>,
    /// 服务端冻结的稳定业务行范围；普通任务为空。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    responsibility_scope_ids: Vec<String>,
    /// 被处理的不可变提交或业务版本。
    pub subject_version: String,
    /// 生命周期状态。
    pub status: WorkItemStatus,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人；开放任务必填。
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
    /// 开放任务必须给出唯一责任人，并立即形成 `assigned_at` 与
    /// `current_assignment_at`。
    ///
    /// # 错误
    /// 必填字段为空、字段超长或缺少个人责任人时返回错误。
    pub fn new(id: WorkItemId, data: WorkItemData) -> Result<Self> {
        Self::new_at_with_optional_responsibility(id, data, None, Vec::new(), Instant::now())
    }

    /// 创建带服务端责任维度的任务。
    ///
    /// 责任维度在创建时规范化并冻结；后续转交不得修改。
    /// 客户端输入不得直接调用本入口，应用服务只能传入已注册的固定维度。
    ///
    /// # 错误
    /// 责任维度为空、字段超长，或任务基础数据无效时返回错误。
    pub fn new_with_responsibility_key(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: impl Into<String>,
    ) -> Result<Self> {
        let responsibility_key = normalize_responsibility_key(responsibility_key.into())?;
        Self::new_at_with_optional_responsibility(
            id,
            data,
            Some(responsibility_key),
            Vec::new(),
            Instant::now(),
        )
    }

    /// 创建带服务端责任维度与稳定业务行范围的任务。
    ///
    /// # 参数
    /// * `id` - 任务主键
    /// * `data` - 任务责任与业务对象快照
    /// * `responsibility_key` - 参与开放唯一性的服务端稳定键
    /// * `scope_ids` - 已由服务端解析、跨版本稳定的业务行 ID
    ///
    /// # 返回
    /// 返回责任键和责任行集合均已规范化、排序并冻结的开放任务。
    ///
    /// # 错误
    /// 责任键或行 ID 为空、字段过长、行数超过上限，或任务基础数据无效时返回错误。
    pub fn new_with_responsibility_scope(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: impl Into<String>,
        scope_ids: Vec<String>,
    ) -> Result<Self> {
        let responsibility_key = normalize_responsibility_key(responsibility_key.into())?;
        let scope_ids = normalize_responsibility_scope(scope_ids)?;
        Self::new_at_with_optional_responsibility(
            id,
            data,
            Some(responsibility_key),
            scope_ids,
            Instant::now(),
        )
    }

    /// 使用确定时间创建任务，供事务编排和确定性测试使用。
    ///
    /// # 错误
    /// 必填字段为空、字段超长或分派模式与个人责任不匹配时返回错误。
    pub fn new_at(id: WorkItemId, data: WorkItemData, at: Instant) -> Result<Self> {
        Self::new_at_with_optional_responsibility(id, data, None, Vec::new(), at)
    }

    /// 创建指定到人的单据审批任务。
    ///
    /// 必须同时提供非空责任人、角色、组织、审批运行时来源和节点执行 ID。
    ///
    /// # 参数
    /// * `id` - 任务主键
    /// * `data` - 审批任务数据
    /// * `at` - 创建时间
    ///
    /// # 错误
    /// 任一必填责任字段为空或超长时返回错误。
    pub fn new_document_approval(
        id: WorkItemId,
        data: DocumentApprovalWorkItemData,
        at: Instant,
    ) -> Result<Self> {
        let owner_user_id = normalize_required_text(
            data.owner_user_id,
            "审批任务责任人不能为空",
            USER_ID_MAX_LEN,
            "审批任务责任人过长",
        )?;
        let generic = WorkItemData {
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            business_object_type: data.business_object_type,
            business_object_id: data.business_object_id,
            subject_version: data.subject_version,
            owner_role: data.owner_role,
            owner_organization_id: data.owner_organization_id,
            owner_user_id: owner_user_id.clone(),
            assignment_source: AssignmentSource::ApprovalRuntime,
            priority: data.priority,
            due_at: data.due_at,
            reason_code: None,
            impact_summary: None,
        };
        let mut item = Self::new_at_with_optional_responsibility(id, generic, None, Vec::new(), at)?;
        item.work_item_type = WorkItemType::DocumentApproval;
        item.approval_node_execution_id = Some(data.approval_node_execution_id);
        item.assignment_source = AssignmentSource::ApprovalRuntime;
        Ok(item)
    }

    /// 使用已规范化的可选责任键和责任范围创建任务。
    ///
    /// # 参数
    /// * `id` - 任务主键
    /// * `data` - 任务基础数据
    /// * `responsibility_key` - 可选服务端责任键
    /// * `responsibility_scope_ids` - 已规范化的稳定业务行集合
    /// * `at` - 责任形成时间
    ///
    /// # 返回
    /// 返回初始状态为开放且已指定到人的任务。
    ///
    /// # 错误
    /// 任务基础数据不合法或误用通用路径创建审批任务时返回错误。
    fn new_at_with_optional_responsibility(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: Option<String>,
        responsibility_scope_ids: Vec<String>,
        at: Instant,
    ) -> Result<Self> {
        let normalized = NormalizedWorkItemData::try_from(data)?;
        if normalized.work_item_type == WorkItemType::DocumentApproval {
            return Err(Error::from("单据审批任务必须使用专用构造路径"));
        }
        if normalized.work_item_type == WorkItemType::ProcurementOrderCreation
            && (responsibility_key.is_none() || responsibility_scope_ids.is_empty())
        {
            return Err(Error::from("采购建单任务必须冻结责任键和责任行范围"));
        }
        if normalized.work_item_type != WorkItemType::ProcurementOrderCreation
            && !responsibility_scope_ids.is_empty()
        {
            return Err(Error::from("只有采购建单任务可以冻结责任行范围"));
        }
        let responsibility_actor_ids = vec![normalized.owner_user_id.clone()];
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            work_item_type: normalized.work_item_type,
            approval_node_execution_id: None,
            business_object_type: normalized.business_object_type,
            business_object_id: normalized.business_object_id,
            responsibility_key,
            responsibility_scope_ids,
            subject_version: normalized.subject_version,
            status: WorkItemStatus::Open,
            owner_role: normalized.owner_role,
            owner_organization_id: normalized.owner_organization_id,
            owner_user_id: Some(normalized.owner_user_id),
            responsibility_actor_ids,
            assignment_source: normalized.assignment_source,
            assigned_at: Some(at),
            started_at: None,
            current_assignment_at: Some(at),
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

    /// 返回创建时冻结的稳定业务行范围。
    ///
    /// # 返回
    /// 返回按稳定 ID 排序并去重的只读切片；普通任务返回空切片。
    pub fn responsibility_scope_ids(&self) -> &[String] {
        &self.responsibility_scope_ids
    }

    /// 更新开放任务的业务影响摘要。
    ///
    /// # 参数
    /// * `impact_summary` - 面向用户的最新业务影响；空白值会规范化为空
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 任务非开放或摘要超过长度上限时返回错误。
    pub fn update_impact_summary(&mut self, impact_summary: Option<String>) -> Result<()> {
        self.ensure_open()?;
        self.impact_summary = normalize_optional_text(impact_summary, "影响摘要", IMPACT_SUMMARY_MAX_LEN)?;
        Ok(())
    }

    /// 在业务需求已由系统事实完全满足时自动完成开放任务。
    ///
    /// # 参数
    /// * `at` - 系统确认需求归零的时间
    ///
    /// # 返回
    /// 自动完成成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 单据审批任务或非开放任务不能使用本入口。
    pub fn complete_when_requirement_satisfied(&mut self, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        if self.work_item_type != WorkItemType::ProcurementOrderCreation {
            return Err(Error::from("只有采购建单任务可以按需求归零自动完成"));
        }
        self.ensure_open()?;
        self.status = WorkItemStatus::Completed;
        self.completed_at = Some(at);
        self.completed_by = Some("__system__".to_string());
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 为重新释放的采购需求创建一条新的开放任务。
    ///
    /// # 参数
    /// * `id` - 新任务主键
    /// * `subject_version` - 重新释放时的销售当前版本
    /// * `impact_summary` - 重新释放后的剩余数量摘要
    ///
    /// # 返回
    /// 返回复制当前责任人、责任来源和冻结行范围的新开放任务。
    ///
    /// # 错误
    /// 当前任务不是采购建单终态、缺少责任人或冻结责任事实时返回错误。
    ///
    /// # 关键业务约束
    /// 历史终态任务保持不变；重新释放必须形成新任务身份。
    pub fn successor_for_released_requirement(
        &self,
        id: WorkItemId,
        subject_version: String,
        impact_summary: Option<String>,
    ) -> Result<Self> {
        if self.work_item_type != WorkItemType::ProcurementOrderCreation {
            return Err(Error::from("只有采购建单任务可以创建释放后继任务"));
        }
        if self.status == WorkItemStatus::Open {
            return Err(Error::from("开放采购建单任务不能创建释放后继任务"));
        }
        let responsibility_key = self
            .responsibility_key()
            .ok_or_else(|| Error::from("历史采购建单任务缺少责任键"))?
            .to_string();
        let owner_user_id = self
            .owner_user_id
            .clone()
            .ok_or_else(|| Error::from("历史采购建单任务缺少具体责任人"))?;
        Self::new_with_responsibility_scope(
            id,
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: self.business_object_type.clone(),
                business_object_id: self.business_object_id.clone(),
                subject_version,
                owner_role: self.owner_role.clone(),
                owner_organization_id: self.owner_organization_id.clone(),
                owner_user_id,
                assignment_source: self.assignment_source,
                priority: self.priority,
                due_at: self.due_at,
                reason_code: Some("PROCUREMENT_QUANTITY_RELEASED".to_string()),
                impact_summary,
            },
            responsibility_key,
            self.responsibility_scope_ids.clone(),
        )
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

    /// 转交开放任务的当前个人责任。
    ///
    /// 首次分派时间只在此前从未形成个人责任时写入；首次处理时间保持不变。
    /// 调用方必须在应用层重新校验目标任职、角色、数据范围与岗位分离。
    ///
    /// # 错误
    /// 任务非开放或目标用户为空、超长时返回错误。
    pub fn reassign(&mut self, target_user_id: impl Into<String>, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        self.assign_to(target_user_id, AssignmentSource::AdminReassign, at)
    }

    /// 由强类型领域命令完成当前开放任务。
    ///
    /// 本方法只形成任务事实；调用方必须把正式领域事实、审批推进和本实体写入
    /// 放在同一事务。完成动作同时按 `if_null` 语义形成首次处理时间。
    ///
    /// # 错误
    /// 任务非开放、没有个人责任或执行人不是当前责任人时返回错误。
    pub fn complete_by_domain_command(&mut self, completed_by: impl Into<String>, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        self.complete_open(completed_by, at)
    }

    /// 由审批运行时完成当前开放的单据审批任务。
    ///
    /// # 错误
    /// 不是单据审批任务、任务非开放或执行人不是当前责任人时返回错误。
    pub fn complete_by_approval_runtime(
        &mut self,
        completed_by: impl Into<String>,
        at: Instant,
    ) -> Result<()> {
        self.ensure_document_approval()?;
        self.complete_open(completed_by, at)
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
        self.ensure_generic_mutation()?;
        self.close_open(closed_by, data, at)
    }

    /// 由审批运行时关闭当前开放的单据审批任务。
    ///
    /// # 错误
    /// 不是单据审批任务、任务非开放或关闭数据非法时返回错误。
    pub fn close_by_approval_runtime(
        &mut self,
        closed_by: impl Into<String>,
        data: WorkItemCloseData,
        at: Instant,
    ) -> Result<()> {
        self.ensure_document_approval()?;
        self.close_open(closed_by, data, at)
    }

    fn close_open(
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

    fn complete_open(&mut self, completed_by: impl Into<String>, at: Instant) -> Result<()> {
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

    fn ensure_generic_mutation(&self) -> Result<()> {
        if self.work_item_type == WorkItemType::DocumentApproval {
            return Err(Error::from("APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN"));
        }
        Ok(())
    }

    fn ensure_document_approval(&self) -> Result<()> {
        if self.work_item_type != WorkItemType::DocumentApproval {
            return Err(Error::from("只有单据审批任务可以由审批运行时完成或关闭"));
        }
        Ok(())
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

/// 规范化参与开放任务唯一性的服务端责任键。
///
/// # 参数
/// * `responsibility_key` - 原始责任键
///
/// # 返回
/// 返回去除首尾空白后的非空责任键。
///
/// # 错误
/// 责任键为空或超过长度上限时返回错误。
fn normalize_responsibility_key(responsibility_key: String) -> Result<String> {
    normalize_required_text(
        responsibility_key,
        "责任维度不能为空",
        RESPONSIBILITY_KEY_MAX_LEN,
        "责任维度过长",
    )
}

/// 规范化服务端冻结的稳定业务行范围。
///
/// # 参数
/// * `scope_ids` - 原始稳定业务行 ID 集合
///
/// # 返回
/// 返回逐项去空白、排序并去重后的非空集合。
///
/// # 错误
/// 集合为空、超过最大行数，或任一 ID 为空或过长时返回错误。
fn normalize_responsibility_scope(scope_ids: Vec<String>) -> Result<Vec<String>> {
    if scope_ids.is_empty() || scope_ids.len() > RESPONSIBILITY_SCOPE_MAX_ITEMS {
        return Err(Error::from("责任范围行数必须在1-200之间"));
    }
    let mut normalized = scope_ids
        .into_iter()
        .map(|scope_id| {
            normalize_required_text(
                scope_id,
                "责任范围行不能为空",
                OBJECT_ID_MAX_LEN,
                "责任范围行过长",
            )
        })
        .collect::<Result<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

struct NormalizedWorkItemData {
    work_item_type: WorkItemType,
    business_object_type: String,
    business_object_id: String,
    subject_version: String,
    owner_role: String,
    owner_organization_id: String,
    owner_user_id: String,
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
            owner_user_id: normalize_required_text(
                data.owner_user_id,
                "责任人不能为空",
                USER_ID_MAX_LEN,
                "责任人过长",
            )?,
            assignment_source: data.assignment_source,
            priority: data.priority,
            due_at: data.due_at,
            reason_code: normalize_optional_text(data.reason_code, "原因代码", REASON_CODE_MAX_LEN)?,
            impact_summary: normalize_optional_text(data.impact_summary, "影响摘要", IMPACT_SUMMARY_MAX_LEN)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType};
    use crate::common::state::ensure_transition;
    use crate::common::time::Instant;
    use crate::ids::WorkItemId;

    fn direct_data() -> WorkItemData {
        WorkItemData {
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            business_object_type: " LEGACY_IMPORT_BATCH ".to_string(),
            business_object_id: " batch-1 ".to_string(),
            subject_version: " v3 ".to_string(),
            owner_role: " sales ".to_string(),
            owner_organization_id: " org-1 ".to_string(),
            owner_user_id: " alice ".to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: Some(Instant::from_unix_secs(1_700_086_400)),
            reason_code: Some("IMPORT_READY".to_string()),
            impact_summary: Some(" 待确认导入范围 ".to_string()),
        }
    }

    #[test]
    fn open_task_requires_personal_owner() {
        let item = WorkItem::new_at(
            WorkItemId::new("wi-1"),
            direct_data(),
            Instant::from_unix_secs(100),
        )
        .unwrap();
        assert_eq!(item.status, WorkItemStatus::Open);
        assert_eq!(item.owner_user_id.as_deref(), Some("alice"));
        assert_eq!(item.responsibility_actor_ids, vec!["alice".to_string()]);
        let missing = WorkItemData {
            owner_user_id: "   ".to_string(),
            ..direct_data()
        };
        assert!(WorkItem::new_at(WorkItemId::new("wi-2"), missing, Instant::from_unix_secs(100)).is_err());
    }

    #[test]
    fn reassign_and_complete_preserve_first_times() {
        let first = Instant::from_unix_secs(100);
        let mut item = WorkItem::new_at(WorkItemId::new("wi-1"), direct_data(), first).unwrap();
        item.record_activity("alice", Instant::from_unix_secs(110))
            .unwrap();
        item.reassign("bob", Instant::from_unix_secs(130)).unwrap();
        item.complete_by_domain_command("bob", Instant::from_unix_secs(150))
            .unwrap();
        assert_eq!(item.assigned_at, Some(first));
        assert_eq!(item.started_at, Some(Instant::from_unix_secs(110)));
        assert_eq!(item.owner_user_id.as_deref(), Some("bob"));
        assert_eq!(item.assignment_source, AssignmentSource::AdminReassign);
        assert!(item.is_terminal());
    }

    #[test]
    fn procurement_task_cannot_bypass_frozen_scope_constructor() {
        let data = WorkItemData {
            work_item_type: WorkItemType::ProcurementOrderCreation,
            ..direct_data()
        };
        assert!(WorkItem::new_at(
            WorkItemId::new("wi-procurement"),
            data.clone(),
            Instant::from_unix_secs(100),
        )
        .is_err());
        assert!(WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-procurement-key"),
            data,
            "sales-lines:key",
        )
        .is_err());
    }

    #[test]
    fn responsibility_scope_is_normalized_and_system_completion_preserves_history() {
        let mut item = WorkItem::new_with_responsibility_scope(
            WorkItemId::new("wi-procurement"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                ..direct_data()
            },
            " sales-lines:key ",
            vec![" line-b ".to_string(), "line-a".to_string(), "line-a".to_string()],
        )
        .unwrap();
        assert_eq!(item.responsibility_key(), Some("sales-lines:key"));
        assert_eq!(
            item.responsibility_scope_ids(),
            &["line-a".to_string(), "line-b".to_string()]
        );
        item.update_impact_summary(Some(" 剩余 6 件待采购 ".to_string()))
            .unwrap();
        assert_eq!(item.impact_summary.as_deref(), Some("剩余 6 件待采购"));
        item.complete_when_requirement_satisfied(Instant::from_unix_secs(120))
            .unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(item.completed_by.as_deref(), Some("__system__"));
        assert_eq!(item.owner_user_id.as_deref(), Some("alice"));
        assert!(item
            .complete_when_requirement_satisfied(Instant::from_unix_secs(130))
            .is_err());

        let successor = item
            .successor_for_released_requirement(
                WorkItemId::new("wi-procurement-released"),
                "sales-revision-2".to_string(),
                Some("剩余 4 件待采购".to_string()),
            )
            .unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(successor.base.id, "wi-procurement-released");
        assert_eq!(successor.status, WorkItemStatus::Open);
        assert_eq!(successor.subject_version, "sales-revision-2");
        assert_eq!(successor.owner_user_id.as_deref(), Some("alice"));
        assert_eq!(successor.assignment_source, AssignmentSource::SystemRule);
        assert_eq!(successor.responsibility_key(), Some("sales-lines:key"));
        assert_eq!(
            successor.responsibility_scope_ids(),
            item.responsibility_scope_ids()
        );
        assert_eq!(
            successor.reason_code.as_deref(),
            Some("PROCUREMENT_QUANTITY_RELEASED")
        );
        assert_eq!(successor.impact_summary.as_deref(), Some("剩余 4 件待采购"));
    }

    #[test]
    fn responsibility_scope_rejects_empty_lines() {
        assert!(WorkItem::new_with_responsibility_scope(
            WorkItemId::new("wi-empty"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                ..direct_data()
            },
            "sales-lines:key",
            Vec::new(),
        )
        .is_err());
    }

    #[test]
    fn codes_and_bson_shape_are_stable() {
        assert_eq!(AssignmentSource::SystemRule.as_str(), "SYSTEM_RULE");
        assert_eq!(WorkItemType::DocumentApproval.as_str(), "DOCUMENT_APPROVAL");
        let item = WorkItem::new_at(
            WorkItemId::new("wi-1"),
            direct_data(),
            Instant::from_unix_secs(100),
        )
        .unwrap();
        let document = bson::to_document(&item).unwrap();
        assert_eq!(document.get_str("status").unwrap(), "OPEN");
        let roundtrip: WorkItem = bson::from_document(document).unwrap();
        assert_eq!(roundtrip, item);
    }

    #[test]
    fn document_approval_requires_owner_and_execution() {
        let at = Instant::from_unix_secs(100);
        let mut item = WorkItem::new_document_approval(
            WorkItemId::new("wi-approval"),
            super::DocumentApprovalWorkItemData {
                approval_node_execution_id: bpm::ApprovalNodeExecutionId::new("exec-1"),
                business_object_type: "stock_adjustment".into(),
                business_object_id: "adj-1".into(),
                subject_version: "1".into(),
                owner_role: "stock_adjustment_approver".into(),
                owner_organization_id: "org-1".into(),
                owner_user_id: " alice ".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            at,
        )
        .unwrap();
        assert_eq!(item.work_item_type, WorkItemType::DocumentApproval);
        assert!(item.reassign("bob", Instant::from_unix_secs(110)).is_err());
        item.complete_by_approval_runtime("alice", Instant::from_unix_secs(110))
            .unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert!(ensure_transition(WorkItemStatus::Open, WorkItemStatus::Completed).is_ok());
    }
}
