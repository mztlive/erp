use entities::integration_ops::{ErrorClass, InboxMessage, InboxMessageStatus, MessageType};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// `inbox_message` 列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const INBOX_MESSAGE_SORT_FIELDS: &[&str] = &["created_at", "received_at", "status"];

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

#[cfg(test)]
mod tests {
    use super::super::common::SortDir;
    use super::InboxMessageListParams;
    use entities::integration_ops::{InboxMessageStatus, MessageType};

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
}
