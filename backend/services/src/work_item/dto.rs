//! D03 人工任务责任队列的 HTTP 共用 DTO。

use entities::work_item::{
    AssignmentMode, AssignmentSource, WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::brief::assemble_brief;
use super::presentation::{
    next_action_hint, reason_label, usable_impact_summary, UNRESOLVED_OWNER_DISPLAY_NAME,
};
use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

const DEFAULT_TIMEZONE: &str = "Asia/Shanghai";
const WORK_ITEM_TYPES: [WorkItemType; 16] = [
    WorkItemType::DocumentApproval,
    WorkItemType::ProcurementConfirmation,
    WorkItemType::LowMarginManagerConfirmation,
    WorkItemType::PurchaseOrderReview,
    WorkItemType::SalesChangeImpactReview,
    WorkItemType::SalesChangeFinanceReview,
    WorkItemType::CardFundsReview,
    WorkItemType::CardFundsDeltaReview,
    WorkItemType::OwnershipMigrationSalesConfirmation,
    WorkItemType::OwnershipMigrationFinanceConfirmation,
    WorkItemType::InventoryAdjustmentReview,
    WorkItemType::FinanceCorrectionReview,
    WorkItemType::SupplierSettlementReview,
    WorkItemType::ImportBusinessConfirmation,
    WorkItemType::IntegrationResultUnknown,
    WorkItemType::BusinessException,
];

/// 责任队列固定范围。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemScope {
    /// 当前用户已负责的开放任务。
    Mine,
    /// 当前用户有资格开始处理的开放责任池任务。
    Team,
    /// 主管授权组织内全部开放任务。
    Managed,
    /// 当前用户参与过的已完成或已关闭任务。
    History,
}

impl WorkItemScope {
    /// 返回稳定范围代码。
    ///
    /// # 返回
    /// 返回 `mine`、`team`、`managed` 或 `history`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Team => "team",
            Self::Managed => "managed",
            Self::History => "history",
        }
    }
}

/// 任务族筛选。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemFamily {
    /// 审批与确认。
    Approval,
    /// 财务处理。
    Finance,
    /// 履约处理。
    Fulfillment,
    /// 异常与补偿。
    Exception,
}

impl WorkItemFamily {
    /// 返回该任务族的服务端注册任务类型。
    ///
    /// # 返回
    /// 返回不可由客户端扩展的任务类型集合。
    pub(crate) fn work_item_types(self) -> Vec<WorkItemType> {
        WORK_ITEM_TYPES
            .into_iter()
            .filter(|work_item_type| family_of(*work_item_type) == self)
            .collect()
    }
}

/// 到期时间筛选。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemDueFilter {
    /// 当前业务时区今天到期。
    Today,
    /// 当前业务时区今天之前到期。
    Overdue,
}

/// 队列排序。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemSort {
    /// 优先按时限排序；优先级仍由任务行展示。
    PriorityDue,
    /// 到期时间升序。
    DueAsc,
    /// 创建时间倒序。
    CreatedDesc,
}

/// 队列查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WorkItemListParams {
    /// 必填责任范围。
    pub scope: WorkItemScope,
    /// 可选任务族。
    pub family: Option<WorkItemFamily>,
    /// 可选固定任务类型。
    pub work_item_type: Option<WorkItemType>,
    /// 历史状态筛选；开放范围只允许 `OPEN`。
    pub status: Option<WorkItemStatus>,
    /// 到期时间筛选。
    pub due: Option<WorkItemDueFilter>,
    /// 逗号分隔优先级序号，1 至 4 对应紧急至低。
    pub priorities: Option<String>,
    /// 在授权结果内按固定安全摘要字段检索。
    #[validate(length(max = 128, message = "检索词不能超过128个字符"))]
    pub q: Option<String>,
    /// 排序方式。
    pub sort: Option<WorkItemSort>,
    /// 服务端返回的队列上下文；当前实现只接受同查询重算值。
    pub queue_context_id: Option<String>,
    /// 希望聚焦的任务；不可见时服务端失败关闭。
    #[validate(length(max = 128, message = "焦点任务ID不能超过128个字符"))]
    pub current_work_item_id: Option<String>,
    /// IANA 时区；当前版本固定支持 `Asia/Shanghai`。
    pub timezone: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1 至 100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
}

/// Service 使用的规范化队列查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkItemListQuery {
    pub scope: WorkItemScope,
    pub work_item_types: Vec<WorkItemType>,
    pub statuses: Vec<WorkItemStatus>,
    pub due: Option<WorkItemDueFilter>,
    pub priorities: Vec<WorkItemPriority>,
    pub query: Option<String>,
    pub current_work_item_id: Option<String>,
    pub sort_by: &'static str,
    pub sort_ascending: bool,
    pub page: u64,
    pub page_size: u32,
    pub queue_context_id: Option<String>,
}

