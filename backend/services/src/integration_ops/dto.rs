//! 域 D34 `integration_ops` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。
//!
//! 与 `erp-client/features/integration-errors` 契约对齐（P3 §4.7）：
//! - REPLAY 请求类型**不包含** `originalActionIdempotencyKey` 字段，并以
//!   `#[serde(deny_unknown_fields)]` 强制拒绝客户端传入原键（服务端锁定原键）；
//! - QUERY 结果区分「已受理/明确无结果/仍未知」，只有 `no_result_confirmed`
//!   才可能开放 REPLAY（§7.7、W29 §8.2）；
//! - 差异终结只接受固定原因枚举（W29 §7：`SOURCE_CORRECTED_AND_REATTRIBUTED` /
//!   `BUSINESS_CONFIRMED_NO_ERROR` / `COMPENSATION_CLOSED`），禁止自由文本原因。

use entities::integration_ops::{
    ErrorClass, ErrorTaskStatus, InboxMessage, InboxMessageStatus, IntegrationErrorTask, MessageType,
    ReconciliationDifference, ReconciliationDifferenceResolution, ResolutionAction, ResolutionType,
    ResultingStatus,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// `inbox_message` 列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const INBOX_MESSAGE_SORT_FIELDS: &[&str] = &["created_at", "received_at", "status"];
/// `integration_error_task` 列表允许的排序字段白名单。
pub(crate) const ERROR_TASK_SORT_FIELDS: &[&str] = &["created_at", "last_attempt_at", "status"];
/// `reconciliation_difference` 列表允许的排序字段白名单（仅差异发现时间）。
pub(crate) const DIFFERENCE_SORT_FIELDS: &[&str] = &["created_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串不生效）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 入站消息（inbox_message）
// ---------------------------------------------------------------------------

/// 入站消息登记请求（HTTP 契约：消息信封字段；处理状态由服务端置为 `received`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RegisterInboxMessageRequest {
    /// 来源系统 ID（D01，须已登记）。
    pub source_system_id: entities::integration_ops::SourceSystemId,
    /// 来源事件 ID（消息层幂等身份，与来源系统组合唯一）。
    #[validate(
        custom(function = "non_blank", message = "来源事件ID不能为空"),
        length(max = 256, message = "来源事件ID过长")
    )]
    pub source_event_id: String,
    /// 消息类型（商城关键事实或供应商回调）。
    pub message_type: MessageType,
    /// 业务事实键（业务事实层幂等身份）。
    #[validate(
        custom(function = "non_blank", message = "业务事实键不能为空"),
        length(max = 256, message = "业务事实键过长")
    )]
    pub business_fact_key: String,
    /// 来源契约版本。
    #[validate(
        custom(function = "non_blank", message = "来源契约版本不能为空"),
        length(max = 64, message = "来源契约版本过长")
    )]
    pub payload_schema_version: String,
    /// 规范化内容引用（非完整载荷）。
    #[validate(length(max = 512, message = "内容引用过长"))]
    pub payload_reference: Option<String>,
    /// 来源系统发送时间（秒级时间戳）；缺省表示未知。
    #[validate(range(min = 1, message = "发送时间必须大于 0"))]
    pub source_sent_at: Option<i64>,
    /// ERP 接收时间（秒级时间戳）；缺省取服务端当前时间。
    #[validate(range(min = 1, message = "接收时间必须大于 0"))]
    pub received_at: Option<i64>,
}

/// 入站消息结果回写请求（乐观锁：携带期望版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WriteBackInboxResultRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 回写结果：`processed` 标记已处理；`failed` 标记失败并登记错误任务。
    pub outcome: WriteBackOutcome,
    /// 处理完成时间（秒级时间戳）；缺省取服务端当前时间。
    #[validate(range(min = 1, message = "处理完成时间必须大于 0"))]
    pub processed_at: Option<i64>,
    /// 失败时的错误分类（`outcome=failed` 必填）。
    pub error_class: Option<ErrorClass>,
    /// 失败时的责任角色。
    #[validate(length(max = 64, message = "责任角色过长"))]
    pub owner_role: Option<String>,
    /// 失败时的责任人。
    #[validate(length(max = 128, message = "责任人过长"))]
    pub owner_user_id: Option<String>,
    /// 失败时的脱敏尝试结果摘要。
    #[validate(length(max = 512, message = "尝试结果摘要过长"))]
    pub attempt_summary: Option<String>,
}

