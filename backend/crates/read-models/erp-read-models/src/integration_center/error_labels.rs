//! 入站消息和类型明确的供应商关联对象名称；仅投影，不参与权限判定。

use std::collections::HashMap;

use erp_integration::dto::ErrorTaskView;
use erp_integration::entity::integration_ops::{InboxMessage, MessageType};
use erp_integration::repository::IntegrationOpsExt;
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::repository::{SupplierApiExt, SupplierFulfillmentExt};
use erp_support::SourceRegistryExt;
use erp_support::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

#[derive(Default)]
pub(crate) struct IntegrationTaskLabels {
    pub(crate) business_object_label: Option<String>,
    pub(crate) message_label: Option<String>,
}

/// 批量解析已授权任务的关联消息与精确供应商履约单号。
///
/// # 参数
/// `db` 为领域仓储入口，`tasks` 为当前授权页，`executor` 沿用调用方读取快照。
/// # 返回
/// 返回按任务身份索引的展示；未知 payload、错配来源或缺失对象没有业务名称。
/// # 错误
/// 关联仓储读取失败时返回错误。
pub(crate) async fn integration_error_labels(
    db: &Database,
    tasks: &[ErrorTaskView],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, IntegrationTaskLabels>> {
    let ids = tasks.iter().filter_map(|task| task.message_id.clone()).collect::<Vec<_>>();
    let messages = if ids.is_empty() {
        Vec::new()
    } else {
        db.inbox_messages().list_active_by_ids(&ids, executor).await?
    };
    let mut labels = connection_task_labels(db, tasks, executor).await?;
    let source_labels = source_labels(db, &messages, executor).await?;
    let orders = message_orders(db, &messages, executor).await?;
    let messages =
        messages.into_iter().map(|message| (message.base.id.clone(), message)).collect::<HashMap<_, _>>();
    labels.extend(tasks.iter().filter_map(|task| {
        let message = messages.get(task.message_id.as_deref()?)?;
        let source = source_labels
            .get(message.source_system_id.as_ref())
            .map(String::as_str)
            .unwrap_or("来源名称未维护");
        let order = orders.get(&message.base.id);
        Some((
            task.id.clone(),
            IntegrationTaskLabels {
                message_label: Some(format!("{source} · {}", message.message_type.label())),
                business_object_label: order.and_then(|order| {
                    associated_order_label(task.business_object_id.as_deref(), message, order)
                }),
            },
        ))
    }));
    Ok(labels)
}

/// W20 健康检查生产者以固定任务身份关联供应商连接，普通无类型任务不走此读取。
async fn connection_task_labels(
    db: &Database,
    tasks: &[ErrorTaskView],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, IntegrationTaskLabels>> {
    let ids = tasks.iter().filter_map(connection_task_identity).map(str::to_string).collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let connections = db
        .supplier_api_connections()
        .list_active_by_ids(&ids, executor)
        .await?
        .into_iter()
        .filter(|connection| !connection.connection_code.trim().is_empty())
        .map(|connection| (connection.base.id, format!("供应商接口 {}", connection.connection_code)))
        .collect::<HashMap<_, _>>();
    Ok(tasks
        .iter()
        .filter_map(|task| {
            let name = connections.get(connection_task_identity(task)?)?;
            Some((
                task.id.clone(),
                IntegrationTaskLabels { business_object_label: Some(name.clone()), message_label: None },
            ))
        })
        .collect())
}

fn connection_task_identity(task: &ErrorTaskView) -> Option<&str> {
    let digest = task.id.strip_prefix("w20-error-")?;
    (digest.len() == 64 && digest.chars().all(|ch| ch.is_ascii_hexdigit()) && task.message_id.is_none())
        .then_some(task.business_object_id.as_deref())
        .flatten()
}

async fn source_labels(
    db: &Database,
    messages: &[InboxMessage],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, String>> {
    let system_ids = messages.iter().map(|message| message.source_system_id.clone()).collect::<Vec<_>>();
    let mut names = db
        .source_systems()
        .find_systems_by_ids(&system_ids, executor)
        .await?
        .into_iter()
        .filter(|system| !system.name.trim().is_empty())
        .map(|system| (system.base.id, system.name))
        .collect::<HashMap<_, _>>();
    let ids = messages
        .iter()
        .filter_map(|message| supplier_connection_id(message.source_system_id.as_ref()))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !ids.is_empty() {
        let connections = db.supplier_api_connections().list_active_by_ids(&ids, executor).await?;
        names.extend(
            connections.into_iter().filter(|connection| !connection.connection_code.trim().is_empty()).map(
                |connection| {
                    (
                        format!("supplier-api:{}", connection.base.id),
                        format!("供应商接口 {}", connection.connection_code),
                    )
                },
            ),
        );
    }
    Ok(names)
}

async fn message_orders(
    db: &Database,
    messages: &[InboxMessage],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, SupplierFulfillmentOrder>> {
    let action_ids = messages
        .iter()
        .filter_map(|message| supplier_payload(message))
        .filter(|(kind, _)| *kind == "supplier-order-action")
        .map(|(_, id)| id.to_string())
        .collect::<Vec<_>>();
    let actions = if action_ids.is_empty() {
        Vec::new()
    } else {
        db.supplier_order_actions().list_active_by_ids(&action_ids, executor).await?
    };
    let actions = actions
        .into_iter()
        .map(|action| (action.base.id, action.supplier_fulfillment_order_id.to_string()))
        .collect::<HashMap<_, _>>();
    let message_orders = messages
        .iter()
        .filter_map(|message| {
            let (kind, id) = supplier_payload(message)?;
            let order_id = match kind {
                "supplier-order-action" => actions.get(id)?.clone(),
                "supplier-refund-order" => id.to_string(),
                _ => return None,
            };
            Some((message.base.id.clone(), order_id))
        })
        .collect::<HashMap<_, _>>();
    let ids = message_orders.values().cloned().collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let orders = db
        .supplier_fulfillment_orders()
        .list_active_by_ids(&ids, executor)
        .await?
        .into_iter()
        .map(|order| (order.base.id.clone(), order))
        .collect::<HashMap<_, _>>();
    Ok(message_orders
        .into_iter()
        .filter_map(|(message_id, order_id)| Some((message_id, orders.get(&order_id)?.clone())))
        .collect())
}

fn supplier_payload(message: &InboxMessage) -> Option<(&str, &str)> {
    if message.message_type != MessageType::SupplierCallback {
        return None;
    }
    let (kind, id) = message.payload_reference.as_deref()?.split_once(':')?;
    (matches!(kind, "supplier-order-action" | "supplier-refund-order") && valid_identity(id))
        .then_some((kind, id))
}

fn supplier_connection_id(source: &str) -> Option<&str> {
    source.strip_prefix("supplier-api:").filter(|id| valid_identity(id))
}

fn valid_identity(id: &str) -> bool {
    !id.is_empty() && id.trim() == id && !id.contains([':', ';', '/'])
}

fn associated_order_label(
    business_id: Option<&str>,
    message: &InboxMessage,
    order: &SupplierFulfillmentOrder,
) -> Option<String> {
    if business_id.is_some_and(|id| id != order.base.id)
        || supplier_connection_id(message.source_system_id.as_ref()) != Some(order.connection_id.as_ref())
        || order.fulfillment_order_no.trim().is_empty()
    {
        return None;
    }
    Some(order.fulfillment_order_no.clone())
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        InboxMessageId, IntegrationErrorTaskId, SourceSystemId, SupplierAccountId, SupplierApiConnectionId,
        SupplierFulfillmentOrderId,
    };
    use erp_integration::entity::integration_ops::{
        ErrorClass, InboxMessageData, InboxMessageStatus, IntegrationErrorTask, IntegrationErrorTaskData,
    };
    use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrderData;

    use super::*;

    fn message(payload: &str) -> InboxMessage {
        InboxMessage::new(
            InboxMessageId::new("message-1"),
            InboxMessageData {
                source_system_id: SourceSystemId::new("supplier-api:connection-1"),
                source_event_id: "event-1".into(),
                message_type: MessageType::SupplierCallback,
                business_fact_key: "event-1".into(),
                payload_schema_version: "1.0".into(),
                payload_reference: Some(payload.into()),
                status: InboxMessageStatus::Received,
                source_sent_at: None,
                received_at: Instant::from_unix_secs(1_700_000_000),
                processed_at: None,
            },
        )
        .unwrap()
    }

    fn order() -> SupplierFulfillmentOrder {
        SupplierFulfillmentOrder::new(
            SupplierFulfillmentOrderId::new("order-1"),
            SupplierFulfillmentOrderData::submitting(
                "SF-1001",
                SupplierAccountId::new("supplier-1"),
                SupplierApiConnectionId::new("connection-1"),
                1,
                Instant::from_unix_secs(1_700_000_000),
                "encrypted-address",
                "hmac-fingerprint",
            )
            .with_follow_up("operator-1", "org-ops"),
        )
        .unwrap()
    }

    #[test]
    fn synthetic_supplier_source_accepts_one_exact_connection_identity() {
        assert_eq!(supplier_connection_id("supplier-api:connection-1"), Some("connection-1"));
        assert!(supplier_connection_id("supplier-api:").is_none());
        assert!(supplier_connection_id("supplier-api:connection-1:extra").is_none());
        assert!(supplier_connection_id("external:connection-1").is_none());
    }

    #[test]
    fn supplier_payload_requires_exact_registered_kind_and_identity() {
        let mut message = message("supplier-order-action:action-1");
        assert_eq!(supplier_payload(&message), Some(("supplier-order-action", "action-1")));
        message.payload_reference = Some("supplier-refund-order:order-1".into());
        assert_eq!(supplier_payload(&message), Some(("supplier-refund-order", "order-1")));
        message.payload_reference = Some("supplier-order-action:action-1:extra".into());
        assert!(supplier_payload(&message).is_none());
        message.payload_reference = Some("unknown-type:action-1".into());
        assert!(supplier_payload(&message).is_none());
        message.payload_reference = Some("supplier-order-action:action-1".into());
        message.message_type = MessageType::PaymentSucceeded;
        assert!(supplier_payload(&message).is_none());
    }

    #[test]
    fn business_name_requires_matching_task_object_and_source_connection() {
        let mut message = message("supplier-refund-order:order-1");
        let order = order();
        assert_eq!(associated_order_label(Some("order-1"), &message, &order).as_deref(), Some("SF-1001"));
        assert!(associated_order_label(Some("another-order"), &message, &order).is_none());
        message.source_system_id = SourceSystemId::new("supplier-api:another-connection");
        assert!(associated_order_label(Some("order-1"), &message, &order).is_none());
    }
    #[test]
    fn health_connection_identity_requires_registered_task_shape_without_message() {
        let mut task = ErrorTaskView::from(
            IntegrationErrorTask::new(
                IntegrationErrorTaskId::new(format!("w20-error-{}", "a".repeat(64))),
                IntegrationErrorTaskData {
                    message_id: None,
                    business_object_id: Some("connection-1".into()),
                    error_class: ErrorClass::AuthSignature,
                    owner_role: Some("integration-operator".into()),
                    owner_user_id: Some("operator-1".into()),
                    owner_org_unit_id: "org-ops".into(),
                },
            )
            .unwrap(),
        );
        assert_eq!(connection_task_identity(&task), Some("connection-1"));
        task.message_id = Some("message-1".into());
        assert!(connection_task_identity(&task).is_none());
        task.message_id = None;
        task.id = "w20-error-not-a-health-task".into();
        assert!(connection_task_identity(&task).is_none());
    }
}
