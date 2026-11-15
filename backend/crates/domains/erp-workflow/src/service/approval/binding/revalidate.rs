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

/// 复用 BPM 图原语重验发布结构；不得把 Executor 传入 BPM。
///
/// # 参数
/// * `graph` - Repository 一次性加载的发布定义图
///
/// # 返回
/// 状态为已发布且线性图完整时返回 `Ok(())`。
///
/// # 错误
/// 状态不是已发布时映射为未配置；图结构损坏时返回稳定校验错误。
///
/// # 关键业务约束
/// Service 不得复制节点顺序、入口或连线算法；仓储过滤不能替代 BPM 确认。
pub(super) fn revalidate_published_graph(graph: &DefinitionGraph) -> Result<()> {
    graph.validate_published_linear().map_err(|error| match error {
        ModelError::InvalidStatus(_) => process_not_configured(),
        other => map_model_error(other),
    })
}

/// 重验指定审批人的账号、审批权限、订单必要事实与岗位分离。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `policy` - 当前单据类型必须审批政策
/// * `context` - 当前单据组织与创建人事实
/// * `graph` - 已由 BPM 校验的定义图
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 全部定义审批人资格和绑定事实有效时返回 `Ok(())`。
///
/// # 错误
/// 岗位分离、账号、审批权限或订单必要事实校验失败时返回错误。
///
/// # 关键业务约束
/// 审批人集合由 BPM 图确定性提取，Service 只编排外部资格判断。
pub async fn revalidate_binding_graph(
    authorization: &AuthorizationService,
    db: &Database,
    identity_service: &SharedRbacService,
    policy: &ProcessRequiredApprovalPolicy,
    context: &BindingRevalidationContext,
    graph: &DefinitionGraph,
    executor: &mut dyn Executor,
) -> Result<()> {
    let assignee_ids = graph.assignee_ids();
    ensure_separation_of_duties(policy.separation_of_duties_policy, &context.creator_id, &assignee_ids)?;
    for user_id in &assignee_ids {
        let actor = AuditActor::new(user_id.clone(), user_id.clone(), AccountKind::Admin);
        match approver_qualification(authorization, db, identity_service, &actor, executor).await? {
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
