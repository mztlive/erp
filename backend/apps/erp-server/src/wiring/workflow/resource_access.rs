//! 固定任务及审批入口将完整功能资格与真实来源交给同一次 identity 授权。
use application_core::AuditActor;
use erp_identity::entity::resource_access::{ResourceKind, SourceKey};
use erp_identity::{Permission, SharedRbacService};
use erp_read_models::sales_center::access::SalesAccess;
use erp_resource_grants::{GrantReadiness, ResourceGrantProcess};
use erp_workflow::ErrorCode;
use erp_workflow::ports::OrderTaskSource;
use mongodb::Database;
use persistence_core::Executor;

use crate::wiring::errors::{Error, Result};
use crate::wiring::purchase_access;

/// 当前入口与精确业务来源必须同时成立，读取授权不包含任务分配资格。
pub(super) async fn readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    source: &OrderTaskSource,
    entry: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let (kind, id) = match source {
        OrderTaskSource::Sales(id) => (ResourceKind::SalesOrder, id),
        OrderTaskSource::Purchase(id) => (ResourceKind::PurchaseOrder, id),
    };
    Ok(readable_batch(db, rbac, actor, kind, std::slice::from_ref(id), entry, executor).await?.contains(id))
}

/// 对每个精确来源执行统一对象检查，结果仅用于当前读取批次。
/// # 参数
/// db、rbac、actor 为认证设施；kind、ids 为有界目标；entry 为真实入口；executor 为原事务。
/// # 返回
/// 本批有读取权的目标 ID。
/// # 错误
/// 身份、功能、来源或数据库失败时拒绝。
pub(super) async fn readable_batch(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    kind: ResourceKind,
    ids: &[String],
    entry: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let permission = Permission::parse(entry)?;
    let mut readable = Vec::with_capacity(ids.len());
    for id in ids {
        let required = if permission.resource() == "approval_instance" && permission.action() == "decide" {
            vec![Permission::parse("approval_instance:read")?]
        } else {
            Vec::new()
        };
        let source = match kind {
            ResourceKind::SalesOrder => OrderTaskSource::Sales(id.clone()),
            ResourceKind::PurchaseOrder => OrderTaskSource::Purchase(id.clone()),
        };
        let result =
            require_source_operation(db, rbac, actor, &source, &permission, &required, executor).await;
        match result {
            Ok(()) => readable.push(id.clone()),
            Err(Error::Forbidden(_) | Error::NotFound(_)) => {},
            Err(error) => return Err(error),
        }
    }
    Ok(readable)
}

/// 实际办理审批时检查当前来源；合法分配事件未应用时只返回刷新状态。
pub(super) async fn require_approval(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    source: &OrderTaskSource,
    instance_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if readable(db, rbac, actor, source, "approval_instance:decide", executor).await? {
        return Ok(());
    }
    let key = SourceKey {
        rule: match source {
            OrderTaskSource::Sales(_) => "sales_order_approval_assigned",
            OrderTaskSource::Purchase(_) => "purchase_order_approval_assigned",
        }
        .into(),
        business_type: "approval_instance".into(),
        business_id: instance_id.into(),
        recipient: actor.id().into(),
    };
    if ResourceGrantProcess::new(db.clone(), rbac.clone()).source_readiness(&key, executor).await?
        == GrantReadiness::NotReady
    {
        return Err(Error::Coded(ErrorCode::ResourceAuthorizationNotReady));
    }
    Err(Error::Forbidden("没有审批单据的读取资格".into()))
}

/// 把完整管理功能资格和真实业务来源交给同一次 identity 检查。
/// # 参数
/// source为服务端强外键来源，entry/required为固定管理操作，executor为原事务。
/// # 返回
/// 完整资格与来源同时成立时成功。
/// # 错误
/// identity拒绝或事实取证失败原样传播。
pub(super) async fn require_source_operation(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    source: &OrderTaskSource,
    entry: &Permission,
    required: &[Permission],
    executor: &mut dyn Executor,
) -> Result<()> {
    match source {
        OrderTaskSource::Sales(id) => {
            SalesAccess::new(db.clone(), rbac.clone())
                .require_source_permissions(actor, entry, std::slice::from_ref(id), required, executor)
                .await?
        },
        OrderTaskSource::Purchase(id) => {
            purchase_access(db.clone(), rbac.clone())
                .require_source_permissions(actor, entry, std::slice::from_ref(id), required, executor)
                .await?
        },
    }
    Ok(())
}
