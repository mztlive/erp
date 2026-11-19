//! 管理端审计事件的查询参数和列表响应。

use application_core::normalized_text;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::PageParams;
use crate::entity::access_control::AuditEventResult;
use crate::error::Result;
use crate::repository::AuditEventFilter;

/// 只读审计列表项。列表仅加载用于展示的安全字段。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuditEventView {
    /// 审计事件的稳定 ID。
    pub id: String,
    /// 发起操作的账号 ID。
    pub actor_id: String,
    /// 事件发生时记录的操作者显示名。
    pub actor_label: String,
    /// 事件发生时记录的责任角色标识。
    pub actor_role: String,
    /// 审计目录中的动作代码。
    pub action_type: String,
    /// 此次操作针对的业务对象类型代码。
    pub object_type: String,
    /// 此次操作关联的业务对象 ID（若有）。
    pub object_id: Option<String>,
    /// 事件记录中可安全展示的对象标题（若有）。
    pub object_label: Option<String>,
    /// 用于关联同一请求下各事件的请求号（若有）。
    pub request_id: Option<String>,
    /// 链路追踪号；当前列表留空。
    pub trace_id: Option<String>,
    /// 记录的操作结果：成功、拒绝、失败或未知。
    pub result: AuditEventResult,
    /// 发生变更的字段名；只标识变更，省略新旧值。
    pub changed_field_names: Vec<String>,
    /// 安全摘要；当前列表留空。
    pub safe_digest: Option<String>,
    /// 事件记录中的来源 IP 地址（若有）。
    pub source_ip: Option<String>,
    /// 设备上下文；当前列表留空。
    pub device_context: Option<String>,
    /// 事件发生时间，以 Unix 秒记录。
    pub created_at: u64,
}

impl From<crate::repository::AuditEventRow> for AuditEventView {
    // 将仓储投影转为列表响应；链路追踪号、安全摘要和设备上下文固定留空。
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

/// 审计列表的筛选和分页参数，按同一层级传入。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct AuditEventListParams {
    /// 对操作者、动作、对象或追踪号做不区分大小写的字面量匹配，最多 200 个字符。
    #[validate(length(max = 200, message = "搜索关键词不能超过 200 个字符"))]
    pub q: Option<String>,
    /// 界面按动作标签匹配出的逗号分隔动作代码；有关键词时，动作代码或任一文本字段命中即可。
    pub keyword_actions: Option<String>,
    /// 精确匹配审计事件 ID。
    pub event_id: Option<String>,
    /// 精确匹配链路追踪号或请求号，任一命中即可。
    pub trace_id: Option<String>,
    /// 事件发生时间下界（含，Unix 秒）。
    pub created_from: Option<u64>,
    /// 事件发生时间上界（不含，Unix 秒）。
    pub created_before: Option<u64>,

    /// 按操作者 ID 做不区分大小写的字面量匹配。
    pub actor_id: Option<String>,
    /// 按动作代码做不区分大小写的字面量匹配。
    pub action_type: Option<String>,
    /// 精确匹配业务对象类型代码。
    pub object_type: Option<String>,
    /// 精确匹配业务对象 ID。
    pub object_id: Option<String>,
    /// 精确匹配操作结果。
    pub result: Option<AuditEventResult>,
    /// 页码，从 1 开始；默认第 1 页。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数，范围为 1–100；默认 20 条。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段，仅支持 `created_at` 和 `updated_at`；默认 `created_at`。
    pub sort_by: Option<String>,
    /// 排序方向，接受 `asc` 或 `desc`；默认降序。
    pub sort_dir: Option<String>,
}

impl AuditEventListParams {
    /// 裁剪文本筛选项，忽略纯空白值，补齐分页和排序默认值后生成仓储筛选条件。
    /// 分页越界、排序字段超出白名单或方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<AuditEventFilter> {
        Ok(AuditEventFilter {
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
            paging: PageParams::parse(&self.sort_by, &self.sort_dir, self.page, self.page_size)?,
        })
    }
}
