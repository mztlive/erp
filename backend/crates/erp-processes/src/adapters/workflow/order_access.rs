//! 批量订单任务授权；按来源分别解析详情动作，不复制范围运算。

use std::{
    collections::{BTreeSet, HashSet},
    slice,
};

use application_core::AuditActor;
use erp_identity::access_control::ScopedObject;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_identity::{Error as IdentityError, SharedRbacService};
use erp_procurement::repository::PurchaseOrderExt;
use erp_read_models::sales_center::access::SalesAccess;
use erp_read_models::workbench::authority::WorkItemFactsReader;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::{OrderTaskSource, WorkflowScopeObject};
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

/// 绑定本地事实与运行时保持同一详情动作；销售协作与历史沿公共读模型解析。
/// 缺失来源、客户或内部部门时拒绝，不使用结算主体代替内部组织。
pub(super) async fn binding_readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    object: &WorkflowScopeObject,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let Some(source) = &object.order_source else {
        return Ok(false);
    };
    if object.business_org_unit_id.as_deref().is_none_or(str::is_empty) || object.owner_user_id.is_empty() {
        return Ok(false);
    }
    match source {
        OrderTaskSource::Sales(id) => sales_binding_readable(db, rbac, actor, object, id, executor).await,
        OrderTaskSource::Purchase(id) => {
            if !db
                .purchase_orders()
                .list_active_by_ids(slice::from_ref(id), executor)
                .await?
                .is_empty()
            {
                return Ok(purchase_sources(db, rbac, actor, slice::from_ref(id), executor)
                    .await?
                    .contains(source));
            }
            let scope = match DataScopeService::new(db.clone(), rbac.clone())
                .resolve(actor, "purchase_order", "detail", executor)
                .await
            {
                Ok(scope) => scope,
                Err(IdentityError::Forbidden(_)) => return Ok(false),
                Err(error) => return Err(error.into()),
            };
            Ok(scope.scope.allows(
                &ScopedObject {
                    owned: object.owner_user_id == actor.id(),
                    org_unit_id: object.business_org_unit_id.as_deref(),
                    collaborating: false,
                    historical_read_participant: false,
                    settlement_party_id: None,
                    warehouse_id: None,
                },
                true,
            ))
        }
    }
}

async fn sales_binding_readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    object: &WorkflowScopeObject,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let Some(customer) = &object.customer_id else {
        return Ok(false);
    };
    let (access, scope) = match SalesAccess::new(db.clone(), rbac.clone())
        .resolve(actor, "detail", &[], executor)
        .await
        .map_err(Error::from)
    {
        Ok(scope) => scope,
        Err(Error::Forbidden(_)) => return Ok(false),
        Err(error) => return Err(error),
    };
    let collaborating = scope
        .roles
        .iter()
        .chain(scope.user_limit.iter())
        .any(|clause| clause.collaborative_customer_ids.contains(customer));
    Ok(access.scope.allows(
        &ScopedObject {
            owned: object.owner_user_id == actor.id(),
            org_unit_id: object.business_org_unit_id.as_deref(),
            collaborating,
            historical_read_participant: scope.historical_order_ids.iter().any(|existing| existing == id),
            settlement_party_id: None,
            warehouse_id: None,
        },
        true,
    ))
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
