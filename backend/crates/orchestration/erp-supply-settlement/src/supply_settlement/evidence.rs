//! W27 结算差异的不可变补证强命令。
//!
//! 审计和根事务由本流程组合；本域复验与 CAS 由供应链服务完成。
use application_core::AuditActor;
use erp_audit::execution::persist_log;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_identity::AccessTarget;
use erp_permissions::supplier_settlement;
use erp_supply::dto::supplier_settlement::*;
use erp_supply::entity::supplier_settlement::SupplierSettlementDifferenceEvidence;
use erp_supply::service::supplier_settlement::evidence::{evidence_command_hash, evidence_result};
use erp_supply::service::supplier_settlement::{SettlementAction, SupplierSettlementService};
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SupplierSettlementProcess;
use crate::{Error, Result};
impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 为指定差异追加不可变证据引用和业务意见，供后续财务判断，不直接改变差异结论。
    /// 请求 ID 全局唯一；同 ID 同载荷返回原结果，摘要不符时拒绝。
    ///
    /// 路径不一致、无差异操作或补证权限、差异不可补证时失败。写入失败后若查证原请求已提交，则返回对应结果。
    pub async fn append_difference_evidence(
        &self,
        difference_id: &str,
        req: SettlementDifferenceEvidenceRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDifferenceEvidenceResult> {
        req.validate()?;
        if difference_id != req.difference_id {
            return Err(Error::ValidationError("本次补证与当前打开的差异不符，请重新选择差异记录".into()));
        }

        let process = self.clone();
        let command = req.clone();
        let writer = actor.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.append_evidence(&command, &writer, executor).await })
            })
            .await;

        let Err(error) = result else { return result };
        let process = self.clone();
        let actor = actor.clone();
        let recovered = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.recover_evidence(&req, &actor, executor).await })
            })
            .await?;
        match recovered {
            Some(result) => Ok(result),
            None => Err(error),
        }
    }

    // 在同一事务核验功能资格、既有请求和精确差异；只在没有可复用结果时追加证据，避免重复登记或写入他单。
    async fn append_evidence(
        &self,
        req: &SettlementDifferenceEvidenceRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SettlementDifferenceEvidenceResult> {
        self.require_evidence_access(req, actor, executor).await?;

        let command_hash = evidence_command_hash(req);
        if let Some(result) =
            self.domain().replay_difference_evidence(&req.request_id, req, &command_hash, executor).await?
        {
            return Ok(result);
        }

        let evidence = self
            .domain()
            .prepare_difference_evidence(&req.difference_id, req, actor.id(), &command_hash, executor)
            .await?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.difference_evidence.append",
            "supplier_settlement_difference",
            req.difference_id.clone(),
        )?;
        self.persist(req, &evidence, &audit, executor).await?;
        Ok(evidence_result(evidence, "RECORDED", "差异补证已登记，不会直接改变正式差异结论"))
    }

    // 在独立恢复事务重验差异操作和本单补证资格，再查找原请求的补证结果，不重写证据。
    async fn recover_evidence(
        &self,
        req: &SettlementDifferenceEvidenceRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<SettlementDifferenceEvidenceResult>> {
        self.require_evidence_access(req, actor, executor).await?;
        Ok(self
            .domain()
            .replay_difference_evidence(&req.request_id, req, &evidence_command_hash(req), executor)
            .await?)
    }

    // 补证与读取回执共用功能权限和实际结算单范围校验，防止持有请求 ID 的无权账号绕过授权。
    async fn require_evidence_access(
        &self,
        req: &SettlementDifferenceEvidenceRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization
            .require(actor, &supplier_settlement::DIFFERENCE_DECIDE, AccessTarget::Operation, executor)
            .await?;
        Ok(self
            .data_scope
            .require_action(actor, SettlementAction::AppendEvidence, &req.statement_id, executor)
            .await?)
    }

    // 用入口事务写入不可变补证和审计，保证两者同时提交；写入失败后的原命令查证由入口处理。
    async fn persist(
        &self,
        req: &SettlementDifferenceEvidenceRequest,
        evidence: &SupplierSettlementDifferenceEvidence,
        audit: &AuditLog,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        SupplierSettlementService::new(self.db.clone())
            .persist_difference_evidence(
                &req.difference_id,
                &req.statement_id,
                req.expected_difference_version,
                evidence,
                executor,
            )
            .await?;
        persist_log(&self.db, audit, executor).await?;
        Ok(())
    }
}