/// 结果回写取值（`processed` / `failed`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteBackOutcome {
    /// 已处理。
    Processed,
    /// 失败（转入错误任务）。
    Failed,
}

/// 入站消息列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct InboxMessageListParams {
    /// 来源系统 ID 筛选。
    pub source_system_id: Option<entities::integration_ops::SourceSystemId>,
    /// 消息类型筛选。
    pub message_type: Option<MessageType>,
    /// 消息处理状态筛选。
    pub status: Option<InboxMessageStatus>,
    /// 来源事件 ID 模糊匹配（忽略大小写）。
    pub source_event_id: Option<String>,
    /// 接收时间下界（秒级时间戳，含）。
    pub received_at_from: Option<i64>,
    /// 接收时间上界（秒级时间戳，含）。
    pub received_at_to: Option<i64>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`received_at`/`status`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的入站消息列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InboxMessageListQuery {
    /// 来源系统 ID 筛选。
    pub source_system_id: Option<entities::integration_ops::SourceSystemId>,
    /// 消息类型筛选。
    pub message_type: Option<MessageType>,
    /// 消息处理状态筛选。
    pub status: Option<InboxMessageStatus>,
    /// 来源事件 ID 模糊匹配。
    pub source_event_id: Option<String>,
    /// 接收时间下界。
    pub received_at_from: Option<i64>,
    /// 接收时间上界。
    pub received_at_to: Option<i64>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl InboxMessageListParams {
    /// 归一化入站消息列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<InboxMessageListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, INBOX_MESSAGE_SORT_FIELDS)?;
        Ok(InboxMessageListQuery {
            source_system_id: self.source_system_id.clone(),
            message_type: self.message_type,
            status: self.status,
            source_event_id: normalized_text(self.source_event_id.as_deref()),
            received_at_from: self.received_at_from,
            received_at_to: self.received_at_to,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 入站消息列表响应视图（列表投影不暴露内容引用，字段与 P2 列表投影一致）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InboxMessageListView {
    /// 实体主键。
    pub id: String,
    /// 来源系统 ID。
    pub source_system_id: String,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 消息类型。
    pub message_type: MessageType,
    /// 业务事实键（幂等键）。
    pub business_fact_key: String,
    /// 来源契约版本。
    pub payload_schema_version: String,
    /// 消息处理状态。
    pub status: InboxMessageStatus,
    /// 来源系统发送时间（秒级时间戳）。
    pub source_sent_at: Option<i64>,
    /// ERP 接收时间（秒级时间戳）。
    pub received_at: i64,
    /// 处理完成时间（秒级时间戳）。
    pub processed_at: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 入站消息详情响应视图（含规范化内容引用，供详情页证据区展示）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InboxMessageView {
    /// 实体主键。
    pub id: String,
    /// 来源系统 ID。
    pub source_system_id: String,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 消息类型。
    pub message_type: MessageType,
    /// 业务事实键（幂等键）。
    pub business_fact_key: String,
    /// 来源契约版本。
    pub payload_schema_version: String,
    /// 规范化内容引用。
    pub payload_reference: Option<String>,
    /// 消息处理状态。
    pub status: InboxMessageStatus,
    /// 来源系统发送时间（秒级时间戳）。
    pub source_sent_at: Option<i64>,
    /// ERP 接收时间（秒级时间戳）。
    pub received_at: i64,
    /// 处理完成时间（秒级时间戳）。
    pub processed_at: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<InboxMessage> for InboxMessageView {
    /// 从入站消息实体构造详情视图。
    ///
    /// # 参数
    /// * `message` - 入站消息实体
    ///
    /// # 返回
    /// 返回详情视图。
    fn from(message: InboxMessage) -> Self {
        Self {
            id: message.base.id,
            source_system_id: message.source_system_id.to_string(),
            source_event_id: message.source_event_id,
            message_type: message.message_type,
            business_fact_key: message.business_fact_key,
            payload_schema_version: message.payload_schema_version,
            payload_reference: message.payload_reference,
            status: message.status,
            source_sent_at: message.source_sent_at.map(|at| at.unix_secs()),
            received_at: message.received_at.unix_secs(),
            processed_at: message.processed_at.map(|at| at.unix_secs()),
            version: message.base.version,
            created_at: message.base.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// 集成错误任务（integration_error_task）
// ---------------------------------------------------------------------------

/// 错误任务登记请求（消息类失败必填 `message_id`，业务对象类失败必填 `business_object_id`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateErrorTaskRequest {
    /// 关联的消息 ID（消息类失败必填其一）。
    pub message_id: Option<entities::integration_ops::InboxMessageId>,
    /// 关联的业务对象 ID（非消息类失败必填其一）。
    #[validate(length(max = 128, message = "业务对象ID过长"))]
    pub business_object_id: Option<String>,
    /// 错误分类。
    pub error_class: ErrorClass,
    /// 责任角色。
    #[validate(length(max = 64, message = "责任角色过长"))]
    pub owner_role: Option<String>,
    /// 责任人。
    #[validate(length(max = 128, message = "责任人过长"))]
    pub owner_user_id: Option<String>,
}

/// 错误任务列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ErrorTaskListParams {
    /// 关联消息 ID 筛选。
    pub message_id: Option<entities::integration_ops::InboxMessageId>,
    /// 关联业务对象 ID 筛选。
    pub business_object_id: Option<String>,
    /// 错误分类筛选。
    pub error_class: Option<ErrorClass>,
    /// 任务状态筛选。
    pub status: Option<ErrorTaskStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 责任人模糊匹配（忽略大小写）。
    pub owner_user_id: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`last_attempt_at`/`status`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的错误任务列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ErrorTaskListQuery {
    /// 关联消息 ID 筛选。
    pub message_id: Option<entities::integration_ops::InboxMessageId>,
    /// 关联业务对象 ID 筛选。
    pub business_object_id: Option<String>,
    /// 错误分类筛选。
    pub error_class: Option<ErrorClass>,
    /// 任务状态筛选。
    pub status: Option<ErrorTaskStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 责任人模糊匹配。
    pub owner_user_id: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ErrorTaskListParams {
    /// 归一化错误任务列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ErrorTaskListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, ERROR_TASK_SORT_FIELDS)?;
        Ok(ErrorTaskListQuery {
            message_id: self.message_id.clone(),
            business_object_id: normalized_text(self.business_object_id.as_deref()),
            error_class: self.error_class,
            status: self.status,
            owner_role: normalized_text(self.owner_role.as_deref()),
            owner_user_id: normalized_text(self.owner_user_id.as_deref()),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 查询原结果请求（结果未知任务的 REPLAY 前置动作；结果写入最近尝试摘要）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct QueryOriginalResultRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 查询结果：`terminal_evidence_found` 已受理 / `no_result_confirmed` 明确无结果 /
    /// `result_unknown` 仍未知。只有明确无结果才可能开放 REPLAY（§7.7）。
    pub outcome: QueryOutcome,
    /// 查询备注。
    #[validate(length(max = 512, message = "备注过长"))]
    pub comment: Option<String>,
}

/// 查询原结果取值（W29 §8.2：已受理 / 明确无结果 / 仍未知）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryOutcome {
    /// 已受理（取得可验证终态）。
    TerminalEvidenceFound,
    /// 明确无结果（服务端判定安全后可重放）。
    NoResultConfirmed,
    /// 仍未知（保持非终结状态，可再查询或转交）。
    ResultUnknown,
}

impl QueryOutcome {
    /// 返回写入最近尝试摘要的稳定代码。
    ///
    /// # 返回
    /// 返回 `query_outcome=` 前缀的稳定字符串。
    pub(crate) fn summary_marker(self) -> &'static str {
        match self {
            Self::TerminalEvidenceFound => "query_outcome=terminal_evidence_found",
            Self::NoResultConfirmed => "query_outcome=no_result_confirmed",
            Self::ResultUnknown => "query_outcome=result_unknown",
        }
    }
}

/// 重放原动作请求。
///
/// 契约约束（W29 §8.2）：**永不接受** `originalActionIdempotencyKey`——
/// 服务端锁定原幂等键（关联消息的业务事实键）并自行沿用，客户端无权生成、
/// 覆盖或替换；`deny_unknown_fields` 使客户端携带该键时直接 422 拒绝。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ReplayOriginalRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 重放备注。
    #[validate(length(max = 512, message = "备注过长"))]
    pub comment: Option<String>,
}

/// 重放原动作响应（服务端锁定原键，只返回脱敏摘要与锁定标识）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReplayResultView {
    /// 错误任务 ID。
    pub task_id: String,
    /// 服务端锁定的原幂等键摘要（脱敏，非完整键）。
    pub original_action_idempotency_key_summary: String,
    /// 原键锁定标识（视图恒为 `true`，客户端不可传原键）。
    pub original_action_idempotency_key_locked: bool,
    /// 重放已受理（任务仍处于非终结状态）。
    pub replay_accepted: bool,
    /// 重放后的任务状态。
    pub task_status: ErrorTaskStatus,
    /// 累计尝试次数（含本次重放）。
    pub attempt_count: u32,
    /// 重放后的任务乐观锁版本（后续动作回传）。
    pub task_version: u64,
}

/// 暂挂/跳过请求（动作保留在队列，不终结任务；只追加尝试摘要与审计）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct HoldErrorTaskRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 动作类型：`defer` 暂挂 / `skip` 跳过。
    pub kind: HoldKind,
    /// 原因代码。
    #[validate(length(max = 64, message = "原因代码过长"))]
    pub reason_code: Option<String>,
    /// 备注。
    #[validate(length(max = 512, message = "备注过长"))]
    pub comment: Option<String>,
}

/// 暂挂/跳过动作取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldKind {
    /// 暂挂（当前项保留在队列，焦点可进入下一项）。
    Defer,
    /// 跳过（记录跳过，任务仍在队列）。
    Skip,
}

impl HoldKind {
    /// 返回写入最近尝试摘要的稳定代码。
    ///
    /// # 返回
    /// 返回 `deferred` / `skipped`。
    pub(crate) fn summary_marker(self) -> &'static str {
        match self {
            Self::Defer => "deferred",
            Self::Skip => "skipped",
        }
    }
}

/// 转交任务请求（只更新责任人，任务状态不变；转交不是解决）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TransferErrorTaskRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 新的责任角色；与责任人都为空时拒绝。
    #[validate(length(max = 64, message = "责任角色过长"))]
    pub owner_role: Option<String>,
    /// 新的责任人；与责任角色都为空时拒绝。
    #[validate(length(max = 128, message = "责任人过长"))]
    pub owner_user_id: Option<String>,
}

/// 解决任务请求（终态：已解决；解决方式不得为「关闭」）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResolveErrorTaskRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 解决方式（查询确认/修复映射/重放/补偿；「关闭」走关闭入口）。
    pub resolution_type: ResolutionType,
    /// 终态证据说明（非空）。
    #[validate(
        custom(function = "non_blank", message = "终态证据不能为空"),
        length(max = 1024, message = "终态证据过长")
    )]
    pub resolution: String,
}