impl WorkItemListParams {
    /// 校验并规范化责任队列查询。
    ///
    /// # 返回
    /// 返回不包含客户端责任过滤条件的服务端查询事实。
    ///
    /// # 错误
    /// scope/status 不兼容、时区或暂不支持的查询参数非法时返回验证错误。
    pub(crate) fn normalized(&self) -> Result<WorkItemListQuery> {
        let statuses = normalize_statuses(self.scope, self.status)?;
        let work_item_types = normalize_work_item_types(self.family, self.work_item_type)?;
        let priorities = parse_priorities(self.priorities.as_deref())?;
        ensure_supported_query(self)?;
        let (sort_by, sort_ascending) = normalize_sort(self.sort);
        Ok(WorkItemListQuery {
            scope: self.scope,
            work_item_types,
            statuses,
            due: self.due,
            priorities,
            query: normalized_text(self.q.as_deref()),
            current_work_item_id: normalized_text(self.current_work_item_id.as_deref()),
            sort_by,
            sort_ascending,
            page: page_or_default(self.page),
            page_size: page_size_or_default(self.page_size),
            queue_context_id: normalized_text(self.queue_context_id.as_deref()),
        })
    }
}

/// 分页责任队列响应。
#[derive(Debug, Clone, Serialize)]
pub struct WorkItemPageView {
    /// 当前页任务。
    pub items: Vec<WorkItemView>,
    /// 授权范围内总数。
    pub total: i64,
    /// 当前页码。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 服务端形成的稳定队列上下文。
    pub queue_context_id: String,
}

/// 待办统计查询参数。
///
/// 统计与正式列表复用同一责任范围、任务族、类型、时限和工作时区语义；
/// 不接受分页、自由检索或客户端责任人条件。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct WorkItemStatsParams {
    /// 指标预警所依据的当前责任范围。
    pub scope: WorkItemScope,
    /// 可选任务族。
    pub family: Option<WorkItemFamily>,
    /// 可选固定任务类型。
    pub work_item_type: Option<WorkItemType>,
    /// 可选时限筛选。
    pub due: Option<WorkItemDueFilter>,
    /// IANA 时区；当前版本固定支持 `Asia/Shanghai`。
    pub timezone: Option<String>,
}

impl WorkItemStatsParams {
    /// 复用正式队列规范化逻辑形成服务端统计查询。
    ///
    /// # 错误
    /// 任务族与类型冲突或时区不受支持时返回验证错误。
    pub(crate) fn normalized(&self) -> Result<WorkItemListQuery> {
        WorkItemListParams {
            scope: self.scope,
            family: self.family,
            work_item_type: self.work_item_type,
            status: None,
            due: self.due,
            priorities: None,
            q: None,
            sort: Some(WorkItemSort::CreatedDesc),
            queue_context_id: None,
            current_work_item_id: None,
            timezone: self.timezone.clone(),
            page: Some(1),
            page_size: Some(100),
        }
        .normalized()
    }
}

/// 服务端权限过滤后的待办统计。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemStatsView {
    /// 已分配给当前用户的开放任务总数。
    pub assigned: u64,
    /// 当前用户有资格开始处理的团队责任池任务总数。
    pub team: u64,
    /// 当前选中责任范围内、工作时区今天到期的任务数。
    pub due_today: u64,
    /// 当前选中责任范围内、截止时间早于统计时点的开放任务数。
    pub overdue: u64,
    /// 当前选中责任范围内的结果未知与业务异常任务数。
    pub exception: u64,
    /// 服务端统计时点。
    pub as_of: entities::common::time::Instant,
}

/// 当前处理状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcessingState {
    /// 责任和审批步骤允许继续处理。
    Ready,
    /// 审批步骤受阻，普通责任动作必须为空。
    ApprovalBlocked,
}

/// 权限安全的任务阻断摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcessingBlockerView {
    /// 稳定阻断码。
    pub code: String,
    /// 面向用户且不泄露内部细节的说明。
    pub message: String,
}

/// 服务端计算的允许动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemAllowedAction {
    /// 查看对象。
    View,
    /// 进入固定强类型处理器。
    Process,
    /// 从责任池建立本人责任。
    StartProcessing,
    /// 退回责任池。
    ReleaseToTeam,
    /// 受控转交。
    Reassign,
    /// 受控关闭无效任务。
    Close,
}

/// 用户或组织安全摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemPartyView {
    /// 稳定身份。
    pub id: String,
    /// 权限安全的展示名。
    pub display_name: String,
}

/// 事项简报中的只读键值。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemSummarySection {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric: Option<bool>,
}

/// 事项简报中的一行业务明细。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemBriefLine {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_label: Option<String>,
}

/// 受控路由上下文。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemRouteContext {
    /// 导入确认范围；不适用时为空。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_scope: Option<String>,
    /// 单据审批的 DocumentType 稳定代码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_type: Option<String>,
}

