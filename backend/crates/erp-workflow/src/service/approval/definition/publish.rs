use crate::repository::BpmExt;
use bpm::graph::DefinitionGraph;
use bpm::model::types::ApprovalCommandKind;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::error::Result;
use application_core::AuditActor;

use super::super::definition_dto::{DefinitionDetailView, PublishDefinitionRequest};
use super::super::policy::{
    ensure_actions_registered, validate_required_purposes, ProcessRequiredApprovalPolicy,
};
use super::command::{
    applied_definition, ensure_definition_admin_permission, lock_command_identity, map_model_error, now,
    parse_idempotency_key, participant, policy_for_definition, replay_prepared_definition_receipt,
    write_definition_audit, write_receipt, DefinitionCommandResultRef, DefinitionResultExpectation,
    PreparedDefinitionIdentity, PUBLISH_DEFINITION_COMMAND_DOMAIN,
};
use super::mapping::detail_view;
use super::replace::{
    apply_snapshots, load_assignee_snapshots, rebuild_draft_graph, reload_draft_for_cas, replace_graph,
    validate_assignees,
};
use super::ApprovalDefinitionService;

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalDefinitionService<A> {
    /// 发布草稿为当前唯一已发布版本。
    ///
    /// # 参数
    /// * `request` - 发布请求
    /// * `actor` - 已认证操作人
    ///
    /// # 错误
    /// 图、用途、账号或权限校验失败时零写入返回错误。
    pub async fn publish_definition(
        &self,
        mut request: PublishDefinitionRequest,
        actor: &AuditActor,
    ) -> Result<DefinitionDetailView> {
        let key = parse_idempotency_key(&request.idempotency_key)?;
        request.idempotency_key = key.as_str().to_string();
        let identity = lock_command_identity(
            key,
            ApprovalCommandKind::PublishDefinition,
            PUBLISH_DEFINITION_COMMAND_DOMAIN,
            &request.definition_id,
            request.expected_definition_lock_version,
            actor.id(),
        )?;
        let graph = self.require_graph(&request.definition_id).await?;
        let policy = policy_for_definition(&graph.definition)?;
        self.ensure_definition_admin(actor, &policy).await?;
        if let Some(view) = self
            .replay_if_receipt(
                &identity.current,
                DefinitionResultExpectation::DefinitionId(request.definition_id.clone()),
            )
            .await?
        {
            return Ok(view);
        }
        if let Some(view) = self.replay_legacy_if_receipt(&identity).await? {
            return Ok(view);
        }
        self.commit_publish(graph, policy, request, actor, identity).await
    }

    /// 在唯一事务中发布草稿并退役旧版本。
    ///
    /// # 错误
    /// 任一校验失败时整体回滚。
    async fn commit_publish(
        &self,
        graph: DefinitionGraph,
        policy: ProcessRequiredApprovalPolicy,
        request: PublishDefinitionRequest,
        actor: &AuditActor,
        identity: PreparedDefinitionIdentity,
    ) -> Result<DefinitionDetailView> {
        let db = self.db.clone();
        let rbac = self.auth.clone();
        let audit = std::sync::Arc::clone(&self.audit);
        let transaction_actor = actor.clone();
        let transaction_policy = policy.clone();
        let transaction_identity = identity.clone();
        let expectation = DefinitionResultExpectation::DefinitionId(request.definition_id.clone());
        let client = db.client().clone();
        let outcome = client.with_transaction(move |session| {
            Box::pin(async move {
                publish_tx(
                    &db,
                    &rbac,
                    graph,
                    PublishTxInput {
                        policy: transaction_policy,
                        request,
                        actor: &transaction_actor,
                        identity: &transaction_identity,
                        audit: audit.as_ref(),
                    },
                    session,
                )
                .await
            })
        });
        self.recover_definition_command(outcome.await, &policy, actor, identity, expectation)
            .await
    }
}

/// 发布草稿事务的政策、请求、命令身份与操作人。
///
/// # 用途
/// 将发布命令上下文字段打包。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 图、用途、账号或动作任一失败都不得进入发布写库。
struct PublishTxInput<'a> {
    /// 单据审批政策。
    policy: ProcessRequiredApprovalPolicy,
    /// 发布请求。
    request: PublishDefinitionRequest,
    /// 审计操作人。
    actor: &'a AuditActor,
    /// 已规范化的当前命令身份及精确旧格式候选。
    identity: &'a PreparedDefinitionIdentity,
    /// Injected audit port.
    audit: &'a dyn crate::ports::WorkflowAuditPort,
}