/// 关闭任务请求（终态：已关闭；重复关闭必须关联替代任务）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CloseErrorTaskRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 关闭原因：`duplicate` 重复 / `misrouted` 误派。
    pub reason: CloseReason,
    /// 替代任务或终态证据说明（非空）。
    #[validate(
        custom(function = "non_blank", message = "关闭证据不能为空"),
        length(max = 1024, message = "关闭证据过长")
    )]
    pub resolution: String,
    /// 替代任务 ID（`reason=duplicate` 必填）。
    pub replacement_task_id: Option<entities::integration_ops::IntegrationErrorTaskId>,
}

/// 关闭原因取值（`duplicate` / `misrouted`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    /// 重复任务。
    Duplicate,
    /// 误派任务。
    Misrouted,
}

/// 错误任务列表响应视图（列表投影不含解决证据文本）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ErrorTaskView {
    /// 实体主键。
    pub id: String,
    /// 关联的消息。
    pub message_id: Option<String>,
    /// 关联的业务对象。
    pub business_object_id: Option<String>,
    /// 错误分类。
    pub error_class: ErrorClass,
    /// 任务状态。
    pub status: ErrorTaskStatus,
    /// 责任角色。
    pub owner_role: Option<String>,
    /// 责任人。
    pub owner_user_id: Option<String>,
    /// 重试次数。
    pub attempt_count: u32,
    /// 最近尝试时间（秒级时间戳）。
    pub last_attempt_at: Option<i64>,
    /// 最近尝试结果（脱敏）。
    pub last_attempt_summary: Option<String>,
    /// 解决方式。
    pub resolution_type: Option<ResolutionType>,
    /// 完成时间（秒级时间戳）。
    pub resolved_at: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<IntegrationErrorTask> for ErrorTaskView {
    /// 从错误任务实体构造任务视图。
    ///
    /// # 参数
    /// * `task` - 错误任务实体
    ///
    /// # 返回
    /// 返回任务视图。
    fn from(task: IntegrationErrorTask) -> Self {
        Self {
            id: task.base.id,
            message_id: task.message_id.map(|id| id.to_string()),
            business_object_id: task.business_object_id,
            error_class: task.error_class,
            status: task.status,
            owner_role: task.owner_role,
            owner_user_id: task.owner_user_id,
            attempt_count: task.attempt_count,
            last_attempt_at: task.last_attempt_at.map(|at| at.unix_secs()),
            last_attempt_summary: task.last_attempt_summary,
            resolution_type: task.resolution_type,
            resolved_at: task.resolved_at.map(|at| at.unix_secs()),
            version: task.base.version,
            created_at: task.base.created_at,
        }
    }
}

