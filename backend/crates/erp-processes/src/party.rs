//! 拥有带审计外层事务的主体流程。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::ids::PartyId;
use erp_party::PartyExt;
use mongodb::Database;

use crate::Result;
use crate::adapters::{MongoSupplierRole, party_service};
use crate::audit::run_audited;

/// 返回主体流程模块名。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回稳定模块名 `party`。
///
/// # 错误
/// 不返回错误。
pub fn process_name() -> &'static str {
    "party"
}

/// 软删除主体，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - 主体所在数据库。
/// * `id` - 主体主键。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 软删除和审计写入成功后返回。
///
/// # 错误
/// 主体不存在、供应商角色检查失败、主体已挂供应商角色、审计构造失败，或删除与审计事务失败时返回错误。
pub async fn delete_party(db: Database, id: String, actor: AuditActor) -> Result<()> {
    let mut party = party_service(db.clone()).load_party(&id).await?;
    erp_party::ensure_outside_supplier_profile(&*MongoSupplierRole::shared(db.clone()), &PartyId::new(&id))
        .await?;
    let audit = actor.clone().resource_log("party.delete", "party", party.base.id.clone())?;
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            db.parties().soft_delete(&mut party, executor).await?;
            Ok(())
        })
    })
    .await?;
    Ok(())
}
