use application_core::AuditActor;
use bpm::model::types::ModelError;
use erp_core::AccountKind;
use erp_identity::{AuthorizationService, SharedRbacService};
use mongodb::Database;
use persistence_core::Executor;

use super::super::business_adapter::{BindingRevalidationContext, ensure_separation_of_duties};
use super::super::policy::ProcessRequiredApprovalPolicy;
use super::super::scope::approver_qualification;
use super::process_not_configured;
use super::upgrade::map_model_error;
use crate::entity::approval_qualification::ApprovalQualification;
use crate::error::{Error, Result};
use crate::repository::bpm::DefinitionGraph;

// 绑定或升级前，将一次读取的图交给 BPM 验证发布状态、入口、节点顺序和连线；仓储过滤不能替代图验证。
// 未发布报告未配置，结构损坏报告校验错误；持久化执行器不传入 BPM。
pub(super) fn revalidate_published_graph(graph: &DefinitionGraph) -> Result<()> {
    graph.validate_published_linear().map_err(|error| match error {
        ModelError::InvalidStatus(_) => process_not_configured(),
        other => map_model_error(other),
    })
}

/// 绑定或升级时，从 BPM 已校验图提取全部指定审批人，沿调用方事务复核岗位分离、有效账号、审批读写权限和订单必需事实。任一条件不满足即拒绝绑定。
pub async fn revalidate_binding_graph(
    authorization: &AuthorizationService,
    _db: &Database,
    _identity_service: &SharedRbacService,
    policy: &ProcessRequiredApprovalPolicy,
    context: &BindingRevalidationContext,
    graph: &DefinitionGraph,
    executor: &mut dyn Executor,
) -> Result<()> {
    let assignee_ids = graph.assignee_ids();
    ensure_separation_of_duties(policy.separation_of_duties_policy, &context.creator_id, &assignee_ids)?;

    for user_id in &assignee_ids {
        let actor = AuditActor::new(user_id.clone(), user_id.clone(), AccountKind::Admin);
        match approver_qualification(authorization, &actor, executor).await? {
            ApprovalQualification::Inactive => {
                return Err(Error::ValidationError("指定审批人账号不存在、已停用或任职失效".into()));
            },
            qualification => {
                context.ensure_order_facts(policy.document_type)?;
                if matches!(qualification, ApprovalQualification::MissingPermissions) {
                    return Err(Error::ValidationError("指定审批人缺少审批读取或决定权限".into()));
                }
            },
        }
    }
    Ok(())
}