/// 错误任务详情响应视图（任务字段 + 解决/关闭证据文本）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ErrorTaskDetailView {
    /// 任务列表视图字段（扁平展开）。
    #[serde(flatten)]
    pub task: ErrorTaskView,
    /// 解决/关闭证据文本（列表投影不暴露，详情可见）。
    pub resolution: Option<String>,
}

// ---------------------------------------------------------------------------
// 对账差异（reconciliation_difference + reconciliation_difference_resolution）
// ---------------------------------------------------------------------------

/// 对账差异登记请求（创建后不可修改，不设软删除）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateDifferenceRequest {
    /// 差异对象类型（如商城订单、供应商订单、销售单等）。
    #[validate(
        custom(function = "non_blank", message = "差异对象类型不能为空"),
        length(max = 64, message = "差异对象类型过长")
    )]
    pub business_object_type: String,
    /// 差异对象 ID。
    #[validate(
        custom(function = "non_blank", message = "差异对象ID不能为空"),
        length(max = 128, message = "差异对象ID过长")
    )]
    pub business_object_id: String,
    /// 差异分类。
    #[validate(
        custom(function = "non_blank", message = "差异分类不能为空"),
        length(max = 64, message = "差异分类过长")
    )]
    pub difference_type: String,
    /// 左侧不可变证据引用；两侧至少其一。
    #[validate(length(max = 512, message = "证据引用过长"))]
    pub left_fact_reference: Option<String>,
    /// 右侧不可变证据引用；两侧至少其一。
    #[validate(length(max = 512, message = "证据引用过长"))]
    pub right_fact_reference: Option<String>,
}

