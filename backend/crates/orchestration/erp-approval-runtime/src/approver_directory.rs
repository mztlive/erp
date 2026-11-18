//! 将身份域组织负责人及事务内账号资格接入工作流。
use std::sync::Arc;

use application_core::AuditActor;
use bpm::graph::DefinitionGraph;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::{ApprovalCommandReceipt, AssigneeResolution, ParticipantId, Timestamp};
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_identity::AuthorizationService;
use erp_identity::entity::organization::OrgUnitKind;
use erp_identity::service::organization::leader::applicant_leader;
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
                return Err(WorkflowError::ValidationError(
                    "该审批步骤已指定人员，不能改由组织负责人审批".into(),
                ));
            },
        };

        let unit = applicant_leader(self.db, applicant, kind, at, ex)
            .await
            .map_err(|e| map_service(Error::from(e)))?;
        let leader = unit.leader.ok_or_else(|| {
            WorkflowError::ValidationError("该组织尚未指定负责人，请联系管理员设置审批负责人".into())
        })?;

        let actor = AuditActor::new(leader.user_id.clone(), leader.user_id, AccountKind::Admin);
        let required = [approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION];
        let qualified = self
            .identity
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

        let account = qualified.account();
        let participant_id = ParticipantId::new(account.id.clone())
            .map_err(|e| WorkflowError::ValidationError(e.to_string()))?;
        ResolvedApprover::new(
            participant_id,
            AssigneeResolution {
                rule: source
                    .rule()
                    .ok_or_else(|| {
                        WorkflowError::ValidationError("找不到审批关联的业务单据，请联系管理员核对".into())
                    })?
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

/// 按申请人和当前组织事实解析定义图中的动态审批人，供新提交生成正式指派。只修改内存副本并沿用原执行器，任一动态节点无法解析时拒绝提交。
pub async fn resolve_approvers(
    db: &Database,
    identity: &Arc<AuthorizationService>,
    graph: DefinitionGraph,
    applicant: &str,
    ex: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    Ok(resolve_graph(&OrganizationApprovers { db, identity }, graph, applicant, Instant::now(), ex).await?)
}

/// 写入审批实例前，在原事务核对计划中的动态指派是否仍符合当前组织来源和权限，避免负责人变更或撤权后仍创建任务。复验或读取失败时中止提交。
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

/// 新命令才解析当前组织负责人；有启动回执时读取原实例的人员和定义图，避免组织调整改变历史分派。
/// 原实例或审批图缺失、申请人或绑定定义版本不符时拒绝；命令摘要由后续统一启动入口核验。
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
        .ok_or_else(|| Error::ConflictError("找不到原审批提交结果，请先刷新单据确认审批状态".into()))?;
    if instance.started_by.as_str() != applicant
        || instance.process_definition_id.as_ref() != graph.definition.base.id
        || instance.definition_version != graph.definition.definition_version
    {
        return Err(Error::ConflictError("原审批结果与本次提交不符，请核对单据的审批记录".into()));
    }

    db.bpm_workflow()
        .load_instance_graph(&instance, ex)
        .await?
        .ok_or_else(|| Error::ConflictError("原审批路线缺失，请联系管理员".into()))
}
