//! 拥有带审计外层事务的主体流程。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::run_audited;
use erp_party::PartyService;
use mongodb::Database;

use crate::{Error, Result};

/// 软删除主体，并在同一事务中写入成功审计。
///
/// `db`：主体所在数据库；`service`：应用装配的主体领域服务；`id`：主体主键。
///
/// 软删除和审计写入成功后返回。
///
/// 主体不存在、供应商角色检查失败、主体已挂供应商角色、审计构造失败，或删除与审计事务失败时返回错误。
pub async fn delete_party(db: Database, service: PartyService, id: String, actor: AuditActor) -> Result<()> {
    let audit = actor.clone().resource_log("party.delete", "party", id.clone())?;
    run_audited(&db, audit, move |_db, executor| {
        Box::pin(async move {
            service.delete_party(&id, &actor, executor).await?;
            Ok::<(), Error>(())
        })
    })
    .await
}