/// 对账差异列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DifferenceListParams {
    /// 差异对象类型筛选。
    pub business_object_type: Option<String>,
    /// 差异对象 ID 筛选。
    pub business_object_id: Option<String>,
    /// 差异分类筛选。
    pub difference_type: Option<String>,
    /// 发现时间下界（秒级时间戳，含）。
    pub created_at_from: Option<i64>,
    /// 发现时间上界（秒级时间戳，含）。
    pub created_at_to: Option<i64>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的对账差异列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DifferenceListQuery {
    /// 差异对象类型筛选。
    pub business_object_type: Option<String>,
    /// 差异对象 ID 筛选。
    pub business_object_id: Option<String>,
    /// 差异分类筛选。
    pub difference_type: Option<String>,
    /// 发现时间下界。
    pub created_at_from: Option<i64>,
    /// 发现时间上界。
    pub created_at_to: Option<i64>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl DifferenceListParams {
    /// 归一化对账差异列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<DifferenceListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, DIFFERENCE_SORT_FIELDS)?;
        Ok(DifferenceListQuery {
            business_object_type: normalized_text(self.business_object_type.as_deref()),
            business_object_id: normalized_text(self.business_object_id.as_deref()),
            difference_type: normalized_text(self.difference_type.as_deref()),
            created_at_from: self.created_at_from,
            created_at_to: self.created_at_to,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 差异人工处理请求（非终态动作：领取/处理中/补证，只追加处理记录）。
///
/// 差异本身是不可变正式事实（锁版本永不变化），并发保护以「处理记录序号」为
/// 乐观锁令牌：`version` = 期望的最新处理序号（0 表示尚无处理记录），由上一次
/// 处理/解决响应回传；序号不一致或并发追加冲突时返回 409。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProcessDifferenceRequest {
    /// 期望的最新处理序号（0 表示无处理记录；由上一次响应回传）。
    #[validate(range(max = 1_000_000, message = "处理序号不合法"))]
    pub version: Option<u64>,
    /// 处理动作：`claim` 领取（仅首条）/ `processing` 处理中 / `add_evidence` 补充证据。
    pub action: DifferenceProcessAction,
    /// 终态证据或补充证据引用（追加式，不可覆盖历史）。
    #[validate(length(max = 512, message = "证据引用过长"))]
    pub evidence_reference: Option<String>,
    /// 备注。
    #[validate(length(max = 512, message = "备注过长"))]
    pub comment: Option<String>,
}

/// 差异人工处理动作取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifferenceProcessAction {
    /// 领取（仅首条处理记录允许）。
    Claim,
    /// 处理中。
    Processing,
    /// 补充证据。
    AddEvidence,
}

/// 差异终结结论请求（只追加处理记录并派生终态；不完成/关闭任何任务）。
///
/// 并发保护同 [`ProcessDifferenceRequest`]：`version` 是期望的最新处理序号。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResolveDifferenceRequest {
    /// 期望的最新处理序号（0 表示无处理记录；由上一次响应回传）。
    #[validate(range(max = 1_000_000, message = "处理序号不合法"))]
    pub version: Option<u64>,
    /// 终结结论：`confirm_no_error` 确认无误 / `confirm_valid_difference` 确认有效差异。
    pub conclusion: DifferenceConclusion,
    /// 固定原因枚举（禁止自由文本，W29 §7）。
    pub reason_code: DifferenceReasonCode,
    /// 受控证据引用（非空）。
    #[validate(
        custom(function = "non_blank", message = "受控证据不能为空"),
        length(max = 400, message = "证据引用过长")
    )]
    pub evidence_reference: String,
    /// 备注。
    #[validate(length(max = 512, message = "备注过长"))]
    pub comment: Option<String>,
}

