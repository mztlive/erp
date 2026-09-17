use application_core::AuditActor;
use bpm::graph::DefinitionGraph;
use bpm::model::ApprovalProcessDefinition;
use bpm::model::types::ApprovalCommandKind;
use mongodb::Database;
use persistence_core::Transactional;

use super::super::definition_dto::{DefinitionDetailView, RetireDefinitionRequest};
use super::super::policy::ProcessRequiredApprovalPolicy;
use super::ApprovalDefinitionService;
use super::command::{
    DefinitionCommandResultRef, DefinitionResultExpectation, PreparedDefinitionIdentity,
    RETIRE_DEFINITION_COMMAND_DOMAIN, ensure_definition_admin_permission, ensure_lock, lock_command_identity,
    map_model_error, now, parse_idempotency_key, participant, policy_for_definition,
    replay_prepared_definition_receipt, write_definition_audit, write_receipt,
};
use super::mapping::detail_view;
use crate::error::{Error, Result};
use crate::repository::BpmExt;

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalDefinitionService<A> {
    /// 退役当前已发布定义。
    ///
    /// # 参数
    /// * `request` - 退役请求
    /// * `actor` - 已认证操作人
    ///
    /// # 错误
    /// 目标不是当前发布版本或锁冲突时返回错误。
    pub async fn retire_definition(
        &self,
        mut request: RetireDefinitionRequest,
        actor: &AuditActor,
    ) -> Result<DefinitionDetailView> {
        let key = parse_idempotency_key(&request.idempotency_key)?;
        request.idempotency_key = key.as_str().to_string();
        let identity = lock_command_identity(
            key,
            ApprovalCommandKind::RetireDefinition,
            RETIRE_DEFINITION_COMMAND_DOMAIN,
            &request.definition_id,
            request.expected_definition_lock_version,
            actor.id(),
        )?;
        let graph = self.require_graph(&request.definition_id).await?;
        let policy = policy_for_definition(&graph.definition)?;
        self.ensure_definition_admin(actor, &policy).await?;
        if let Some(view) = self
            .replay_prepared_if_receipt(
                &identity,
                DefinitionResultExpectation::DefinitionId(request.definition_id.clone()),
            )
            .await?
        {
            return Ok(view);
        }
        self.commit_retire(graph, policy, request, actor, identity).await
    }

    /// 在唯一事务中退役当前发布版本。
    ///
    /// # 错误
    /// 目标不是当前发布版本或写入失败时返回错误。
    async fn commit_retire(
        &self,
        graph: DefinitionGraph,
        policy: ProcessRequiredApprovalPolicy,
        request: RetireDefinitionRequest,
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
                retire_tx(
                    &db,
                    &rbac,
                    graph,
                    RetireTxInput {
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
        self.recover_definition_command(outcome.await, &policy, actor, identity, expectation).await
    }
}

/// 退役事务的政策、请求、命令身份与操作人。
///
/// # 用途
/// 打包 [`retire_tx`] 的非基础设施参数。
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
/// 只能退役当前已发布定义。
struct RetireTxInput<'a> {
    /// 单据审批政策。
    policy: ProcessRequiredApprovalPolicy,
    /// 退役请求。
    request: RetireDefinitionRequest,
    /// 审计操作人。
    actor: &'a AuditActor,
    /// 已规范化的当前命令身份及精确旧格式候选。
    identity: &'a PreparedDefinitionIdentity,
    /// Injected audit port.
    audit: &'a dyn crate::ports::WorkflowAuditPort,
}

/// 事务内退役当前发布版本。
///
/// # 用途
/// 回放收据或退役当前已发布定义并写入审计。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `graph` - 当前定义图
/// * `input` - 政策、请求、摘要与操作人
/// * `session` - 事务会话
///
/// # 返回
/// 返回退役后的定义详情。
///
/// # 错误
/// 目标不是当前发布版本或写入失败时返回错误。
///
/// # 关键业务约束
/// 已有收据必须原样回放，不得重复 persist。
async fn retire_tx(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    graph: DefinitionGraph,
    input: RetireTxInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<DefinitionDetailView> {
    let RetireTxInput { policy, request, actor, identity, audit } = input;
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
    let published = db.bpm_workflow().find_published_by_process_kind(policy.process_kind, session).await?;
    let RetireWriteStep::RetireCurrentPublished = decide_retire_write(
        published.as_ref(),
        &graph.definition.base.id,
        request.expected_definition_lock_version,
    )?;
    let Some(mut retired) = published else {
        return Err(Error::BusinessLogicError("当前没有可退役的已发布定义".to_string()));
    };
    let expected = retired.definition_lock_version();
    retired.retire(participant(actor)?, now()?).map_err(map_model_error)?;
    let graph =
        DefinitionGraph { definition: retired.clone(), nodes: graph.nodes, transitions: graph.transitions };
    let result_ref = DefinitionCommandResultRef::from_graph(&graph).encode();
    write_receipt(db, &identity.current, &result_ref, session).await?;
    retired.base.version = expected;
    db.approval_process_definitions().update(&mut retired, session).await?;
    let graph = DefinitionGraph { definition: retired, nodes: graph.nodes, transitions: graph.transitions };
    write_definition_audit(
        audit,
        actor,
        "approval_definition.retire",
        &graph,
        Some(request.expected_definition_lock_version),
        None,
        session,
    )
    .await?;
    Ok(detail_view(&graph))
}

/// 退役写库步骤。仅当前 PUBLISHED 且锁匹配才允许写回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetireWriteStep {
    /// 允许对当前已发布定义执行 retire + CAS 写回。
    RetireCurrentPublished,
}

/// 只能退役当前已发布定义。
///
/// # 错误
/// 无 PUBLISHED 或请求 ID 不是当前发布版时返回业务错误。
fn ensure_retire_target(published_id: Option<&str>, requested_id: &str) -> Result<()> {
    let Some(published_id) = published_id else {
        return Err(Error::BusinessLogicError("当前没有可退役的已发布定义".to_string()));
    };
    if published_id != requested_id {
        return Err(Error::BusinessLogicError("只能退役当前已发布定义".to_string()));
    }
    Ok(())
}

/// 目标与锁均匹配后才允许退役写回。
///
/// # 错误
/// 无发布版、ID 不匹配或锁版本陈旧时返回错误。
fn decide_retire_write(
    published: Option<&ApprovalProcessDefinition>,
    requested_id: &str,
    expected_lock: u64,
) -> Result<RetireWriteStep> {
    ensure_retire_target(published.map(|item| item.base.id.as_str()), requested_id)?;
    let published =
        published.ok_or_else(|| Error::BusinessLogicError("当前没有可退役的已发布定义".to_string()))?;
    ensure_lock(published, expected_lock)?;
    Ok(RetireWriteStep::RetireCurrentPublished)
}

#[cfg(test)]
mod tests {
    use bpm::{ParticipantId, ProcessKind, Timestamp};

    use super::super::super::definition_dto::DefinitionConfigurationStatus;
    use super::super::super::policy::ApprovalRequirement;
    use super::super::mapping::configuration_status;
    use super::super::test_support::{draft_definition, production_source, source_fn};
    use super::*;
    use crate::error::ErrorCode;

    /// 只能退役当前 PUBLISHED；无发布版或非当前版失败，锁匹配才允许写回。
    #[test]
    fn retire_only_current_published() {
        assert!(matches!(
            ensure_retire_target(None, "def-1"),
            Err(Error::BusinessLogicError(message)) if message.contains("没有可退役")
        ));
        assert!(matches!(
            ensure_retire_target(Some("def-pub"), "def-draft"),
            Err(Error::BusinessLogicError(message)) if message.contains("只能退役当前已发布定义")
        ));
        ensure_retire_target(Some("def-1"), "def-1").expect("ID 匹配应放行");

        let published = {
            let mut definition = draft_definition(ProcessKind::StockAdjustment, "n1");
            definition
                .publish(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(2).unwrap())
                .unwrap();
            definition
        };
        let lock = published.definition_lock_version();
        assert!(matches!(
            decide_retire_write(None, "def-1", lock),
            Err(Error::BusinessLogicError(message)) if message.contains("没有可退役")
        ));
        assert!(matches!(
            decide_retire_write(Some(&published), "other-id", lock),
            Err(Error::BusinessLogicError(message)) if message.contains("只能退役当前已发布定义")
        ));
        assert!(matches!(
            decide_retire_write(Some(&published), &published.base.id, lock + 1),
            Err(Error::Coded(ErrorCode::ApprovalDefinitionVersionConflict))
        ));
        assert!(matches!(
            decide_retire_write(Some(&published), &published.base.id, lock),
            Ok(RetireWriteStep::RetireCurrentPublished)
        ));
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, None, None),
            DefinitionConfigurationStatus::MissingConfiguration
        );
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, None, Some(1)),
            DefinitionConfigurationStatus::Draft
        );
        let retire_tx = source_fn(production_source(), "async fn retire_tx", "async fn build_new_draft");
        assert!(retire_tx.contains("decide_retire_write"));
    }
}
