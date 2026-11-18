use application_core::AuditActor;
use bpm::graph::DefinitionGraph;
use bpm::model::ApprovalProcessDefinition;
use bpm::model::types::ApprovalCommandKind;
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_permissions::approval_process;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::definition_dto::{DefinitionDetailView, PublishDefinitionRequest};
use super::super::policy::{
    ProcessRequiredApprovalPolicy, ensure_actions_registered, validate_required_purposes,
};
use super::ApprovalDefinitionService;
use super::command::{
    DefinitionCommandResultRef, DefinitionResultExpectation, PUBLISH_DEFINITION_COMMAND_DOMAIN,
    PreparedDefinitionIdentity, applied_definition, lock_command_identity, map_model_error, now,
    parse_idempotency_key, participant, policy_for_definition, replay_prepared_definition_receipt,
    write_definition_audit, write_receipt,
};
use super::mapping::detail_with_copies;
use super::replace::{
    apply_snapshots, ensure_draft_lock, rebuild_draft_graph, replace_graph, validate_assignees,
};
use crate::entity::approval_copy::ApprovalCopyRule;
use crate::error::Result;
use crate::repository::BpmExt;
use crate::repository::approval_copy::ApprovalCopyExt;
use crate::service::approval::copies::validate_recipients;

impl ApprovalDefinitionService {
    /// 按命令身份将草稿发布为该单据类型唯一当前版本；同载荷重放返回已发布详情。图结构、节点用途、审批账号或权限校验失败时不提交发布写入。
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
        self.commit_publish(request, actor, identity).await
    }

    // 在唯一事务中发布草稿并退役旧版本。
    // 任一校验失败时整体回滚。
    async fn commit_publish(
        &self,
        request: PublishDefinitionRequest,
        actor: &AuditActor,
        identity: PreparedDefinitionIdentity,
    ) -> Result<DefinitionDetailView> {
        let service = self.clone();
        let transaction_actor = actor.clone();
        let transaction_identity = identity.clone();
        let expectation = DefinitionResultExpectation::DefinitionId(request.definition_id.clone());
        let client = self.db.client().clone();
        let outcome = client.with_transaction(move |executor| {
            Box::pin(async move {
                publish_tx(
                    &service,
                    PublishTxInput { request, actor: &transaction_actor, identity: &transaction_identity },
                    executor,
                )
                .await
            })
        });

        self.recover_definition_command(
            outcome.await,
            &approval_process::PUBLISH_DEFINITION,
            actor,
            identity,
            expectation,
        )
        .await
    }
}

/// 发布草稿事务的请求、命令身份与操作人。
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
    /// 发布请求。
    request: PublishDefinitionRequest,
    /// 审计操作人。
    actor: &'a AuditActor,
    /// 已规范化的当前命令身份及精确旧格式候选。
    identity: &'a PreparedDefinitionIdentity,
}

// 事务内发布草稿。
// 任一校验失败必须整体回滚。
//
// 回放收据或刷新快照并发布当前草稿。
//
// 返回发布后的定义详情。
//
// 陈旧锁、图校验失败或写入失败时返回错误。
//
// `service`：同一应用注入的定义服务与基础设施。
async fn publish_tx(
    service: &ApprovalDefinitionService,
    input: PublishTxInput<'_>,
    executor: &mut dyn Executor,
) -> Result<DefinitionDetailView> {
    let PublishTxInput { request, actor, identity } = input;
    let graph = service
        .require_graph(&request.definition_id, actor, &approval_process::PUBLISH_DEFINITION, executor)
        .await?;

    if let Some(view) = replay_prepared_definition_receipt(
        &service.db,
        identity,
        &DefinitionResultExpectation::DefinitionId(request.definition_id.clone()),
        executor,
    )
    .await?
    {
        return Ok(view);
    }

    ensure_draft_lock(&graph.definition, request.expected_definition_lock_version)?;
    let policy = policy_for_definition(&graph.definition)?;
    let current =
        publish_candidate(&service.authorization, &service.identity, &service.db, graph, &policy, executor)
            .await?;
    apply_publication(service, current, request, actor, identity, executor).await
}

