use entities::integration_ops::{ErrorClass, ErrorTaskStatus, IntegrationErrorTask, ResolutionType};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// `integration_error_task` 列表允许的排序字段白名单。
pub(crate) const ERROR_TASK_SORT_FIELDS: &[&str] = &["created_at", "last_attempt_at", "status"];

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

#[cfg(test)]
mod tests {
    use super::super::common::SortDir;
    use super::ErrorTaskListParams;
    use entities::integration_ops::{ErrorClass, ErrorTaskStatus};

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
}