/// 人工任务队列安全投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemView {
    pub id: String,
    pub work_item_type: WorkItemType,
    pub handler_key: String,
    pub destination_workspace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_context: Option<WorkItemRouteContext>,
    pub approval_step_instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_node_execution_id: Option<String>,
    pub status: WorkItemStatus,
    pub assignment_mode: AssignmentMode,
    pub assignment_source: AssignmentSource,
    pub owner_role: String,
    pub owner_role_label: String,
    pub owner_organization_id: String,
    pub owner_organization: WorkItemPartyView,
    pub owner_user_id: Option<String>,
    pub owner_user: Option<WorkItemPartyView>,
    pub processing_state: ProcessingState,
    pub processing_blocker: Option<ProcessingBlockerView>,
    pub business_object_type: String,
    pub business_object_id: String,
    /// 业务对象所属的工作面根对象；与任务对象相同时返回同一 ID。
    pub root_business_object_id: String,
    pub business_object_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterparty_label: Option<String>,
    pub next_action_hint: String,
    pub summary_sections: Vec<WorkItemSummarySection>,
    pub brief_lines: Vec<WorkItemBriefLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brief_more_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_summary: Option<String>,
    pub subject_version: String,
    pub task_version: String,
    pub allowed_actions: Vec<WorkItemAllowedAction>,
    pub action_blockers: Vec<ProcessingBlockerView>,
    pub priority: WorkItemPriority,
    pub due_at: Option<u64>,
    pub reason_code: Option<String>,
    pub reason_label: String,
    pub impact_summary: String,
    pub assigned_at: Option<u64>,
    pub started_at: Option<u64>,
    pub current_assignment_at: Option<u64>,
    pub last_activity_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub completed_by: Option<String>,
    pub closed_at: Option<u64>,
    pub closed_by: Option<String>,
    pub close_reason: Option<String>,
    pub created_at: u64,
    pub queue_context_id: String,
}

/// 责任命令的稳定冲突分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkItemConflictKind {
    /// 客户端提交的任务版本已经陈旧。
    Version,
    /// 任务当前责任已经由其他操作改变。
    Responsibility,
}

impl WorkItemConflictKind {
    /// 返回 HTTP 契约使用的稳定错误码。
    ///
    /// # 返回
    /// 返回不依赖展示文案的冲突代码。
    pub fn code(self) -> &'static str {
        match self {
            Self::Version => "WORK_ITEM_VERSION_CONFLICT",
            Self::Responsibility => "WORK_ITEM_RESPONSIBILITY_CONFLICT",
        }
    }

    /// 返回权限安全的用户提示。
    ///
    /// # 返回
    /// 返回不包含处理人 ID 或内部版本细节的提示。
    pub fn message(self) -> &'static str {
        match self {
            Self::Version => "任务已被其他操作更新，请按最新状态重试",
            Self::Responsibility => "任务责任已变化，请按最新状态处理",
        }
    }
}

/// 责任命令冲突时返回的权限安全数据。
///
/// `current_work_item` 必须由 Service 使用当前 actor 重新投影；若最新任务
/// 已不在 actor 的查看范围内，则固定返回 `null`，不得降级返回原始实体。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemConflict {
    #[serde(skip)]
    kind: WorkItemConflictKind,
    /// 当前仍可见时的最新安全投影；不可见或已删除时为空。
    pub current_work_item: Option<WorkItemView>,
}

impl WorkItemConflict {
    /// 创建责任命令冲突数据。
    ///
    /// # 参数
    /// * `kind` - 稳定冲突分类
    /// * `current_work_item` - actor 重新授权后的最新安全投影
    ///
    /// # 返回
    /// 返回可直接放入 409 响应 `data` 的冲突数据。
    pub fn new(kind: WorkItemConflictKind, current_work_item: Option<WorkItemView>) -> Self {
        Self {
            kind,
            current_work_item,
        }
    }

    /// 返回稳定冲突分类。
    ///
    /// # 返回
    /// 返回版本冲突或责任冲突。
    pub fn kind(&self) -> WorkItemConflictKind {
        self.kind
    }
}

/// 责任命令的服务端结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkItemMutationOutcome {
    /// 命令已应用并返回更新后的安全投影。
    Applied(WorkItemView),
    /// 命令因并发版本或责任变化未应用。
    Conflict(WorkItemConflict),
}

impl WorkItemView {
    /// 设置服务端完成的处理状态与动作判断。
    ///
    /// # 返回
    /// 返回更新后的投影。
    pub(crate) fn with_access(
        mut self,
        processing_state: ProcessingState,
        processing_blocker: Option<ProcessingBlockerView>,
        allowed_actions: Vec<WorkItemAllowedAction>,
        action_blockers: Vec<ProcessingBlockerView>,
    ) -> Self {
        self.processing_state = processing_state;
        self.processing_blocker = processing_blocker;
        self.allowed_actions = allowed_actions;
        self.action_blockers = action_blockers;
        self
    }

