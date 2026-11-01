//! 订单与供应商申请任务的独立对象授权；读模型与任务命令复用同一合同。

use std::collections::BTreeSet;

use application_core::AuditActor;
use persistence_core::Executor;

use crate::error::{Error, Result};
use crate::ports::{ObjectFact, ObjectFactMap, ObjectKind, OrderTaskSource, WorkflowAuthorizationPort};

/// 重验供应商申请任务的内部账号及独立对象读取范围。
///
/// # 参数
/// * `auth` - 已装配的工作流授权 Port。
/// * `actor_id` - 操作人或转交候选人的当前账号 ID。
/// * `kind` - 固定任务关系中的对象种类。
/// * `object_id` - 权威对象主键，不从展示字段推断。
/// * `executor` - 调用方事务或查询执行器。
/// # 返回
/// 供应商申请详情范围满足时返回成功；其他任务沿各自授权政策继续执行。
/// # 错误
/// 内部身份失效、范围越界或适配器未装配时失败关闭。
pub async fn require_supplier_portal_task_read(
    auth: &impl WorkflowAuthorizationPort,
    actor_id: &str,
    kind: ObjectKind,
    object_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if kind != ObjectKind::SupplierPortalRequest {
        return Ok(());
    }
    let account = auth
        .load_account(actor_id, executor)
        .await?
        .filter(|account| account.is_active_backoffice())
        .ok_or_else(|| Error::Forbidden("供应商申请任务账号不是当前启用的内部账号".into()))?;
    let actor = AuditActor::new(account.id, account.login_account, account.kind);
    if auth.supplier_portal_request_readable(&actor, object_id, executor).await? {
        return Ok(());
    }
    Err(Error::Forbidden("当前账号不具备供应商申请读取范围".into()))
}

/// 重验当前内部账号对精确供应商申请的商品与供给业务确认资格。
///
/// # 参数
/// * `auth` - 已装配的工作流授权 Port
/// * `actor_id` - 当前内部确认人账号 ID
/// * `kind` - 固定任务关系中的对象种类
/// * `object_id` - 权威申请主键
/// * `executor` - 调用方执行器
///
/// # 返回
/// 供应商申请业务动作和责任范围全部满足时成功；其他对象沿原授权规则执行。
/// # 错误
/// 外部身份、资格撤销或越界拒绝；配置和基础设施错误原样传播。
pub(super) async fn require_supplier_portal_task_review(
    auth: &impl WorkflowAuthorizationPort,
    actor_id: &str,
    kind: ObjectKind,
    object_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if kind != ObjectKind::SupplierPortalRequest {
        return Ok(());
    }
    let account = auth
        .load_account(actor_id, executor)
        .await?
        .filter(|account| account.is_active_backoffice())
        .ok_or_else(|| Error::Forbidden("供应商申请确认人不是当前启用的内部账号".into()))?;
    let actor = AuditActor::new(account.id, account.login_account, account.kind);
    if auth.supplier_portal_request_reviewable(&actor, object_id, executor).await? {
        return Ok(());
    }
    Err(Error::Forbidden("当前账号不具备供应商申请所需的商品或供给业务确认资格".into()))
}

/// 隐藏不可见对象的存在性，保留配置、版本及基础设施错误供调用方处理。
///
/// # 参数
/// * `error` - 对象读取或参与校验返回的错误
///
/// # 返回
/// 读取拒绝统一为 Forbidden，其他错误原样返回，不得伪装成候选不合格。
///
/// # 错误
/// 不返回错误。
pub(super) fn task_read_error(error: Error) -> Error {
    match error {
        Error::Forbidden(_) | Error::NotFound(_) => {
            Error::Forbidden("当前账号不具备任务业务对象读取资格".into())
        },
        error => error,
    }
}

/// 按权威订单来源重验当前账号的业务对象读取范围。
///
/// # 参数
/// * `auth` - 已装配的工作流授权 Port。
/// * `actor_id` - 操作人或候选接收人，账号在本次执行器内重读。
/// * `kind` - 任务注册表固定的对象种类。
/// * `fact` - 同一执行器读取的业务对象事实。
/// * `executor` - 调用方查询或任务事务执行器。
/// # 返回
/// S2 订单来源通过独立详情授权时成功；其他对象仍执行其所属阶段的授权。
/// # 错误
/// 必需来源缺失、账号失效、未装配或详情范围越界时拒绝。
pub async fn require_order_task_read(
    auth: &impl WorkflowAuthorizationPort,
    actor_id: &str,
    kind: ObjectKind,
    fact: &ObjectFact,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !OrderTaskSource::required_for(kind) {
        return Ok(());
    }
    let source = fact
        .order_scope_source
        .as_ref()
        .filter(|source| source.matches_kind(kind))
        .ok_or_else(|| Error::Internal("订单关联任务缺少权威范围来源".into()))?;
    let account = auth
        .load_account(actor_id, executor)
        .await?
        .filter(|account| account.can_login)
        .ok_or_else(|| Error::Forbidden("任务账号不存在或已失效".into()))?;
    let actor = AuditActor::new(account.id, account.login_account, account.kind);
    auth.require_order_task_read(&actor, source, executor).await
}

