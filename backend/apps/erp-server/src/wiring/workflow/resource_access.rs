//! 固定任务及审批入口将完整功能资格与真实来源交给同一次 identity 授权。
use application_core::AuditActor;
use erp_identity::entity::resource_access::SourceKey;
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
    let permission = Permission::parse(entry)?;
    let required = if permission.resource() == "approval_instance" && permission.action() == "decide" {
        vec![Permission::parse("approval_instance:read")?]
    } else {
        Vec::new()
    };
    let result = require_source_operation(db, rbac, actor, source, &permission, &required, executor).await;
    match result {
        Ok(()) => Ok(true),
        Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
        Err(error) => Err(error),
    }
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