/// 差异终结结论取值（`confirm_no_error` / `confirm_valid_difference`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifferenceConclusion {
    /// 确认无误。
    ConfirmNoError,
    /// 确认有效差异。
    ConfirmValidDifference,
}

/// 差异终结固定原因枚举（W29 §7 至少含三项；禁止自由字符串原因）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DifferenceReasonCode {
    /// 来源已更正并重新归集。
    SourceCorrectedAndReattributed,
    /// 业务确认无误。
    BusinessConfirmedNoError,
    /// 已补偿闭环。
    CompensationClosed,
}

impl DifferenceReasonCode {
    /// 返回原因稳定代码。
    ///
    /// # 返回
    /// 返回用于写入处理记录证据引用的稳定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceCorrectedAndReattributed => "SOURCE_CORRECTED_AND_REATTRIBUTED",
            Self::BusinessConfirmedNoError => "BUSINESS_CONFIRMED_NO_ERROR",
            Self::CompensationClosed => "COMPENSATION_CLOSED",
        }
    }
}

/// 对账差异列表响应视图（`status` 由最后一条处理记录派生；无处理记录时为 `None`）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DifferenceView {
    /// 实体主键。
    pub id: String,
    /// 差异对象类型。
    pub business_object_type: String,
    /// 差异对象 ID。
    pub business_object_id: String,
    /// 差异分类。
    pub difference_type: String,
    /// 左侧不可变证据引用。
    pub left_fact_reference: Option<String>,
    /// 右侧不可变证据引用。
    pub right_fact_reference: Option<String>,
    /// 派生处理状态（`None` 表示尚无处理记录）。
    pub status: Option<ResultingStatus>,
    /// 乐观锁版本。
    pub version: u64,
    /// 差异发现时间（秒级时间戳）。
    pub created_at: u64,
}

