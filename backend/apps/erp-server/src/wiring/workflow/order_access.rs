//! 批量订单任务授权；按来源分别解析详情动作，不复制范围运算。

use std::collections::{BTreeSet, HashSet};

use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_identity::entity::resource_access::ResourceKind;
use erp_read_models::workbench::authority::WorkItemFactsReader;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::{Error as WorkflowError, Result as WorkflowResult};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;
use crate::wiring::errors::{Error, Result};

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

/// 每种订单解析一次权限并批量加载对象；任务重复引用不引起逐任务查询。
///
/// # 参数
/// * `db` - 销售与采购订单所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
/// * `actor` - 当前操作人。
/// * `sources` - 待判定的订单来源。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回详情范围允许的来源子集。
///
/// # 错误
/// 来源超过 500 条时返回校验错误。无详情权限或未找到只过滤该资源；配置、版本与基础设施错误原样返回。
pub(super) async fn readable_sources(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    sources: &BTreeSet<OrderTaskSource>,
    executor: &mut dyn Executor,
) -> WorkflowResult<BTreeSet<OrderTaskSource>> {
    if sources.len() > 500 {
        return Err(WorkflowError::ValidationError("单批任务订单来源超过 500 条".into()));
    }
    let mut allowed = BTreeSet::new();
    for kind in [ResourceKind::SalesOrder, ResourceKind::PurchaseOrder] {
        let ids = sources
            .iter()
            .filter_map(|source| match (kind, source) {
                (ResourceKind::SalesOrder, OrderTaskSource::Sales(id))
                | (ResourceKind::PurchaseOrder, OrderTaskSource::Purchase(id)) => Some(id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let result =
            super::resource_access::readable_batch(db, rbac, actor, kind, &ids, "work_item:list", executor)
                .await
                .map(|ids| {
                    ids.into_iter()
                        .map(|id| match kind {
                            ResourceKind::SalesOrder => OrderTaskSource::Sales(id),
                            ResourceKind::PurchaseOrder => OrderTaskSource::Purchase(id),
                        })
                        .collect()
                });
        allowed.extend(read_result(result)?);
    }
    Ok(allowed)
}

/// 无对象详情权限只过滤该资源；配置、版本与基础设施错误不得转换成授权。
fn read_result(result: Result<Vec<OrderTaskSource>>) -> WorkflowResult<Vec<OrderTaskSource>> {
    match result {
        Ok(sources) => Ok(sources),
        Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(Vec::new()),
        Err(error) => Err(map_service(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_resource_is_empty_but_configuration_and_version_errors_propagate() {
        assert!(read_result(Err(Error::Forbidden("revoked".into()))).unwrap().is_empty());
        assert!(read_result(Err(Error::ValidationError("unwired dimension".into()))).is_err());
        assert!(read_result(Err(Error::ConflictError("DATA_SCOPE_CHANGED".into()))).is_err());
        assert_eq!(
            read_result(Ok(vec![OrderTaskSource::Sales("s1".into())])).unwrap(),
            vec![OrderTaskSource::Sales("s1".into())]
        );
    }
}