/// 批量过滤订单与供应商申请对象；调用方须在排序、分页、计数之前应用结果。
///
/// # 参数
/// * `auth` - 注入的授权 Port。
/// * `actor_id` - 当前账号 ID。
/// * `facts` - 当前执行器读取的对象事实。
/// * `executor` - 原读取执行器。
/// # 返回
/// 保留订单及供应商申请仍可读的对象；其他阶段沿原有规则继续检查。
/// # 错误
/// 缺失或错配来源、账号失效及授权配置错误失败关闭。
pub async fn filter_order_facts(
    auth: &impl WorkflowAuthorizationPort,
    actor_id: &str,
    facts: &mut ObjectFactMap,
    executor: &mut dyn Executor,
) -> Result<()> {
    filter_supplier_portal_facts(auth, actor_id, facts, executor).await?;
    let sources = order_sources(facts)?;
    if sources.is_empty() {
        return Ok(());
    }
    let account = auth
        .load_account(actor_id, executor)
        .await?
        .filter(|account| account.can_login)
        .ok_or_else(|| Error::Forbidden("任务账号不存在或已失效".into()))?;
    let actor = AuditActor::new(account.id, account.login_account, account.kind);
    let allowed = auth.readable_order_sources(&actor, &sources, executor).await?;
    facts.retain(|(kind, _), fact| {
        !OrderTaskSource::required_for(*kind)
            || fact.order_scope_source.as_ref().is_some_and(|source| allowed.contains(source))
    });
    Ok(())
}

/// 在分页、统计与简报形成前移除当前账号无权读取的供应商申请。
async fn filter_supplier_portal_facts(
    auth: &impl WorkflowAuthorizationPort,
    actor_id: &str,
    facts: &mut ObjectFactMap,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut keys = facts
        .keys()
        .filter(|(kind, _)| *kind == ObjectKind::SupplierPortalRequest)
        .cloned()
        .collect::<Vec<_>>();
    keys.sort_by(|left, right| left.1.cmp(&right.1));
    for (kind, id) in keys {
        match require_supplier_portal_task_read(auth, actor_id, kind, &id, executor).await {
            Ok(()) => {},
            Err(Error::Forbidden(_) | Error::NotFound(_)) => {
                facts.remove(&(kind, id));
            },
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// 注册类型必须有精确来源；空集不能掩盖缺失的业务事实。
fn order_sources(facts: &ObjectFactMap) -> Result<BTreeSet<OrderTaskSource>> {
    facts
        .iter()
        .filter(|((kind, _), _)| OrderTaskSource::required_for(*kind))
        .map(|((kind, _), fact)| {
            fact.order_scope_source
                .as_ref()
                .filter(|source| source.matches_kind(*kind))
                .cloned()
                .ok_or_else(|| Error::Internal("订单关联任务缺少权威范围来源或来源类型不符".into()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::ports::FailClosedWorkflowAuthorizationPort;

    #[test]
    fn typed_order_sources_are_deduplicated_and_never_inferred_from_display_root() {
        let source = OrderTaskSource::Purchase("purchase".into());
        let fact = ObjectFact::new("display", "title", "creator").with_order_source(source.clone());
        let mut facts = ObjectFactMap::from([
            ((ObjectKind::PurchaseReceipt, "receipt".into()), fact.clone()),
            ((ObjectKind::ElectronicDelivery, "electronic".into()), fact),
        ]);
        assert_eq!(order_sources(&facts).unwrap(), BTreeSet::from([source]));
        facts.insert(
            (ObjectKind::Delivery, "sales-delivery".into()),
            ObjectFact::new("display", "title", "creator"),
        );
        assert!(order_sources(&facts).is_err());
        facts.get_mut(&(ObjectKind::Delivery, "sales-delivery".into())).unwrap().order_scope_source =
            Some(OrderTaskSource::Purchase("wrong-dimension".into()));
        assert!(order_sources(&facts).is_err());
    }

    #[tokio::test]
    async fn missing_composition_cannot_authorize_an_order_task() {
        let auth = FailClosedWorkflowAuthorizationPort;
        let actor = AuditActor::new("actor".into(), "account".into(), AccountKind::Admin);
        let source = OrderTaskSource::Sales("sales".into());
        assert!(auth.require_order_task_read(&actor, &source, &mut NoTransaction).await.is_err());
        assert!(
            auth.readable_order_sources(&actor, &BTreeSet::from([source]), &mut NoTransaction).await.is_err()
        );
    }

    #[tokio::test]
    async fn missing_composition_cannot_authorize_supplier_portal_request_scope() {
        let auth = FailClosedWorkflowAuthorizationPort;
        let actor = AuditActor::new("reviewer".into(), "account".into(), AccountKind::Admin);
        assert!(auth.supplier_portal_request_readable(&actor, "request", &mut NoTransaction).await.is_err());
    }
}
