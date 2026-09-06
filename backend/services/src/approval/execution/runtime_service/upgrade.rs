//! 未提交单据绑定升级。

use bpm::ids::ApprovalCommandReceiptId;
use entities::document_registry::{DocumentType, WorkflowActionId};
use id_generator::next_id;
use persistence_core::Transactional;

use super::super::idempotency::{
    command_may_have_committed, command_recovery_delay, normalize_idempotency_key, upgrade_binding_identity,
};
use super::ApprovalRuntimeService;
use crate::approval::binding::{
    replay_unsubmitted_document_definition_upgrade, upgrade_unsubmitted_document_definition,
    UpgradeBindingResultView, UpgradeUnsubmittedDefinitionCommand,
};
use crate::errors::{Error, Result};
use application_core::AuditActor;

/// 绑定升级命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeBindingCommand {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 单据 ID。
    pub document_id: String,
    /// 升级原因。
    pub reason: String,
    /// 期望单据版本。
    pub expected_document_version: u64,
    /// 期望绑定版本。
    pub expected_approval_binding_version: u64,
    /// 幂等键。
    pub idempotency_key: String,
}

impl ApprovalRuntimeService {
    /// 升级未提交单据绑定到当前发布定义。
    ///
    /// # 错误
    /// 已提交或版本冲突时返回错误。
    pub async fn upgrade_binding(
        &self,
        actor: &AuditActor,
        command: UpgradeBindingCommand,
    ) -> Result<UpgradeBindingResultView> {
        let reason = command.reason.trim().to_string();
        if reason.is_empty() {
            return Err(Error::ValidationError("升级原因不能为空".to_string()));
        }
        let idempotency_key = normalize_idempotency_key(&command.idempotency_key)?;
        let identity = upgrade_binding_identity(
            command.document_type.as_str(),
            &command.document_id,
            command.expected_document_version,
            command.expected_approval_binding_version,
            &reason,
            actor.id(),
            idempotency_key,
        )?;
        let prepared = UpgradeUnsubmittedDefinitionCommand {
            document_type: command.document_type,
            document_id: command.document_id,
            expected_business_object_version: command.expected_document_version,
            expected_binding_version: command.expected_approval_binding_version,
            reason,
            identity,
            action_id: WorkflowActionId::new(next_id()),
            receipt_id: ApprovalCommandReceiptId::new(next_id()),
        };
        match self.commit_upgrade_binding(actor, prepared.clone()).await {
            Ok(view) => Ok(view),
            Err(error) if command_may_have_committed(&error) => {
                self.recover_upgrade_binding(actor, prepared, error).await
            }
            Err(error) => Err(error),
        }
    }

    /// 在唯一调用方事务内执行升级；绑定端口不得自行开启嵌套事务。
    async fn commit_upgrade_binding(
        &self,
        actor: &AuditActor,
        command: UpgradeUnsubmittedDefinitionCommand,
    ) -> Result<UpgradeBindingResultView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    upgrade_unsubmitted_document_definition(&db, &rbac, &command, &actor, session).await
                })
            })
            .await
    }

    /// 唯一键竞争或提交结果未知后，以新事务重验授权并只读胜者收据。
    async fn recover_upgrade_binding(
        &self,
        actor: &AuditActor,
        command: UpgradeUnsubmittedDefinitionCommand,
        original_error: Error,
    ) -> Result<UpgradeBindingResultView> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let rbac = self.rbac.clone();
            let actor = actor.clone();
            let command = command.clone();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        replay_unsubmitted_document_definition_upgrade(&db, &rbac, &command, &actor, session)
                            .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }
}
