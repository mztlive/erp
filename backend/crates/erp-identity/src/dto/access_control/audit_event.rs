//! 域 D06 `access_control` 的 审计事件 DTO。

use application_core::normalized_text;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::PageParams;
use crate::entity::access_control::{AuditEvent, AuditEventResult};
use crate::error::Result;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuditEventView {
    /// 实体主键。
    pub id: String,
    /// 操作者 ID。
    pub actor_id: String,
    /// 操作者名称快照。
    pub actor_label: String,
    /// 责任角色快照。
    pub actor_role: String,
    /// 动作代码。
    pub action_type: String,
    /// 业务对象类型代码。
    pub object_type: String,
    /// 业务对象 ID。
    pub object_id: Option<String>,
    /// 业务对象安全标题。
    pub object_label: Option<String>,
    /// 请求追踪号。
    pub request_id: Option<String>,
    /// 链路追踪号。
    pub trace_id: Option<String>,
    /// 最终结果。
    pub result: AuditEventResult,
    /// 变更字段名（只记录字段名和「已变更」）。
    pub changed_field_names: Vec<String>,
    /// 安全摘要。
    pub safe_digest: Option<String>,
    /// 来源 IP。
    pub source_ip: Option<String>,
    /// 设备上下文。
    pub device_context: Option<String>,
    /// 创建时间（秒级时间戳，即事件发生时间）。
    pub created_at: u64,
}

impl From<AuditEvent> for AuditEventView {
    /// 从实体构造响应视图。
    fn from(event: AuditEvent) -> Self {
        Self {
            id: event.base.id,
            actor_id: event.actor_id,
            actor_label: event.actor_label,
            actor_role: event.actor_role,
            action_type: event.action_type,
            object_type: event.object_type,
            object_id: event.object_id,
            object_label: event.object_label,
            request_id: event.request_id,
            trace_id: event.trace_id,
            result: event.result,
            changed_field_names: event.changed_field_names,
            safe_digest: event.safe_digest,
            source_ip: event.source_ip,
            device_context: event.device_context,
            created_at: event.base.created_at,
        }
    }
}

impl From<crate::repository::AuditEventRow> for AuditEventView {
    /// 从列表投影行构造响应视图（`trace_id`/`safe_digest`/`device_context`
    /// 非投影字段，列表视图固定为 `None`，与现有手工映射一致）。
    fn from(row: crate::repository::AuditEventRow) -> Self {
        Self {
            id: row.id,
            actor_id: row.actor_id,
            actor_label: row.actor_label,
            actor_role: row.actor_role,
            action_type: row.action_type,
            object_type: row.object_type,
            object_id: row.object_id,
            object_label: row.object_label,
            request_id: row.request_id,
            trace_id: None,
            result: row.result,
            changed_field_names: row.changed_field_names,
            safe_digest: None,
            source_ip: row.source_ip,
            device_context: None,
            created_at: row.created_at,
        }
    }
}

/// 审计事件列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct AuditEventListParams {
    /// 操作者、动作、对象或追踪号字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,
    /// 界面动作标签匹配的动作代码（逗号分隔），只作为关键词 OR 条件。
    pub keyword_actions: Option<String>,
    /// 审计事件稳定身份。
    pub event_id: Option<String>,
    /// 链路追踪号或请求号精确筛选。
    pub trace_id: Option<String>,
    /// 创建时间下界（含，Unix 秒）。
    pub created_from: Option<u64>,
    /// 创建时间上界（不含，Unix 秒）。
    pub created_before: Option<u64>,

    /// 操作者 ID 模糊筛选（忽略大小写）。
    pub actor_id: Option<String>,
    /// 动作代码模糊筛选（忽略大小写）。
    pub action_type: Option<String>,
    /// 业务对象类型代码筛选。
    pub object_type: Option<String>,
    /// 业务对象 ID 筛选。
    pub object_id: Option<String>,
    /// 最终结果筛选。
    pub result: Option<AuditEventResult>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的审计事件列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuditEventListQuery {
    /// 操作者、动作、对象或追踪号字面量关键词。
    pub q: Option<String>,
    /// 界面动作标签匹配的动作代码（逗号分隔），只作为关键词 OR 条件。
    pub keyword_actions: Option<String>,
    /// 审计事件稳定身份。
    pub event_id: Option<String>,
    /// 链路追踪号或请求号精确筛选。
    pub trace_id: Option<String>,
    /// 创建时间下界（含，Unix 秒）。
    pub created_from: Option<u64>,
    /// 创建时间上界（不含，Unix 秒）。
    pub created_before: Option<u64>,

    /// 操作者 ID 模糊筛选。
    pub actor_id: Option<String>,
    /// 动作代码模糊筛选。
    pub action_type: Option<String>,
    /// 业务对象类型代码筛选。
    pub object_type: Option<String>,
    /// 业务对象 ID 筛选。
    pub object_id: Option<String>,
    /// 最终结果筛选。
    pub result: Option<AuditEventResult>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl AuditEventListParams {
    /// 归一化审计事件列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<AuditEventListQuery> {
        Ok(AuditEventListQuery {
            q: normalized_text(self.q.as_deref()),
            keyword_actions: normalized_text(self.keyword_actions.as_deref()),
            event_id: normalized_text(self.event_id.as_deref()),
            trace_id: normalized_text(self.trace_id.as_deref()),
            created_from: self.created_from,
            created_before: self.created_before,
            actor_id: normalized_text(self.actor_id.as_deref()),
            action_type: normalized_text(self.action_type.as_deref()),
            object_type: normalized_text(self.object_type.as_deref()),
            object_id: normalized_text(self.object_id.as_deref()),
            result: self.result,
            paging: super::page_params(&self.sort_by, &self.sort_dir, self.page, self.page_size)?,
        })
    }
}
