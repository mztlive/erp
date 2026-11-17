//! 审批按真实业务外键取得当前订单来源。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService, Error as IdentityError};
use erp_permissions::{purchase_order, sales_order};
use erp_read_models::workbench::authority::WorkItemFactsReader;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::{Error as WorkflowError, Result as WorkflowResult};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;
use crate::wiring::errors::Error;

/// 审批沿强实体外键取得当前订单，展示根节点不提供读取权。
///
/// # 参数
/// * `db` - 订单事实所在数据库。
/// * `authorization` - 启动时完整绑定的共享授权服务。
/// * `actor` - 当前操作人。
/// * `document_type` - 审批单据类型。
/// * `document_id` - 单据编号。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 当前订单来源在详情范围内时返回 `true`；没有事实时返回 `false`。
///
/// # 错误
/// 单据类型未接入订单审批、来源事实与类型不符，或读模型查询失败时返回错误。
pub(super) async fn approval_readable(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    document_type: DocumentType,
    document_id: &str,
    executor: &mut dyn Executor,
) -> WorkflowResult<bool> {
    let kind = OrderTaskSource::approval_kind(document_type)
        .ok_or_else(|| WorkflowError::ValidationError("不是已接入的订单审批类型".into()))?;
    let key = (kind, document_id.to_string());
    let facts = WorkItemFactsReader::new(db.clone())
        .load(&HashSet::from([key.clone()]), executor)
        .await
        .map_err(|error| map_service(Error::from(error)))?;
    let Some(fact) = facts.get(&key) else {
        return Ok(false);
    };
    let source = fact
        .order_scope_source
        .as_ref()
        .filter(|source| source.matches_kind(kind))
        .ok_or_else(|| WorkflowError::Internal("订单审批缺少权威来源".into()))?;
    let (permission, id) = match source {
        OrderTaskSource::Sales(id) => (&sales_order::DETAIL, id),
        OrderTaskSource::Purchase(id) => (&purchase_order::DETAIL, id),
    };
    match authorization.require(actor, permission, AccessTarget::record(id)?, executor).await {
        Ok(()) => Ok(true),
        Err(IdentityError::Forbidden(_) | IdentityError::NotFound(_)) => Ok(false),
        Err(error) => Err(map_service(Error::from(error))),
    }
}
