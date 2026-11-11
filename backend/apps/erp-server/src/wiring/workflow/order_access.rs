//! 审批按真实业务外键取得当前订单来源。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_identity::{Permission, SharedRbacService};
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
/// * `rbac` - 现有 RBAC 快照服务。
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
    rbac: &SharedRbacService,
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
    super::resource_access::readable(db, rbac, actor, source, "approval_instance:read", executor)
        .await
        .map_err(map_service)
}

/// 在原事务取得强外键来源并执行完整管理操作的同次 identity 对象授权。
/// # 参数
/// db、rbac、actor 为基础设施及账号；operation 为固定入口及完整必需权限组合；目标与executor为真实业务对象和原事务。
/// # 返回
/// 实际来源及完整资格同时成立时成功。
/// # 错误
/// 来源缺失、类型错配或授权失败拒绝。
pub(super) async fn require_management(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    operation: (&Permission, &[Permission]),
    document_type: DocumentType,
    document_id: &str,
    executor: &mut dyn Executor,
) -> WorkflowResult<()> {
    let (entry, required) = operation;
    let kind = OrderTaskSource::approval_kind(document_type)
        .ok_or_else(|| WorkflowError::ValidationError("不是已接入的订单审批类型".into()))?;
    let key = (kind, document_id.to_string());
    let facts = WorkItemFactsReader::new(db.clone())
        .load(&HashSet::from([key.clone()]), executor)
        .await
        .map_err(|error| map_service(Error::from(error)))?;
    let fact = facts.get(&key).ok_or_else(|| WorkflowError::NotFound("审批来源不存在".into()))?;
    let source = fact
        .order_scope_source
        .as_ref()
        .filter(|source| source.matches_kind(kind))
        .ok_or_else(|| WorkflowError::Internal("订单审批缺少权威来源".into()))?;
    super::resource_access::require_source_operation(db, rbac, actor, source, entry, required, executor)
        .await
        .map_err(map_service)
}