/// 事务内发布草稿。
///
/// # 用途
/// 回放收据或刷新快照并发布当前草稿。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `graph` - 当前草稿图
/// * `input` - 政策、请求、摘要与操作人
/// * `session` - 事务会话
///
/// # 返回
/// 返回发布后的定义详情。
///
/// # 错误
/// 陈旧锁、图校验失败或写入失败时返回错误。
///
/// # 关键业务约束
/// 任一校验失败必须整体回滚。
async fn publish_tx(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    graph: DefinitionGraph,
    input: PublishTxInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<DefinitionDetailView> {
    let PublishTxInput {
        policy,
        request,
        actor,
        identity,
        audit,
    } = input;
    ensure_definition_admin_permission(rbac, actor, &policy, session).await?;
    if let Some(view) = replay_prepared_definition_receipt(
        db,
        identity,
        &DefinitionResultExpectation::DefinitionId(request.definition_id.clone()),
        session,
    )
    .await?
    {
        return Ok(view);
    }
    let mut current = reload_draft_for_cas(
        db,
        &graph.definition,
        request.expected_definition_lock_version,
        session,
    )
    .await?;
    let PublishWriteStep::RefreshSnapshotsAndRetirePrevious = decide_publish_write(
        current.validate_linear().map_err(map_model_error),
        validate_required_purposes(&policy, &current.purpose_refs()),
        validate_assignees(db, rbac, &policy, &current.assignee_ids(), session).await,
        ensure_actions_registered(&policy),
    )?;
    current = prepare_publish_graph(db, rbac, current, session).await?;
    let previous = db
        .bpm_workflow()
        .find_published_by_process_kind(policy.process_kind, session)
        .await?;
    let previous_lock = previous.as_ref().map(|item| item.definition_lock_version());
    let (published, previous) = current
        .definition
        .clone()
        .publish_replacing(previous, participant(actor)?, now()?)
        .map_err(map_model_error)?;
    let mut result = DefinitionGraph {
        definition: published,
        nodes: current.nodes.clone(),
        transitions: current.transitions.clone(),
    };
    let result_ref = DefinitionCommandResultRef::from_graph(&result).encode();
    write_receipt(db, &identity.current, &result_ref, session).await?;
    let refreshed = replace_graph(db, current, request.expected_definition_lock_version, session).await?;
    let outcome = db
        .bpm_workflow()
        .publish_and_retire_previous(
            &result.definition,
            previous.as_ref(),
            refreshed.definition.definition_lock_version(),
            previous_lock,
            session,
        )
        .await?;
    result.definition = applied_definition(outcome)?;
    write_definition_audit(
        audit,
        actor,
        "approval_definition.publish",
        &result,
        Some(request.expected_definition_lock_version),
        None,
        session,
    )
    .await?;
    Ok(detail_view(&result))
}

/// 刷新快照并构造待写草稿图，供发布事务在 receipt-first 后持久化。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `graph` - 已完成发布前校验的草稿图
/// * `session` - 调用方事务执行器
///
/// # 返回
/// 返回尚未写库的完整草稿图。
///
/// # 错误
/// 账号快照缺失或 BPM 重建失败时返回错误。
///
/// # 关键业务约束
/// 发布前必须以当前账号显示名刷新全部节点快照；本函数不得产生物理写入。
async fn prepare_publish_graph(
    _db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    graph: DefinitionGraph,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let assignee_ids = graph.assignee_ids();
    let snapshots = load_assignee_snapshots(rbac, &assignee_ids, session).await?;
    let nodes = apply_snapshots(graph.nodes, &snapshots)?;
    rebuild_draft_graph(&graph.definition, nodes)
}

/// 发布写库步骤。仅在全部重验通过后允许刷新快照并退役旧版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishWriteStep {
    /// 允许准备快照并在 receipt-first 后替换图与发布定义。
    RefreshSnapshotsAndRetirePrevious,
}

/// 图、用途、账号或动作任一失败都不得进入发布写库。
///
/// # 错误
/// 返回第一个失败的校验错误。
fn decide_publish_write(
    graph: Result<()>,
    purposes: Result<()>,
    assignees: Result<()>,
    actions: Result<()>,
) -> Result<PublishWriteStep> {
    graph?;
    purposes?;
    assignees?;
    actions?;
    Ok(PublishWriteStep::RefreshSnapshotsAndRetirePrevious)
}

#[cfg(test)]
mod tests {
    use super::super::super::policy::SeparationOfDutiesPolicy;
    use super::super::replace::{ensure_static_decide_permission, validate_static_separation};
    use super::super::test_support::{production_source, source_fn};
    use super::*;
    use crate::error::Error;

    /// 发布重验岗位分离不把提交人隔离伪装成实例 DataScopeFact。
    #[test]
    fn publish_static_separation_does_not_forge_instance_data_scope() {
        validate_static_separation(
            SeparationOfDutiesPolicy::ForbidSubmitterAsApprover,
            &["u1".to_string(), "u1".to_string()],
        )
        .unwrap();
        assert!(production_source().contains("不伪造实例"));
        assert!(!production_source().contains("access_control::DataScopeFact"));
        let failed_assignees = Err(Error::ValidationError(
            "指定审批人账号不存在、已停用或任职失效".into(),
        ));
        assert!(decide_publish_write(Ok(()), Ok(()), failed_assignees, Ok(())).is_err());
        let publish_tx = source_fn(production_source(), "async fn publish_tx", "async fn retire_tx");
        let gate = publish_tx.find("decide_publish_write").expect("发布闸门");
        let prepared = publish_tx.find("prepare_publish_graph").expect("刷新规划");
        let receipt = publish_tx.find("write_receipt").expect("收据首写");
        let graph_write = publish_tx.find("replace_graph").expect("图写入");
        assert!(gate < prepared);
        assert!(prepared < receipt);
        assert!(receipt < graph_write);
    }

    /// 账号或静态权限重验失败时发布不得进入写库步骤。
    #[test]
    fn publish_revalidation_failure_blocks_write_step() {
        let no_decide = ensure_static_decide_permission(false).unwrap_err();
        assert!(matches!(
            no_decide,
            Error::BusinessLogicError(message) if message.contains("approval") || message.contains("静态审批")
        ));
        ensure_static_decide_permission(true).unwrap();
        assert!(decide_publish_write(
            Ok(()),
            Ok(()),
            Err(Error::ValidationError(
                "指定审批人账号不存在、已停用或任职失效".into()
            )),
            Ok(()),
        )
        .is_err());
        assert!(decide_publish_write(
            Ok(()),
            Ok(()),
            Err(Error::BusinessLogicError("指定审批人不具备静态审批权限".into())),
            Ok(()),
        )
        .is_err());
    }
}
