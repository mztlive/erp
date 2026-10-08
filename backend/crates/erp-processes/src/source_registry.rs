//! 拥有带审计外层事务的来源系统流程。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_support::{
    CreateSourceSystemRequest, SourceRegistryExt, SourceSystem, SourceSystemId, SourceSystemView,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::Result;
use crate::audit::run_audited;

/// 返回来源系统流程模块名。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回稳定模块名 `source_registry`。
///
/// # 错误
/// 不返回错误。
pub fn process_name() -> &'static str {
    "source_registry"
}

/// 创建来源系统，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - 来源系统所在数据库。
/// * `req` - 来源系统创建请求。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回创建后的来源系统视图。
///
/// # 错误
/// 请求校验失败、来源系统构造失败，或写入与审计事务失败时返回错误。
pub async fn create_source_system(
    db: Database,
    req: CreateSourceSystemRequest,
    actor: AuditActor,
) -> Result<SourceSystemView> {
    req.validate()?;
    let id = SourceSystemId::new(next_id());
    let system = SourceSystem::new(id, req.into_data(), actor.id())?;
    let audit =
        actor.clone().resource_log("source_system.create", "source_system", system.base.id.clone())?;
    let system_for_tx = system.clone();
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            db.source_systems().create(&system_for_tx, executor).await?;
            Ok(())
        })
    })
    .await?;
    Ok(system.into())
}
