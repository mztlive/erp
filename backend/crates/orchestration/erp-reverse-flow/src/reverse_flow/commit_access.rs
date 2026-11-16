//! 一次创建提交只提供服务端经办人与完整来源引用，政策和来源取证归 identity。
use application_core::{AuditActor, CommandReceipt};
use erp_identity::entity::authorization::{FactValue, ObjectFacts, RelatedObjects, RelationKey};
use erp_identity::{AccessTarget, ResourceTargets};
use erp_returns::ReturnsCommandReceiptService;
use erp_workflow::entity::document_registry::DocumentType;
use persistence_core::{Executor, Transactional};

use super::ReturnsProcess;
use super::command_recovery::recovered_resource;
use super::start_approval::{ensure_return_start_replay_authorized, submit_permission};
use crate::{Error, Result};

impl ReturnsProcess {
    /// 原事务核验创建提交资格和真实资金来源，不信任客户端提供经办人或来源事实。
    /// # 参数
    /// actor 为真实经办人，kind 和 source_id 为已校验命令的完整来源；executor 为原事务。
    /// # 返回
    /// 固定提交资格和真实来源全部通过时成功。
    /// # 错误
    /// 缺少资格、来源缺失或无权、执行边界错误时拒绝。
    pub(super) async fn require_commit_source(
        &self,
        actor: &AuditActor,
        kind: DocumentType,
        source_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let relation = match kind {
            DocumentType::CustomerRefund | DocumentType::ReceiptReversal => "receipt_source",
            DocumentType::SupplierRefund | DocumentType::PaymentReversal => "payment_source",
            _ => return Err(Error::ValidationError("未登记退款或冲正提交入口".into())),
        };
        let permission = submit_permission(kind)?;
        let mut candidate = ObjectFacts {
            resource: permission.resource().into(),
            owner_id: FactValue::Known(Some(actor.id().into())),
            relations: [(
                RelationKey::parse(relation)?,
                RelatedObjects {
                    expected_ids: ResourceTargets::new(&[source_id])?.ids().clone(),
                    objects: vec![],
                },
            )]
            .into(),
            ..Default::default()
        };
        let alternate = match kind {
            DocumentType::CustomerRefund => Some("receivable_source"),
            DocumentType::SupplierRefund => Some("payable_source"),
            _ => None,
        };
        if let Some(alternate) = alternate {
            candidate.relations.insert(RelationKey::parse(alternate)?, RelatedObjects::default());
        }
        self.rbac.identity.require(actor, &permission, AccessTarget::Candidate(candidate), executor).await?;
        Ok(())
    }

    /// 只读恢复命令；命中后按真实结果类型在原事务重验完整提交规则。
    /// # 参数
    /// receipt 为原请求身份，actor 和 kind 为实际命令上下文，executor 为原事务。
    /// # 返回
    /// 已提交且通过当前授权的结果身份，未命中返回 None。
    /// # 错误
    /// 收据不符、授权失败、岗位分离不满足或取证故障时拒绝。
    pub(super) async fn committed_return_command(
        &self,
        receipt: &CommandReceipt,
        actor: &AuditActor,
        kind: DocumentType,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        let result = ReturnsCommandReceiptService::new(self.db.clone())
            .committed_resource_id(receipt, executor)
            .await?;
        if let Some(id) = &result {
            ensure_return_start_replay_authorized(&self.db, &self.rbac, actor, kind, id, executor).await?;
        }
        Ok(result)
    }

    /// 只有可能提交的错误允许新快照只读查证；授权失败不能被旧结果覆盖。
    /// # 参数
    /// error 为原事务结果，其余参数为同一次请求的精确身份、来源和操作人。
    /// # 返回
    /// 新快照只读确认的原结果身份。
    /// # 错误
    /// 不能确认提交时保留原错误；确定失败不执行恢复读取。
    pub(super) async fn recover_return_commit(
        &self,
        error: Error,
        receipt: &CommandReceipt,
        actor: &AuditActor,
        kind: DocumentType,
        source_id: &str,
    ) -> Result<String> {
        if !error.command_may_have_committed() {
            return Err(error);
        }
        let this = self.clone();
        let receipt = receipt.clone();
        let actor = actor.clone();
        let source_id = source_id.to_owned();
        let recovered = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.require_commit_source(&actor, kind, &source_id, executor).await?;
                    this.committed_return_command(&receipt, &actor, kind, executor).await
                })
            })
            .await;
        recovered_resource(error, recovered)
    }
}