// 把新定义发布与旧发布版本退役放在同一事务内规划并保存，使当前发布版本唯一。
async fn apply_publication(
    service: &ApprovalDefinitionService,
    current: DefinitionGraph,
    request: PublishDefinitionRequest,
    actor: &AuditActor,
    identity: &PreparedDefinitionIdentity,
    executor: &mut dyn Executor,
) -> Result<DefinitionDetailView> {
    let db = &service.db;
    let Publication { mut result, previous, previous_lock } =
        plan_publication(db, &current, actor, executor).await?;

    write_receipt(db, &identity.current, &DefinitionCommandResultRef::from_graph(&result).encode(), executor)
        .await?;
    let refreshed = replace_graph(db, current, request.expected_definition_lock_version, executor).await?;
    let outcome = db
        .bpm_workflow()
        .publish_and_retire_previous(
            &result.definition,
            previous.as_ref(),
            refreshed.definition.definition_lock_version(),
            previous_lock,
            executor,
        )
        .await?;
    result.definition = applied_definition(outcome)?;

    write_definition_audit(
        service.audit.as_ref(),
        actor,
        "approval_definition.publish",
        &result,
        Some(request.expected_definition_lock_version),
        None,
        executor,
    )
    .await?;
    detail_with_copies(db, &result, executor).await
}

struct Publication {
    result: DefinitionGraph,
    previous: Option<ApprovalProcessDefinition>,
    previous_lock: Option<u64>,
}

// 在发布事务内先规划草稿发布及旧发布退役的实体结果；构造收据以前不落库。
async fn plan_publication(
    db: &Database,
    current: &DefinitionGraph,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Publication> {
    let previous =
        db.bpm_workflow().find_published_by_process_kind(current.definition.process_kind, executor).await?;
    let previous_lock = previous.as_ref().map(|item| item.definition_lock_version());
    let (published, previous) = current
        .definition
        .clone()
        .publish_replacing(previous, participant(actor)?, now()?)
        .map_err(map_model_error)?;
    Ok(Publication {
        result: DefinitionGraph {
            definition: published,
            nodes: current.nodes.clone(),
            transitions: current.transitions.clone(),
        },
        previous,
        previous_lock,
    })
}

async fn publish_candidate(
    authorization: &AuthorizationService,
    identity_service: &SharedRbacService,
    db: &Database,
    current: DefinitionGraph,
    policy: &ProcessRequiredApprovalPolicy,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    current.validate_linear().map_err(map_model_error)?;
    validate_required_purposes(policy, &current.purpose_refs())?;
    let snapshots =
        validate_assignees(authorization, db, identity_service, &current.assignee_ids(), executor).await?;
    ensure_actions_registered(policy)?;

    if let Some(config) = db.approval_copies().config(&current.definition.base.id, executor).await? {
        let rules = ApprovalCopyRule::prepare(config.rules, current.nodes.len())?;
        validate_recipients(identity_service, &rules, executor).await?;
    }

    let nodes = apply_snapshots(current.nodes, &snapshots)?;
    rebuild_draft_graph(&current.definition, nodes)
}

#[cfg(test)]
mod tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::super::test_support::unavailable_service;
    use super::*;
    use crate::error::Error;

    #[tokio::test]
    async fn authorization_failure_precedes_definition_facts_receipts_and_lock_checks() {
        let (service, actor) = unavailable_service().await;
        let request = PublishDefinitionRequest {
            definition_id: "definition-never-read".into(),
            expected_definition_lock_version: 99,
            idempotency_key: "publish-retry".into(),
        };
        let identity = lock_command_identity(
            parse_idempotency_key(&request.idempotency_key).unwrap(),
            ApprovalCommandKind::PublishDefinition,
            PUBLISH_DEFINITION_COMMAND_DOMAIN,
            &request.definition_id,
            request.expected_definition_lock_version,
            actor.id(),
        )
        .unwrap();

        let error = publish_tx(
            &service,
            PublishTxInput { request, actor: &actor, identity: &identity },
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
