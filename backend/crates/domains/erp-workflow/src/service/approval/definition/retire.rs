use application_core::AuditActor;
use bpm::graph::DefinitionGraph;
use bpm::model::ApprovalProcessDefinition;
use bpm::model::types::ApprovalCommandKind;
use erp_permissions::approval_process;
use persistence_core::{Executor, Transactional};

use super::super::definition_dto::{DefinitionDetailView, RetireDefinitionRequest};
use super::ApprovalDefinitionService;
use super::command::{
    DefinitionCommandResultRef, DefinitionResultExpectation, PreparedDefinitionIdentity,
    RETIRE_DEFINITION_COMMAND_DOMAIN, ensure_lock, lock_command_identity, map_model_error, now,
    parse_idempotency_key, participant, replay_prepared_definition_receipt, write_definition_audit,
    write_receipt,
};
use super::mapping::detail_with_copies;
use crate::error::{Error, Result};
use crate::repository::BpmExt;

impl ApprovalDefinitionService {
    /// 退役当前已发布定义。
    ///
    /// # 参数
    /// * `request` - 退役请求
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回退役后的定义详情；同载荷回放返回已退役结果。
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
        self.commit_retire(request, actor, identity).await
    }

    /// 在唯一事务中退役当前发布版本。
    ///
    /// # 错误
    /// 目标不是当前发布版本或写入失败时返回错误。
    async fn commit_retire(
        &self,
        request: RetireDefinitionRequest,
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
                retire_tx(
                    &service,
                    RetireTxInput { request, actor: &transaction_actor, identity: &transaction_identity },
                    executor,
                )
                .await
            })
        });
        self.recover_definition_command(
            outcome.await,
            &approval_process::RETIRE_DEFINITION,
            actor,
            identity,
            expectation,
        )
        .await
    }
}

/// 退役事务的请求、命令身份与操作人。
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
    /// 退役请求。
    request: RetireDefinitionRequest,
    /// 审计操作人。
    actor: &'a AuditActor,
    /// 已规范化的当前命令身份及精确旧格式候选。
    identity: &'a PreparedDefinitionIdentity,
}

/// 事务内退役当前发布版本。
///
/// # 用途
/// 回放收据或退役当前已发布定义并写入审计。
///
/// # 参数
/// * `service` - 同一应用注入的定义服务与基础设施
/// * `input` - 请求、摘要与操作人
/// * `executor` - 执行器
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
    service: &ApprovalDefinitionService,
    input: RetireTxInput<'_>,
    executor: &mut dyn Executor,
) -> Result<DefinitionDetailView> {
    let RetireTxInput { request, actor, identity } = input;
    let graph = service
        .require_graph(&request.definition_id, actor, &approval_process::RETIRE_DEFINITION, executor)
        .await?;
    let db = &service.db;
    if let Some(view) = replay_prepared_definition_receipt(
        db,
        identity,
        &DefinitionResultExpectation::DefinitionId(request.definition_id.clone()),
        executor,
    )
    .await?
    {
        return Ok(view);
    }
    apply_retirement(service, graph, request, actor, identity, executor).await
}

/// 使用原事务图和当前唯一发布记录退役定义，仅更新定义文档。
async fn apply_retirement(
    service: &ApprovalDefinitionService,
    mut graph: DefinitionGraph,
    request: RetireDefinitionRequest,
    actor: &AuditActor,
    identity: &PreparedDefinitionIdentity,
    executor: &mut dyn Executor,
) -> Result<DefinitionDetailView> {
    let db = &service.db;
    let published =
        db.bpm_workflow().find_published_by_process_kind(graph.definition.process_kind, executor).await?;
    let mut retired =
        retire_target(published, &graph.definition.base.id, request.expected_definition_lock_version)?;
    let expected = retired.definition_lock_version();
    retired.retire(participant(actor)?, now()?).map_err(map_model_error)?;
    graph.definition = retired;
    let result_ref = DefinitionCommandResultRef::from_graph(&graph).encode();
    write_receipt(db, &identity.current, &result_ref, executor).await?;
    graph.definition.base.version = expected;
    db.approval_process_definitions().update(&mut graph.definition, executor).await?;
    write_definition_audit(
        service.audit.as_ref(),
        actor,
        "approval_definition.retire",
        &graph,
        Some(request.expected_definition_lock_version),
        None,
        executor,
    )
    .await?;
    detail_with_copies(db, &graph, executor).await
}

/// 取得目标和锁版本均匹配的当前发布定义。
/// # 错误
/// 无发布版、ID 不匹配或锁版本陈旧时拒绝。
fn retire_target(
    published: Option<ApprovalProcessDefinition>,
    requested_id: &str,
    expected_lock: u64,
) -> Result<ApprovalProcessDefinition> {
    let published =
        published.ok_or_else(|| Error::BusinessLogicError("当前没有可退役的已发布定义".into()))?;
    if published.base.id != requested_id {
        return Err(Error::BusinessLogicError("只能退役当前已发布定义".into()));
    }
    ensure_lock(&published, expected_lock)?;
    Ok(published)
}

#[cfg(test)]
mod tests {
    use bpm::{ParticipantId, ProcessKind, Timestamp};
    use persistence_core::testing::SnapshotExecutor;

    use super::super::super::definition_dto::DefinitionConfigurationStatus;
    use super::super::super::policy::ApprovalRequirement;
    use super::super::mapping::configuration_status;
    use super::super::test_support::{draft_definition, unavailable_service};
    use super::*;
    use crate::error::ErrorCode;

    #[tokio::test]
    async fn authorization_failure_precedes_definition_facts_receipts_and_lock_checks() {
        let (service, actor) = unavailable_service().await;
        let request = RetireDefinitionRequest {
            definition_id: "definition-never-read".into(),
            expected_definition_lock_version: 99,
            idempotency_key: "retire-retry".into(),
        };
        let identity = lock_command_identity(
            parse_idempotency_key(&request.idempotency_key).unwrap(),
            ApprovalCommandKind::RetireDefinition,
            RETIRE_DEFINITION_COMMAND_DOMAIN,
            &request.definition_id,
            request.expected_definition_lock_version,
            actor.id(),
        )
        .unwrap();
        let error = retire_tx(
            &service,
            RetireTxInput { request, actor: &actor, identity: &identity },
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }

    /// 只能退役当前 PUBLISHED；无发布版或非当前版失败，锁匹配才允许写回。
    #[test]
    fn retire_only_current_published() {
        let published = {
            let mut definition = draft_definition(ProcessKind::StockAdjustment, "n1");
            definition
                .publish(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(2).unwrap())
                .unwrap();
            definition
        };
        let lock = published.definition_lock_version();
        assert!(matches!(
            retire_target(None, "def-1", lock),
            Err(Error::BusinessLogicError(message)) if message.contains("没有可退役")
        ));
        assert!(matches!(
            retire_target(Some(published.clone()), "other-id", lock),
            Err(Error::BusinessLogicError(message)) if message.contains("只能退役当前已发布定义")
        ));
        assert!(matches!(
            retire_target(Some(published.clone()), &published.base.id, lock + 1),
            Err(Error::Coded(ErrorCode::ApprovalDefinitionVersionConflict))
        ));
        assert_eq!(retire_target(Some(published.clone()), &published.base.id, lock).unwrap(), published);
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, None, None),
            DefinitionConfigurationStatus::MissingConfiguration
        );
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, None, Some(1)),
            DefinitionConfigurationStatus::Draft
        );
    }
}
