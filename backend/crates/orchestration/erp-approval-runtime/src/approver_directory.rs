//! 将身份域组织负责人及事务内账号资格接入工作流。
use std::sync::Arc;

use application_core::AuditActor;
use bpm::graph::DefinitionGraph;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::{ApprovalCommandReceipt, AssigneeResolution, ParticipantId, Timestamp};
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_identity::entity::organization::OrgUnitKind;
use erp_identity::service::organization::leader::applicant_leader;
use erp_identity::{AccountSummary, AuthorizationService};
use erp_permissions::approval_instance;
use erp_workflow::ports::approver_directory::{ApproverDirectory, ResolvedApprover};
use erp_workflow::service::approval::assignee_source::AssigneeSource;
use erp_workflow::service::approval::dynamic_assignees::{resolve_graph, revalidate_assignments};
use erp_workflow::service::approval::execution::PlannedWrites;
use erp_workflow::{BpmExt, Error as WorkflowError, Result as WorkflowResult};
use mongodb::Database;
use persistence_core::Executor;

use crate::errors::map_workflow_error as map_service;
use crate::{Error, Result};

/// 只借用数据库与调用方事务，不读取其他业务领域。
struct OrganizationApprovers<'a> {
    db: &'a Database,
    identity: &'a Arc<AuthorizationService>,
}
impl OrganizationApprovers<'_> {
    /// 通过统一入口核验动态审批人的有效内部账号及同角色完整资格。
    async fn account(&self, user: &str, ex: &mut dyn Executor) -> WorkflowResult<AccountSummary> {
        let actor = AuditActor::new(user.into(), user.into(), AccountKind::Admin);
        let required = [approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION];
        let authorization = self.identity.clone();
        let qualified = authorization
            .permissions(&actor, &required, ex)
            .await
            .map_err(|e| map_service(Error::from(e)))?
            .ok_or_else(|| {
                WorkflowError::ValidationError("审批负责人账号不存在或已停用，请调整负责人".into())
            })?;
        if !qualified.same_role(&required) {
            return Err(WorkflowError::ValidationError(
                "审批负责人缺少审批读取或决定权限，请联系管理员配置角色".into(),
            ));
        }
        Ok(qualified.account().clone())
    }
}
impl ApproverDirectory for OrganizationApprovers<'_> {
    async fn resolve(
        &self,
        applicant: &str,
        source: AssigneeSource,
        at: Instant,
        ex: &mut dyn Executor,
    ) -> WorkflowResult<ResolvedApprover> {
        let kind = match source {
            AssigneeSource::ApplicantTeamLeader => OrgUnitKind::Team,
            AssigneeSource::ApplicantDepartmentLeader => OrgUnitKind::Department,
            AssigneeSource::Specified => {
                return Err(WorkflowError::ValidationError("指定人员不使用组织负责人解析".into()));
            },
        };
        let unit = applicant_leader(self.db, applicant, kind, at, ex)
            .await
            .map_err(|e| map_service(Error::from(e)))?;
        let leader = unit.leader.ok_or_else(|| WorkflowError::ValidationError("组织未配置负责人".into()))?;
        let account = self.account(&leader.user_id, ex).await?;
        let participant_id = ParticipantId::new(account.id.clone())
            .map_err(|e| WorkflowError::ValidationError(e.to_string()))?;
        ResolvedApprover::new(
            participant_id,
            AssigneeResolution {
                rule: source
                    .rule()
                    .ok_or_else(|| WorkflowError::ValidationError("审批来源缺失".into()))?
                    .into(),
                subject_id: applicant.into(),
                source_id: unit.base.id,
                source_name: unit.name,
                assignment_id: leader.assignment_id,
                participant_name: account.display_name.clone(),
                resolved_at: Timestamp::from_unix_secs(at.unix_secs())
                    .map_err(|e| WorkflowError::ValidationError(e.to_string()))?,
            },
        )
    }
}

/// 为本次提交构造动态人员图；只解析内存副本。
/// # 参数
/// 数据库、共享身份服务、定义图、本次申请人及原执行器。
/// # 返回
/// 已解析的图。
/// # 错误
/// 任一动态节点不可用时拒绝。
pub async fn resolve_approvers(
    db: &Database,
    identity: &Arc<AuthorizationService>,
    graph: DefinitionGraph,
    applicant: &str,
    ex: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    Ok(resolve_graph(&OrganizationApprovers { db, identity }, graph, applicant, Instant::now(), ex).await?)
}

/// 写入运行实例前在原事务中重验来源与资格。
/// # 参数
/// 数据库、共享身份服务、待写计划及原事务。
/// # 返回
/// 全部动态指派有效时成功。
/// # 错误
/// 负责人变化、权限撤销或查询错误时拒绝整次提交。
pub async fn revalidate_approvers(
    db: &Database,
    identity: &Arc<AuthorizationService>,
    writes: &PlannedWrites,
    ex: &mut dyn Executor,
) -> Result<()> {
    Ok(revalidate_assignments(
        &OrganizationApprovers { db, identity },
        &writes.instance,
        &writes.created_assignees,
        Instant::now(),
        ex,
    )
    .await?)
}

/// 已提交请求只恢复原实例人员；新请求才解析当前组织负责人。
/// # 参数
/// 数据库、共享身份服务、定义图、申请人、已找到的启动收据和调用方执行器。
/// # 返回
/// 回放使用原实例图，新命令使用当前来源解析结果。
/// # 错误
/// 收据实例或申请人不匹配时拒绝；命令摘要仍由后续统一启动入口核验。
pub async fn resolve_start_approvers(
    db: &Database,
    identity: &Arc<AuthorizationService>,
    graph: DefinitionGraph,
    applicant: &str,
    receipt: Option<&ApprovalCommandReceipt>,
    ex: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let Some(receipt) = receipt else { return resolve_approvers(db, identity, graph, applicant, ex).await };
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&receipt.result_ref), ex)
        .await?
        .ok_or_else(|| Error::ConflictError("审批提交结果缺失，请刷新后重试".into()))?;
    if instance.started_by.as_str() != applicant
        || instance.process_definition_id.as_ref() != graph.definition.base.id
        || instance.definition_version != graph.definition.definition_version
    {
        return Err(Error::ConflictError("审批提交结果与本次请求不匹配".into()));
    }
    db.bpm_workflow()
        .load_instance_graph(&instance, ex)
        .await?
        .ok_or_else(|| Error::ConflictError("原审批路线缺失，请联系管理员".into()))
}