    /// 由已授权字段生成队列安全投影。
    ///
    /// # 参数
    /// * `fields` - 已通过对象授权的任务字段
    /// * `queue_context_id` - 当前队列上下文
    ///
    /// # 返回
    /// 返回原因、影响和下一步均已翻译成业务语言的投影；处理人姓名仍可能是占位，由服务层补齐。
    ///
    /// # 错误
    /// DocumentApproval 缺少已签署页面映射时返回错误。
    pub(crate) fn from_fields(fields: WorkItemFields, queue_context_id: String) -> Result<Self> {
        let route = handler_route(
            fields.work_item_type,
            &fields.business_object_type,
            &fields.owner_role,
        )?;
        let owner_user = fields.owner_user_id.as_ref().map(|id| WorkItemPartyView {
            id: id.clone(),
            display_name: UNRESOLVED_OWNER_DISPLAY_NAME.to_string(),
        });
        let brief = fields
            .brief_source
            .as_ref()
            .map(|source| assemble_brief(source, fields.reason_code.as_deref()));
        Ok(Self {
            id: fields.id,
            work_item_type: fields.work_item_type,
            handler_key: route.handler_key.to_string(),
            destination_workspace_id: route.destination_workspace_id.to_string(),
            route_context: route.route_context,
            approval_step_instance_id: fields.approval_step_instance_id,
            approval_node_execution_id: fields.approval_node_execution_id,
            status: fields.status,
            assignment_mode: fields.assignment_mode,
            assignment_source: fields.assignment_source,
            owner_role_label: role_label(&fields.owner_role),
            owner_role: fields.owner_role,
            owner_organization: WorkItemPartyView {
                id: fields.owner_organization_id.clone(),
                display_name: "责任组织".to_string(),
            },
            owner_organization_id: fields.owner_organization_id,
            owner_user_id: fields.owner_user_id,
            owner_user,
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            business_object_label: fields.business_object_label,
            counterparty_label: fields.counterparty_label,
            next_action_hint: next_action_hint(fields.work_item_type),
            summary_sections: brief
                .as_ref()
                .map(|assembled| {
                    assembled
                        .sections
                        .iter()
                        .map(|section| WorkItemSummarySection {
                            label: section.label.clone(),
                            value: section.value.clone(),
                            numeric: section.numeric.then_some(true),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            brief_lines: brief
                .as_ref()
                .map(|assembled| {
                    assembled
                        .lines
                        .iter()
                        .map(|line| WorkItemBriefLine {
                            title: line.title.clone(),
                            quantity: line.quantity.clone(),
                            due_label: line.due_label.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            brief_more_count: brief
                .as_ref()
                .map(|assembled| assembled.more_count)
                .filter(|count| *count > 0),
            list_summary: brief
                .as_ref()
                .map(|assembled| assembled.list_summary.clone())
                .filter(|text| !text.trim().is_empty()),
            business_object_type: fields.business_object_type,
            business_object_id: fields.business_object_id,
            root_business_object_id: fields.root_business_object_id,
            subject_version: fields.subject_version,
            task_version: fields.task_version.to_string(),
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
            priority: fields.priority,
            due_at: seconds(fields.due_at),
            reason_label: reason_label(fields.reason_code.as_deref(), fields.work_item_type),
            reason_code: fields.reason_code,
            impact_summary: usable_impact_summary(fields.impact_summary.as_deref(), fields.work_item_type),
            assigned_at: seconds(fields.assigned_at),
            started_at: seconds(fields.started_at),
            current_assignment_at: seconds(fields.current_assignment_at),
            last_activity_at: seconds(fields.last_activity_at),
            completed_at: seconds(fields.completed_at),
            completed_by: fields.completed_by,
            closed_at: seconds(fields.closed_at),
            closed_by: fields.closed_by,
            close_reason: fields.close_reason,
            created_at: fields.created_at,
            queue_context_id,
        })
    }
}

/// 开始处理请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StartProcessingRequest {
    #[validate(length(min = 1, max = 20, message = "任务版本格式非法"))]
    pub expected_task_version: String,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 退回团队请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReleaseToTeamRequest {
    #[validate(length(min = 1, max = 20, message = "任务版本格式非法"))]
    pub expected_task_version: String,
    #[validate(length(min = 1, max = 150, message = "原因长度必须在1-150之间"))]
    pub reason: String,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 转交任务请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReassignWorkItemRequest {
    #[validate(length(min = 1, max = 20, message = "任务版本格式非法"))]
    pub expected_task_version: String,
    #[validate(length(min = 1, max = 128, message = "目标用户不能为空或过长"))]
    pub target_user_id: String,
    #[validate(length(min = 1, max = 150, message = "原因长度必须在1-150之间"))]
    pub reason: String,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 关闭无效任务请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CloseWorkItemRequest {
    #[validate(length(min = 1, max = 20, message = "任务版本格式非法"))]
    pub expected_task_version: String,
    #[validate(length(min = 1, max = 64, message = "关闭原因代码不能为空或过长"))]
    pub reason_code: String,
    #[validate(length(max = 100, message = "关闭说明不能超过100个字符"))]
    pub comment: Option<String>,
    /// `DUPLICATE` 时必填的有效替代正式任务。
    #[validate(length(min = 1, max = 128, message = "替代任务ID不能为空或过长"))]
    pub replacement_work_item_id: Option<String>,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkItemFields {
    pub id: String,
    pub work_item_type: WorkItemType,
    pub approval_step_instance_id: Option<String>,
    pub approval_node_execution_id: Option<String>,
    pub business_object_type: String,
    pub business_object_id: String,
    pub root_business_object_id: String,
    pub business_object_label: String,
    pub counterparty_label: Option<String>,
    pub subject_version: String,
    pub status: WorkItemStatus,
    pub assignment_mode: AssignmentMode,
    pub owner_role: String,
    pub owner_organization_id: String,
    pub owner_user_id: Option<String>,
    pub assignment_source: AssignmentSource,
    pub assigned_at: Option<entities::common::time::Instant>,
    pub started_at: Option<entities::common::time::Instant>,
    pub current_assignment_at: Option<entities::common::time::Instant>,
    pub last_activity_at: Option<entities::common::time::Instant>,
    pub priority: WorkItemPriority,
    pub due_at: Option<entities::common::time::Instant>,
    pub reason_code: Option<String>,
    pub impact_summary: Option<String>,
    pub completed_at: Option<entities::common::time::Instant>,
    pub completed_by: Option<String>,
    pub closed_at: Option<entities::common::time::Instant>,
    pub closed_by: Option<String>,
    pub close_reason: Option<String>,
    pub task_version: u64,
    pub created_at: u64,
    pub brief_source: Option<super::brief::ObjectBriefSource>,
}

impl From<WorkItem> for WorkItemFields {
    fn from(item: WorkItem) -> Self {
        let root_business_object_id = item.business_object_id.clone();
        Self {
            id: item.base.id,
            work_item_type: item.work_item_type,
            approval_step_instance_id: item.approval_step_instance_id,
            approval_node_execution_id: item
                .approval_node_execution_id
                .as_ref()
                .map(|id| id.as_ref().to_string()),
            business_object_type: item.business_object_type,
            business_object_id: item.business_object_id,
            root_business_object_id,
            business_object_label: item.work_item_type.label().to_string(),
            counterparty_label: None,
            subject_version: item.subject_version,
            status: item.status,
            assignment_mode: item.assignment_mode,
            owner_role: item.owner_role,
            owner_organization_id: item.owner_organization_id,
            owner_user_id: item.owner_user_id,
            assignment_source: item.assignment_source,
            assigned_at: item.assigned_at,
            started_at: item.started_at,
            current_assignment_at: item.current_assignment_at,
            last_activity_at: item.last_activity_at,
            priority: item.priority,
            due_at: item.due_at,
            reason_code: item.reason_code,
            impact_summary: item.impact_summary,
            completed_at: item.completed_at,
            completed_by: item.completed_by,
            closed_at: item.closed_at,
            closed_by: item.closed_by,
            close_reason: item.close_reason,
            task_version: item.base.version,
            created_at: item.base.created_at,
            brief_source: None,
        }
    }
}

impl From<database::WorkItemRow> for WorkItemFields {
    fn from(item: database::WorkItemRow) -> Self {
        let root_business_object_id = item.business_object_id.clone();
        Self {
            id: item.id,
            work_item_type: item.work_item_type,
            approval_step_instance_id: item.approval_step_instance_id,
            approval_node_execution_id: item.approval_node_execution_id,
            business_object_type: item.business_object_type,
            business_object_id: item.business_object_id,
            root_business_object_id,
            business_object_label: item.work_item_type.label().to_string(),
            counterparty_label: None,
            subject_version: item.subject_version,
            status: item.status,
            assignment_mode: item.assignment_mode,
            owner_role: item.owner_role,
            owner_organization_id: item.owner_organization_id,
            owner_user_id: item.owner_user_id,
            assignment_source: item.assignment_source,
            assigned_at: item.assigned_at,
            started_at: item.started_at,
            current_assignment_at: item.current_assignment_at,
            last_activity_at: item.last_activity_at,
            priority: item.priority,
            due_at: item.due_at,
            reason_code: item.reason_code,
            impact_summary: item.impact_summary,
            completed_at: item.completed_at,
            completed_by: item.completed_by,
            closed_at: item.closed_at,
            closed_by: item.closed_by,
            close_reason: item.close_reason,
            task_version: item.version,
            created_at: item.created_at,
            brief_source: None,
        }
    }
}

struct HandlerRoute {
    handler_key: &'static str,
    destination_workspace_id: &'static str,
    route_context: Option<WorkItemRouteContext>,
}

fn handler_route(
    work_item_type: WorkItemType,
    business_object_type: &str,
    owner_role: &str,
) -> Result<HandlerRoute> {
    let (handler_key, destination_workspace_id) = match (work_item_type, business_object_type) {
        (
            WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException,
            "SUPPLIER_FULFILLMENT_ORDER",
        ) => ("supplier_fulfillment_investigation", "W26"),
        (WorkItemType::BusinessException, "SUPPLIER_OFFERING") => ("supplier_supply_exception", "W21"),
        (WorkItemType::BusinessException, "MASTER_MAPPING_TASK") => ("master_mapping_task", "W17"),
        (WorkItemType::IntegrationResultUnknown, "integration_error_task") => ("integration_unknown", "W29"),
        (WorkItemType::BusinessException, "integration_error_task" | "reconciliation_difference") => {
            ("business_exception", "W29")
        }
        (WorkItemType::IntegrationResultUnknown, "reconciliation_difference") => {
            ("integration_unknown", "W29")
        }
        (WorkItemType::ProcurementConfirmation, _) => ("procurement_confirmation", "W07"),
        (WorkItemType::LowMarginManagerConfirmation, _) => ("low_margin_manager", "W05"),
        (WorkItemType::PurchaseOrderReview, _) => ("po_review", "W08"),
        (WorkItemType::SalesChangeImpactReview, _) => ("sales_change_impact_review", "W05"),
        (WorkItemType::SalesChangeFinanceReview, _) => ("sales_change_finance_review", "W05"),
        (WorkItemType::CardFundsReview, _) => ("card_funds", "W13"),
        (WorkItemType::CardFundsDeltaReview, _) => ("card_funds_delta", "W13"),
        (WorkItemType::CardSalesManagerApproval, _) => ("card_sales_manager_approval", "W05"),
        (WorkItemType::CardSalesOperationApproval, _) => ("card_sales_operations_approval", "W05"),
        (WorkItemType::OwnershipMigrationSalesConfirmation, _) => ("ownership_sales", "W03"),
        (WorkItemType::OwnershipMigrationFinanceConfirmation, _) => ("ownership_finance", "W17"),
        (WorkItemType::InventoryAdjustmentReview, _) => ("inventory_adj", "W10"),
        (WorkItemType::FinanceCorrectionReview, _) => ("finance_correction", "W17"),
        (WorkItemType::SupplierSettlementReview, _) => ("supplier_settlement", "W27"),
        (WorkItemType::ImportBusinessConfirmation, _) => ("import_business_confirmation", "W18"),
        (WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException, _) => {
            ("unregistered_work_item", "W01")
        }
        (WorkItemType::DocumentApproval, object_type) => document_approval_route(object_type)?,
    };
    let mut route_context =
        w18_confirmation_scope(work_item_type, owner_role).map(|scope| WorkItemRouteContext {
            confirmation_scope: Some(scope.to_string()),
            document_type: None,
        });
    if work_item_type == WorkItemType::DocumentApproval {
        route_context = Some(WorkItemRouteContext {
            confirmation_scope: None,
            document_type: Some(business_object_type.to_string()),
        });
    }
    Ok(HandlerRoute {
        handler_key,
        destination_workspace_id,
        route_context,
    })
}

fn w18_confirmation_scope(work_item_type: WorkItemType, owner_role: &str) -> Option<&'static str> {
    if work_item_type != WorkItemType::ImportBusinessConfirmation {
        return None;
    }
    match owner_role {
        "role-sales" => Some("SALES"),
        "role-procurement" => Some("PROCUREMENT"),
        "role-operations" => Some("OPERATIONS"),
        "role-warehouse" => Some("WAREHOUSE"),
        "role-finance" => Some("FINANCE"),
        _ => None,
    }
}

fn family_of(work_item_type: WorkItemType) -> WorkItemFamily {
    match work_item_type {
        WorkItemType::DocumentApproval
        | WorkItemType::CardSalesManagerApproval
        | WorkItemType::CardSalesOperationApproval
        | WorkItemType::LowMarginManagerConfirmation
        | WorkItemType::OwnershipMigrationSalesConfirmation => WorkItemFamily::Approval,
        WorkItemType::CardFundsReview
        | WorkItemType::CardFundsDeltaReview
        | WorkItemType::PurchaseOrderReview
        | WorkItemType::SalesChangeFinanceReview
        | WorkItemType::OwnershipMigrationFinanceConfirmation
        | WorkItemType::FinanceCorrectionReview
        | WorkItemType::SupplierSettlementReview => WorkItemFamily::Finance,
        WorkItemType::ProcurementConfirmation
        | WorkItemType::SalesChangeImpactReview
        | WorkItemType::InventoryAdjustmentReview => WorkItemFamily::Fulfillment,
        WorkItemType::ImportBusinessConfirmation
        | WorkItemType::IntegrationResultUnknown
        | WorkItemType::BusinessException => WorkItemFamily::Exception,
    }
}

/// 按已签署页面映射单据审批目标工作面。缺少映射失败关闭，不得回落 W05。
///
/// # 参数
/// * `business_object_type` - WorkItem 中的 DocumentType 稳定代码
///
/// # 返回
/// 返回 handler 与目标 workspace。
///
/// # 错误
/// 未签署映射时返回稳定错误，不得回落默认工作面。
fn document_approval_route(business_object_type: &str) -> Result<(&'static str, &'static str)> {
    match business_object_type {
        "sales_order" | "voucher_sales_order" | "sales_change_order" => Ok(("document_approval", "W05")),
        "purchase_order" | "purchase_change_order" => Ok(("document_approval", "W08")),
        "stock_adjustment" => Ok(("document_approval", "W10")),
        "customer_receipt" | "customer_refund" | "receipt_reversal" => Ok(("document_approval", "W11")),
        "supplier_payment" | "supplier_refund" | "payment_reversal" => Ok(("document_approval", "W12")),
        _ => Err(Error::ValidationError(
            "APPROVAL_DOCUMENT_ROUTE_UNMAPPED".to_string(),
        )),
    }
}

fn normalize_work_item_types(
    family: Option<WorkItemFamily>,
    work_item_type: Option<WorkItemType>,
) -> Result<Vec<WorkItemType>> {
    let Some(family) = family else {
        return Ok(work_item_type.into_iter().collect());
    };
    if let Some(work_item_type) = work_item_type {
        if family_of(work_item_type) != family {
            return Err(Error::ValidationError("任务类型不属于所选任务族".to_string()));
        }
        return Ok(vec![work_item_type]);
    }
    Ok(family.work_item_types())
}

fn normalize_statuses(scope: WorkItemScope, status: Option<WorkItemStatus>) -> Result<Vec<WorkItemStatus>> {
    match scope {
        WorkItemScope::History => match status {
            None => Ok(vec![WorkItemStatus::Completed, WorkItemStatus::Closed]),
            Some(WorkItemStatus::Completed | WorkItemStatus::Closed) => Ok(vec![status.unwrap()]),
            Some(WorkItemStatus::Open) => Err(Error::ValidationError(
                "处理历史只能查询已完成或已关闭任务".to_string(),
            )),
        },
        _ => match status {
            None | Some(WorkItemStatus::Open) => Ok(vec![WorkItemStatus::Open]),
            Some(_) => Err(Error::ValidationError("开放队列只能查询待处理任务".to_string())),
        },
    }
}

fn parse_priorities(value: Option<&str>) -> Result<Vec<WorkItemPriority>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    value
        .split(',')
        .map(|part| match part.trim() {
            "1" => Ok(WorkItemPriority::Urgent),
            "2" => Ok(WorkItemPriority::High),
            "3" => Ok(WorkItemPriority::Normal),
            "4" => Ok(WorkItemPriority::Low),
            _ => Err(Error::ValidationError("优先级必须是1至4".to_string())),
        })
        .collect()
}

fn ensure_supported_query(params: &WorkItemListParams) -> Result<()> {
    let timezone = params.timezone.as_deref().unwrap_or(DEFAULT_TIMEZONE).trim();
    if timezone != DEFAULT_TIMEZONE {
        return Err(Error::ValidationError(
            "当前任务队列只支持 Asia/Shanghai 时区".to_string(),
        ));
    }
    Ok(())
}

fn normalize_sort(sort: Option<WorkItemSort>) -> (&'static str, bool) {
    match sort.unwrap_or(WorkItemSort::PriorityDue) {
        WorkItemSort::PriorityDue | WorkItemSort::DueAsc => ("due_at", true),
        WorkItemSort::CreatedDesc => ("created_at", false),
    }
}

fn seconds(value: Option<entities::common::time::Instant>) -> Option<u64> {
    value.and_then(|instant| u64::try_from(instant.unix_secs()).ok())
}

fn role_label(role: &str) -> String {
    match role {
        "role-sales" | "sales" => "销售",
        "role-sales-leader" | "sales_leader" => "销售领导",
        "role-procurement" | "procurement" => "采购",
        "role-operations" | "operations" => "运营",
        "role-finance" | "finance" => "财务",
        "role-management" | "management" => "管理层",
        _ => role,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(scope: WorkItemScope) -> WorkItemListParams {
        WorkItemListParams {
            scope,
            family: None,
            work_item_type: None,
            status: None,
            due: None,
            priorities: None,
            q: None,
            sort: None,
            queue_context_id: None,
            current_work_item_id: None,
            timezone: Some(DEFAULT_TIMEZONE.to_string()),
            page: None,
            page_size: None,
        }
    }

    #[test]
    fn scope_status_combinations_fail_closed() {
        let mut history = params(WorkItemScope::History);
        history.status = Some(WorkItemStatus::Open);
        assert!(history.normalized().is_err());

        let mut mine = params(WorkItemScope::Mine);
        mine.status = Some(WorkItemStatus::Completed);
        assert!(mine.normalized().is_err());
    }

    #[test]
    fn family_and_type_must_match_registered_mapping() {
        let mut query = params(WorkItemScope::Mine);
        query.family = Some(WorkItemFamily::Finance);
        query.work_item_type = Some(WorkItemType::BusinessException);
        assert!(query.normalized().is_err());
    }

    #[test]
    fn text_search_and_focus_are_normalized_for_server_filtering() {
        let mut query = params(WorkItemScope::Mine);
        query.q = Some("  SO-1  ".to_string());
        query.current_work_item_id = Some("  wi-1  ".to_string());
        let normalized = query.normalized().unwrap();
        assert_eq!(normalized.query.as_deref(), Some("SO-1"));
        assert_eq!(normalized.current_work_item_id.as_deref(), Some("wi-1"));
    }

    #[test]
    fn priority_codes_are_strict_and_ordered() {
        let priorities = parse_priorities(Some("1,3")).unwrap();
        assert_eq!(
            priorities,
            vec![WorkItemPriority::Urgent, WorkItemPriority::Normal]
        );
        assert!(parse_priorities(Some("urgent")).is_err());
    }

    #[test]
    fn conflict_data_serializes_only_permission_safe_projection() {
        let hidden = WorkItemConflict::new(WorkItemConflictKind::Responsibility, None);
        let value = serde_json::to_value(&hidden).expect("conflict data should serialize");

        assert_eq!(value, serde_json::json!({ "current_work_item": null }));
        assert_eq!(hidden.kind().code(), "WORK_ITEM_RESPONSIBILITY_CONFLICT");
        assert_eq!(WorkItemConflictKind::Version.code(), "WORK_ITEM_VERSION_CONFLICT");
    }

    #[test]
    fn w13_receivable_review_routes_use_fixed_handlers() {
        let opening = handler_route(
            WorkItemType::CardFundsReview,
            "receivable_account",
            "role-finance",
        )
        .unwrap();
        let delta = handler_route(
            WorkItemType::CardFundsDeltaReview,
            "receivable_account",
            "role-finance",
        )
        .unwrap();

        assert_eq!(opening.handler_key, "card_funds");
        assert_eq!(opening.destination_workspace_id, "W13");
        assert_eq!(delta.handler_key, "card_funds_delta");
        assert_eq!(delta.destination_workspace_id, "W13");
        assert!(opening.route_context.is_none());
        assert!(delta.route_context.is_none());
    }

    #[test]
    fn w18_route_context_uses_only_fixed_owner_role_registry() {
        let cases = [
            ("role-sales", "SALES"),
            ("role-procurement", "PROCUREMENT"),
            ("role-operations", "OPERATIONS"),
            ("role-warehouse", "WAREHOUSE"),
            ("role-finance", "FINANCE"),
        ];
        for (role, scope) in cases {
            let route = handler_route(
                WorkItemType::ImportBusinessConfirmation,
                "LEGACY_IMPORT_BATCH",
                role,
            )
            .unwrap();
            assert_eq!(
                route
                    .route_context
                    .and_then(|context| context.confirmation_scope)
                    .as_deref(),
                Some(scope)
            );
        }
        let unknown = handler_route(
            WorkItemType::ImportBusinessConfirmation,
            "LEGACY_IMPORT_BATCH",
            "role-unregistered",
        )
        .unwrap();
        assert!(unknown.route_context.is_none());
    }

    #[test]
    fn document_approval_maps_to_signed_workspace_and_approval_family() {
        let stock = handler_route(
            WorkItemType::DocumentApproval,
            "stock_adjustment",
            "stock_adjustment_approver",
        )
        .unwrap();
        assert_eq!(stock.handler_key, "document_approval");
        assert_eq!(stock.destination_workspace_id, "W10");
        assert_eq!(
            stock
                .route_context
                .and_then(|context| context.document_type)
                .as_deref(),
            Some("stock_adjustment")
        );
        let missing = handler_route(WorkItemType::DocumentApproval, "unknown_type", "approver");
        match missing {
            Err(error) => assert!(error.to_string().contains("APPROVAL_DOCUMENT_ROUTE_UNMAPPED")),
            Ok(_) => panic!("缺少映射必须失败关闭"),
        }
        assert_eq!(
            family_of(WorkItemType::DocumentApproval),
            WorkItemFamily::Approval
        );
        assert!(!WORK_ITEM_TYPES.contains(&WorkItemType::CardSalesManagerApproval));
        assert!(!WORK_ITEM_TYPES.contains(&WorkItemType::CardSalesOperationApproval));
        assert!(WORK_ITEM_TYPES.contains(&WorkItemType::DocumentApproval));
    }
}
