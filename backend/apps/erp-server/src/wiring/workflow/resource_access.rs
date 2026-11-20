//! 固定任务及审批入口将完整功能资格与真实来源交给同一次 identity 授权。
use application_core::AuditActor;
use erp_identity::entity::resource_access::GrantRule;
use erp_workflow::ErrorCode;
use erp_workflow::ports::OrderTaskSource;
use mongodb::Database;
use persistence_core::Executor;

use crate::wiring::authorization::resource_grants::{GrantReadiness, source_readiness};
use crate::wiring::errors::{Error, Result};

// 实际办理授权被拒绝后，只检查当前精确来源的分配事件是否仍待应用。
// db 为共享数据库，actor、source与instance来自本次真实任务，executor为原事务。
// 当前没有待应用分配时成功，不产生或替代对象授权结果。
// 合法分配尚未应用时返回授权未就绪；取证失败保持原错误。
pub(super) async fn approval_readiness(
    db: &Database,
    actor: &AuditActor,
    source: &OrderTaskSource,
    instance_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let rule = match source {
        OrderTaskSource::Sales(_) => GrantRule::SalesOrderApprovalAssigned,
        OrderTaskSource::Purchase(_) => GrantRule::PurchaseOrderApprovalAssigned,
    };
    let key = rule.source_key(instance_id, actor.id());
    if source_readiness(db, &key, executor).await? == GrantReadiness::NotReady {
        return Err(Error::Coded(ErrorCode::ResourceAuthorizationNotReady));
    }
    Ok(())
}
