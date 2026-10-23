//! 将身份域组织负责人及事务内账号资格接入工作流。
use bpm::graph::DefinitionGraph;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::{ApprovalCommandReceipt, AssigneeResolution, ParticipantId, Timestamp};
use erp_core::common::time::Instant;
use erp_identity::entity::organization::OrgUnitKind;
use erp_identity::service::organization::leader::applicant_leader;
use erp_identity::service::organization::qualification::qualified_account;
use erp_workflow::ports::approver_directory::{ApproverDirectory, ResolvedApprover};
use erp_workflow::service::approval::assignee_source::AssigneeSource;
use erp_workflow::service::approval::dynamic_assignees::{resolve_graph, revalidate_assignments};
use erp_workflow::service::approval::execution::PlannedWrites;
use erp_workflow::{BpmExt, Error as WorkflowError, Result as WorkflowResult};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;
use crate::{Error, Result};

/// 只借用数据库与调用方事务，不读取其他业务领域。
pub struct OrganizationApprovers<'a>(pub &'a Database);
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
        let unit = applicant_leader(self.0, applicant, kind, at, ex)
            .await
            .map_err(|e| map_service(Error::from(e)))?;
        let leader = unit.leader.ok_or_else(|| WorkflowError::ValidationError("组织未配置负责人".into()))?;
        let account = qualified_account(
            self.0,
            &leader.user_id,
            &["approval_instance:read", "approval_instance:decide"],
            ex,
        )
        .await
        .map_err(|e| map_service(Error::from(e)))?;
        let participant_id =
            ParticipantId::new(account.base.id).map_err(|e| WorkflowError::ValidationError(e.to_string()))?;
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
                participant_name: account.name,
                resolved_at: Timestamp::from_unix_secs(at.unix_secs())
                    .map_err(|e| WorkflowError::ValidationError(e.to_string()))?,
            },
        )
    }
}

/// 为本次提交构造动态人员图；只解析内存副本。
/// # 参数
/// 数据库、定义图、本次申请人及原执行器。
/// # 返回
/// 已解析的图。
/// # 错误
/// 任一动态节点不可用时拒绝。
pub async fn resolve_approvers(
    db: &Database,
    graph: DefinitionGraph,
    applicant: &str,
    ex: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    Ok(resolve_graph(&OrganizationApprovers(db), graph, applicant, Instant::now(), ex).await?)
}

/// 写入运行实例前在原事务中重验来源与资格。
/// # 参数
/// 数据库、待写计划及原事务。
/// # 返回
/// 全部动态指派有效时成功。
/// # 错误
/// 负责人变化、权限撤销或查询错误时拒绝整次提交。
pub async fn revalidate_approvers(
    db: &Database,
    writes: &PlannedWrites,
    ex: &mut dyn Executor,
) -> Result<()> {
    Ok(revalidate_assignments(
        &OrganizationApprovers(db),
        &writes.instance,
        &writes.created_assignees,
        Instant::now(),
        ex,
    )
    .await?)
}

/// 已提交请求只恢复原实例人员；新请求才解析当前组织负责人。
/// # 参数
/// 数据库、定义图、申请人、已找到的启动收据和调用方执行器。
/// # 返回
/// 回放使用原实例图，新命令使用当前来源解析结果。
/// # 错误
/// 收据实例或申请人不匹配时拒绝；命令摘要仍由后续统一启动入口核验。
pub async fn resolve_start_approvers(
    db: &Database,
    graph: DefinitionGraph,
    applicant: &str,
    receipt: Option<&ApprovalCommandReceipt>,
    ex: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let Some(receipt) = receipt else { return resolve_approvers(db, graph, applicant, ex).await };
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