/// 差异处理记录视图（不可变追加历史）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolutionView {
    /// 实体主键。
    pub id: String,
    /// 递增处理序号。
    pub resolution_no: u32,
    /// 解决动作。
    pub resolution_action: ResolutionAction,
    /// 动作后的派生状态。
    pub resulting_status: ResultingStatus,
    /// 终态证据引用。
    pub evidence_reference: Option<String>,
    /// 替代任务 ID（关闭重复时关联）。
    pub replacement_task_id: Option<String>,
    /// 处理人。
    pub handled_by: String,
    /// 处理时间（秒级时间戳）。
    pub handled_at: i64,
}

impl From<ReconciliationDifferenceResolution> for ResolutionView {
    /// 从处理记录实体构造视图。
    ///
    /// # 参数
    /// * `resolution` - 处理记录实体
    ///
    /// # 返回
    /// 返回处理记录视图。
    fn from(resolution: ReconciliationDifferenceResolution) -> Self {
        Self {
            id: resolution.base.id,
            resolution_no: resolution.resolution_no,
            resolution_action: resolution.resolution_action,
            resulting_status: resolution.resulting_status,
            evidence_reference: resolution.evidence_reference,
            replacement_task_id: resolution.replacement_task_id.map(|id| id.to_string()),
            handled_by: resolution.handled_by,
            handled_at: resolution.handled_at.unix_secs(),
        }
    }
}

/// 对账差异详情响应视图（差异字段 + 全部处理记录时间线）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DifferenceDetailView {
    /// 差异字段（扁平展开）。
    #[serde(flatten)]
    pub difference: DifferenceView,
    /// 处理记录时间线（按处理序号升序，不可变）。
    pub resolutions: Vec<ResolutionView>,
}

/// 差异处理动作响应视图（追加的处理记录 + 最新处理序号，供下一次动作乐观锁回传）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DifferenceActionView {
    /// 本次追加的处理记录（扁平展开）。
    #[serde(flatten)]
    pub resolution: ResolutionView,
    /// 追加后的最新处理序号（下一次处理/解决请求的 `version`）。
    pub version: u64,
}

