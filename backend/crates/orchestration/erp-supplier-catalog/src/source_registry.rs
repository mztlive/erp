//! 拥有带审计外层事务的来源系统流程。
use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::run_audited;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::source_registry;
use erp_support::{
    CreateSourceSystemRequest, SourceRegistryExt, SourceSystem, SourceSystemId, SourceSystemView,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::{Error, Result};

/// 返回来源系统流程模块名。
///
/// 返回稳定模块名 `source_registry`。
pub fn process_name() -> &'static str {
    "source_registry"
}

/// 创建来源系统，并在同一事务中写入成功审计。
///
/// `db`：来源系统所在数据库；`req`：来源系统创建请求。
///
/// 返回创建后的来源系统视图。
///
/// 请求校验失败、来源系统构造失败，或写入与审计事务失败时返回错误。
pub async fn create_source_system(
    db: Database,
    authorization: Arc<AuthorizationService>,
    req: CreateSourceSystemRequest,
    actor: AuditActor,
) -> Result<SourceSystemView> {
    req.validate()?;
    let id = SourceSystemId::new(next_id());
    let system = SourceSystem::new(id, req.into_data(), actor.id())?;
    let audit =
        actor.clone().resource_log("source_system.create", "source_system", system.base.id.clone())?;

    let system_for_tx = system.clone();
    run_audited::<_, Error, _>(&db, audit, move |db, executor| {
        Box::pin(async move {
            authorization
                .require(&actor, &source_registry::SOURCE_SYSTEM_CREATE, AccessTarget::Operation, executor)
                .await?;
            db.source_systems().create(&system_for_tx, executor).await?;
            Ok(())
        })
    })
    .await?;
    Ok(system.into())
}
