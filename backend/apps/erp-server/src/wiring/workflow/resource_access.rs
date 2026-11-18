//! 固定任务及审批入口将完整功能资格与真实来源交给同一次 identity 授权。
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::AuthorizationService;
use erp_identity::entity::resource_access::SourceKey;
use erp_resource_grants::{GrantReadiness, ResourceGrantProcess};
use erp_workflow::ErrorCode;
use erp_workflow::ports::OrderTaskSource;
use mongodb::Database;
use persistence_core::Executor;

use crate::wiring::errors::{Error, Result};

// 实际办理授权被拒绝后，只检查当前精确来源的分配事件是否仍待应用。
// db、authorization为共享设施，actor、source与instance来自本次真实任务，executor为原事务。
// 当前没有待应用分配时成功，不产生或替代对象授权结果。
// 合法分配尚未应用时返回授权未就绪；取证失败保持原错误。
pub(super) async fn approval_readiness(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    actor: &AuditActor,
    source: &OrderTaskSource,
    instance_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
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
    if ResourceGrantProcess::new(db.clone(), authorization.clone()).source_readiness(&key, executor).await?
        == GrantReadiness::NotReady
    {
        return Err(Error::Coded(ErrorCode::ResourceAuthorizationNotReady));
    }
    Ok(())
}