impl From<ReconciliationDifference> for DifferenceView {
    /// 从差异实体构造视图（无处理记录时状态为 `None`）。
    ///
    /// # 参数
    /// * `difference` - 差异实体
    ///
    /// # 返回
    /// 返回差异视图。
    fn from(difference: ReconciliationDifference) -> Self {
        Self {
            id: difference.base.id,
            business_object_type: difference.business_object_type,
            business_object_id: difference.business_object_id,
            difference_type: difference.difference_type,
            left_fact_reference: difference.left_fact_reference,
            right_fact_reference: difference.right_fact_reference,
            status: None,
            version: difference.base.version,
            created_at: difference.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, DifferenceListParams, ErrorTaskListParams, InboxMessageListParams, SortDir};
    use entities::integration_ops::{ErrorClass, ErrorTaskStatus, InboxMessageStatus, MessageType};
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" received_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "received_at"],
        )
        .unwrap();
        assert_eq!(field, "received_at");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn inbox_list_params_normalize_paging_filters_and_sort_defaults() {
        let params = InboxMessageListParams {
            source_system_id: None,
            message_type: Some(MessageType::PaymentSucceeded),
            status: Some(InboxMessageStatus::Received),
            source_event_id: Some(" SO-1 ".to_string()),
            received_at_from: Some(1_700_000_000),
            received_at_to: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.message_type, Some(MessageType::PaymentSucceeded));
        assert_eq!(query.status, Some(InboxMessageStatus::Received));
        assert_eq!(query.source_event_id.as_deref(), Some("SO-1"));
        assert_eq!(query.received_at_from, Some(1_700_000_000));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn error_task_list_params_normalize_flat_filters() {
        let params = ErrorTaskListParams {
            message_id: None,
            business_object_id: Some(" so-2026-001 ".to_string()),
            error_class: Some(ErrorClass::TransientFailure),
            status: Some(ErrorTaskStatus::AutoRetrying),
            owner_role: Some(" ops ".to_string()),
            owner_user_id: Some("u-1".to_string()),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("last_attempt_at".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.business_object_id.as_deref(), Some("so-2026-001"));
        assert_eq!(query.error_class, Some(ErrorClass::TransientFailure));
        assert_eq!(query.status, Some(ErrorTaskStatus::AutoRetrying));
        assert_eq!(query.owner_role.as_deref(), Some("ops"));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "last_attempt_at");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn difference_list_params_normalize_and_reject_unbounded_page_size() {
        let params = DifferenceListParams {
            business_object_type: Some(" mall_order ".to_string()),
            business_object_id: None,
            difference_type: None,
            created_at_from: None,
            created_at_to: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());

        let params = DifferenceListParams {
            business_object_type: Some(" mall_order ".to_string()),
            business_object_id: Some("MO-1".to_string()),
            difference_type: Some("amount_mismatch".to_string()),
            created_at_from: Some(1_700_000_000),
            created_at_to: Some(1_700_000_100),
            page: Some(3),
            page_size: Some(10),
            sort_by: Some("created_at".to_string()),
            sort_dir: Some("desc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.business_object_type.as_deref(), Some("mall_order"));
        assert_eq!(query.business_object_id.as_deref(), Some("MO-1"));
        assert_eq!(query.created_at_to, Some(1_700_000_100));
        assert_eq!(query.paging.page, 3);
        assert_eq!(query.paging.page_size, 10);
    }

    #[test]
    fn reason_code_serializes_with_stable_codes() {
        use super::DifferenceReasonCode;
        use serde_json::json;

        assert_eq!(
            serde_json::to_string(&DifferenceReasonCode::SourceCorrectedAndReattributed).unwrap(),
            "\"SOURCE_CORRECTED_AND_REATTRIBUTED\""
        );
        assert_eq!(
            DifferenceReasonCode::CompensationClosed.as_str(),
            "COMPENSATION_CLOSED"
        );

        let parsed: DifferenceReasonCode =
            serde_json::from_value(json!("BUSINESS_CONFIRMED_NO_ERROR")).unwrap();
        assert_eq!(parsed, DifferenceReasonCode::BusinessConfirmedNoError);
    }
}
