//! 批量订单任务授权；按来源分别解析详情动作，不复制范围运算。

use std::collections::{BTreeSet, HashSet};

use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_procurement::repository::PurchaseOrderExt;
use erp_read_models::sales_center::access::SalesAccess;
use erp_read_models::workbench::authority::WorkItemFactsReader;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::{Error as WorkflowError, Result as WorkflowResult};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;
use crate::adapters::purchase_access;
use crate::{Error, Result};

/// 审批沿强实体外键取得当前订单，展示根节点不提供读取权。
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
    let sources = BTreeSet::from([source.clone()]);
    Ok(readable_sources(db, rbac, actor, &sources, executor)
        .await?
        .contains(source))
}

/// 每种订单解析一次权限并批量加载对象；任务重复引用不引起逐任务查询。
pub(super) async fn readable_sources(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    sources: &BTreeSet<OrderTaskSource>,
    executor: &mut dyn Executor,
) -> WorkflowResult<BTreeSet<OrderTaskSource>> {
    if sources.len() > 500 {
        return Err(WorkflowError::ValidationError(
            "单批任务订单来源超过 500 条".into(),
        ));
    }
    let sales = sources
        .iter()
        .filter_map(|source| match source {
            OrderTaskSource::Sales(id) => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let purchases = sources
        .iter()
        .filter_map(|source| match source {
            OrderTaskSource::Purchase(id) => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut allowed = BTreeSet::new();
    allowed.extend(read_result(
        sales_sources(db, rbac, actor, &sales, executor).await,
    )?);
    allowed.extend(read_result(
        purchase_sources(db, rbac, actor, &purchases, executor).await,
    )?);
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

async fn sales_sources(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<OrderTaskSource>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let (context, scope) = SalesAccess::new(db.clone(), rbac.clone())
        .resolve(actor, "detail", &[], executor)
        .await?;
    let mut allowed = Vec::new();
    for order in db.sales_orders().list_active_by_ids(ids, executor).await? {
        if SalesAccess::allows(&context, &scope, &order)? {
            allowed.push(OrderTaskSource::Sales(order.base.id));
        }
    }
    Ok(allowed)
}

async fn purchase_sources(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<OrderTaskSource>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let access = purchase_access(db.clone(), rbac.clone());
    let (context, scope) = access.resolve(actor, "detail", executor).await?;
    let mut allowed = Vec::new();
    for order in db.purchase_orders().list_active_by_ids(ids, executor).await? {
        if access.allows(&context, &scope, &order)? {
            allowed.push(OrderTaskSource::Purchase(order.base.id));
        }
    }
    Ok(allowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_resource_is_empty_but_configuration_and_version_errors_propagate() {
        assert!(read_result(Err(Error::Forbidden("revoked".into())))
            .unwrap()
            .is_empty());
        assert!(read_result(Err(Error::ValidationError("unwired dimension".into()))).is_err());
        assert!(read_result(Err(Error::ConflictError("DATA_SCOPE_CHANGED".into()))).is_err());
        assert_eq!(
            read_result(Ok(vec![OrderTaskSource::Sales("s1".into())])).unwrap(),
            vec![OrderTaskSource::Sales("s1".into())]
        );
    }
}
