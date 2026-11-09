//! 审批 outbox 到统一通知入口的组合层适配。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::subject;
use erp_notification::{Channel, Error, NotificationData, NotificationService};
use erp_workflow::entity::approval_integration::ApprovalNotificationEventKind;
use erp_workflow::service::approval::execution::notification_worker::{
    ApprovalNotificationDelivery, DeliveryAttempt,
};
use erp_workflow::service::work_item::read::WorkItemSearchParams;
use erp_workflow::{ApprovalNotificationOutbox, WorkItemListParams, WorkItemReadService};

use super::Application;
use super::errors::{Error as ApplicationError, Result as ApplicationResult};
use crate::permissions;

pub(super) struct ApprovalNotifications(NotificationService);

impl ApprovalNotifications {
    pub(super) fn new(service: NotificationService) -> Self {
        Self(service)
    }
}

#[async_trait]
impl ApprovalNotificationDelivery for ApprovalNotifications {
    async fn deliver(&self, item: &ApprovalNotificationOutbox) -> DeliveryAttempt {
        for recipient in &item.recipient_user_ids {
            if let Err(error) = self.0.notify(message(item, recipient), &Channel::Inbox).await {
                return match error {
                    Error::Invalid(_) | Error::Conflict | Error::Rejected => DeliveryAttempt::Fatal,
                    _ => DeliveryAttempt::Retryable,
                };
            }
        }
        DeliveryAttempt::Delivered
    }
}

fn message(item: &ApprovalNotificationOutbox, recipient: &str) -> NotificationData {
    let params = &item.template_params;
    let label = event_label(item.event_kind);
    NotificationData {
        dedup_key: format!("approval:{}", item.dedup_key),
        recipient_id: recipient.to_owned(),
        title: label.to_owned(),
        body: format!(
            "{} {}：{}。当前环节：{}。",
            params.document_type_label, params.document_no, label, params.current_node_name
        ),
        // 旧 outbox 未保存可导航的实例 ID，统一进入本人队列，不从去重键猜测对象 ID。
        href: Some("/workspace/tasks".into()),
    }
}

fn event_label(event: ApprovalNotificationEventKind) -> &'static str {
    match event {
        ApprovalNotificationEventKind::Started => "审批已发起",
        ApprovalNotificationEventKind::Entered => "有新的待审批事项",
        ApprovalNotificationEventKind::NodeApproved => "审批环节已通过",
        ApprovalNotificationEventKind::NodeRejected => "审批已退回",
        ApprovalNotificationEventKind::Blocked => "审批需要处理",
        ApprovalNotificationEventKind::Resumed => "审批已恢复",
        ApprovalNotificationEventKind::Cancelled | ApprovalNotificationEventKind::BlockedCancelled => {
            "审批已取消"
        },
        ApprovalNotificationEventKind::Completed => "审批已通过",
    }
}

/// 以正式任务列表相同的授权查询计算待办徽标。
/// # 参数
/// app 为业务装配，actor 为会话身份。
/// # 返回
/// 无列表权限返回 None；有权限时返回本人开放队列总数。
/// # 错误
/// 授权或查询失败原样返回，不回退为零。
pub async fn todo_count(app: &Application, actor: &AuditActor) -> ApplicationResult<Option<i64>> {
    let allowed = app
        .rbac()
        .enforce(&subject(actor.kind(), actor.id()), &permissions::work_item::LIST.permission())
        .await?;
    if !allowed {
        return Ok(None);
    }
    let params: WorkItemListParams = serde_json::from_value(serde_json::json!({
        "scope": "mine", "page": 1, "page_size": 1, "timezone": "Asia/Shanghai"
    }))
    .map_err(|error| ApplicationError::Internal(error.to_string()))?;
    let page = WorkItemReadService::new(app.db(), app.rbac())
        .work_item_search(
            WorkItemSearchParams { base: params, filters: Some("{\"include_options\":false}".into()) },
            actor.clone(),
        )
        .await?;
    Ok(Some(page.total))
}
