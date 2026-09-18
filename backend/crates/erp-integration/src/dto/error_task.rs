use application_core::{QueryIds, normalized_text};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::common::{PageParams, normalize_scoped_list};
use super::task_decision::{ControlledEvidenceRef, ResolutionEvidencePolicyView};
use crate::Result;
use crate::entity::integration_ops::{ErrorClass, ErrorTaskStatus, IntegrationErrorTask, ResolutionType};

/// `integration_error_task` 列表允许的排序字段白名单。
pub(crate) const ERROR_TASK_SORT_FIELDS: &[&str] = &["created_at", "last_attempt_at", "status"];

/// 错误任务登记请求（消息类失败必填 `message_id`，业务对象类失败必填 `business_object_id`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateErrorTaskRequest {
    /// 关联的消息 ID（消息类失败必填其一）。
    pub message_id: Option<crate::entity::integration_ops::InboxMessageId>,
    /// 关联的业务对象 ID（非消息类失败必填其一）。
    #[validate(length(max = 128, message = "业务对象ID过长"))]
    pub business_object_id: Option<String>,
    /// 错误分类。
    pub error_class: ErrorClass,
    /// 创建时明确指定的当前责任人。
    #[validate(length(min = 1, max = 128, message = "责任人不能为空或过长"))]
    pub owner_user_id: String,
}

/// 错误任务列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ErrorTaskListParams {
    /// 编号、业务对象、事件或差异摘要的字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,

    /// 关联消息 ID 筛选。
    pub message_id: Option<crate::entity::integration_ops::InboxMessageId>,
    /// 关联业务对象 ID 筛选。
    pub business_object_id: Option<String>,
    /// 错误分类筛选。
    pub error_class: Option<ErrorClass>,
    /// 任务状态筛选。
    pub status: Option<ErrorTaskStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 当前处理人稳定 ID 列表。
    pub handler_user_ids: Option<QueryIds>,
    /// 历史处理人稳定 ID 列表（已办 `completed_by`）。
    pub operator_user_ids: Option<QueryIds>,
    /// 当前处理人所属内部组织。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 跨页必须携带的范围版本。
    pub scope_version: Option<String>,
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
    /// 编号、业务对象、事件或差异摘要的字面量关键词。
    pub q: Option<String>,

    /// 关联消息 ID 筛选。
    pub message_id: Option<crate::entity::integration_ops::InboxMessageId>,
    /// 关联业务对象 ID 筛选。
    pub business_object_id: Option<String>,
    /// 错误分类筛选。
    pub error_class: Option<ErrorClass>,
    /// 任务状态筛选。
    pub status: Option<ErrorTaskStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 当前处理人稳定 ID。
    pub handler_user_ids: Vec<String>,
    /// 历史处理人稳定 ID。
    pub operator_user_ids: Vec<String>,
    /// 当前处理人所属内部组织。
    pub org_unit_ids: Vec<String>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: bool,
    /// 跨页范围版本。
    pub scope_version: Option<String>,
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
        if self.scope_version.as_ref().is_some_and(|version| version.is_empty() || version.len() > 256) {
            return Err(crate::Error::ValidationError("范围版本非法".into()));
        }
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        let handler_user_ids = self.handler_user_ids.as_ref().map(QueryIds::as_slice).unwrap_or(&[]).to_vec();
        let operator_user_ids =
            self.operator_user_ids.as_ref().map(QueryIds::as_slice).unwrap_or(&[]).to_vec();
        reject_me_ids(&handler_user_ids, "当前处理人")?;
        reject_me_ids(&operator_user_ids, "历史处理人")?;
        Ok(ErrorTaskListQuery {
            q: normalized_text(self.q.as_deref()),
            message_id: self.message_id.clone(),
            business_object_id: normalized_text(self.business_object_id.as_deref()),
            error_class: self.error_class,
            status: self.status,
            owner_role: normalized_text(self.owner_role.as_deref()),
            handler_user_ids,
            operator_user_ids,
            org_unit_ids: self.org_unit_ids.as_ref().map(QueryIds::as_slice).unwrap_or(&[]).to_vec(),
            include_descendants: self.include_descendants.unwrap_or(false),
            scope_version: self.scope_version.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
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

/// 带范围版本的错误任务列表响应。
#[derive(Debug, Clone, Serialize)]
pub struct ErrorTaskListView {
    /// 分页结果。
    #[serde(flatten)]
    pub data: super::PageView<ErrorTaskView>,
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`。
    pub empty_reason: Option<&'static str>,
    /// 当前范围口径摘要。
    pub scope_summary: &'static str,
    /// 当前处理人权威来源。
    pub ownership_basis: &'static str,
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
    /// 服务端按当前领域事实开放的 W29 强动作。
    pub allowed_actions: Vec<String>,
    /// 当前阻断原因；未形成可验证结果时必须显式返回。
    pub action_blockers: Vec<ActionBlockerView>,
    /// 服务端发现并重验的受控证据；客户端不得自行补造引用。
    pub linked_evidence: Vec<ControlledEvidenceRef>,
    /// 当前业务项固定终态证据策略；无正式任务或已终结时为空。
    pub resolution_evidence_policy: Option<ResolutionEvidencePolicyView>,
}

/// W29 业务动作阻断投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActionBlockerView {
    /// 被阻断的动作代码。
    pub action: String,
    /// 稳定阻断代码。
    pub code: String,
    /// 面向处理人的明确说明。
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::super::common::SortDir;
    use super::ErrorTaskListParams;
    use crate::entity::integration_ops::{ErrorClass, ErrorTaskStatus};

    #[test]
    fn error_task_list_params_default_is_empty() {
        let params = ErrorTaskListParams::default();
        assert!(params.q.is_none() && params.page.is_none());
    }

    #[test]
    fn error_task_list_params_normalize_flat_filters() {
        let params = ErrorTaskListParams {
            q: Some("  needle.[x]  ".to_string()),
            message_id: None,
            business_object_id: Some(" so-2026-001 ".to_string()),
            error_class: Some(ErrorClass::TransientFailure),
            status: Some(ErrorTaskStatus::AutoRetrying),
            owner_role: Some(" ops ".to_string()),
            handler_user_ids: Some(serde_json::from_value(serde_json::json!("user-2,user-1")).unwrap()),
            operator_user_ids: None,
            org_unit_ids: None,
            include_descendants: None,
            scope_version: Some("v1".into()),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("last_attempt_at".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.q.as_deref(), Some("needle.[x]"));
        assert_eq!(query.business_object_id.as_deref(), Some("so-2026-001"));
        assert_eq!(query.error_class, Some(ErrorClass::TransientFailure));
        assert_eq!(query.status, Some(ErrorTaskStatus::AutoRetrying));
        assert_eq!(query.owner_role.as_deref(), Some("ops"));
        assert_eq!(query.handler_user_ids, vec!["user-1".to_string(), "user-2".to_string()]);
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "last_attempt_at");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn error_task_list_params_reject_me_as_person_id() {
        let params = ErrorTaskListParams {
            handler_user_ids: Some(serde_json::from_value(serde_json::json!("me")).unwrap()),
            ..ErrorTaskListParams::default()
        };
        assert!(params.normalized().is_err());
        let operators = ErrorTaskListParams {
            operator_user_ids: Some(serde_json::from_value(serde_json::json!("ME")).unwrap()),
            ..ErrorTaskListParams::default()
        };
        assert!(operators.normalized().is_err());
    }
}
