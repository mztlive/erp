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
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SupplierSettlementProcess;
use crate::{Error, Result};
impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 为一个精确差异追加不可变证据引用与业务意见。
    ///
    /// 请求 ID 是数据库唯一幂等键；同一请求 ID 的载荷摘要不一致时冲突。补证不会
    /// 直接改变差异结论，正式处理仍需单独差异决定命令。
    ///
    /// # 参数
    /// * `difference_id` - 路径中的差异 ID。
    /// * `req` - 补证命令。
    /// * `actor` - 当前操作人。
    ///
    /// # 返回
    /// 返回已登记的补证结果；同一请求重放返回原结果。
    ///
    /// # 错误
    /// 请求校验失败、路径与载荷 ID 不一致、缺少差异操作或结算单补证资格、差异不可补证或事务写入失败时返回对应错误。
    /// 写入失败后若原请求已落库，则返回该结果而不是原错误。
    pub async fn append_difference_evidence(
        &self,
        difference_id: &str,
        req: SettlementDifferenceEvidenceRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDifferenceEvidenceResult> {
        req.validate()?;
        if difference_id != req.difference_id {
            return Err(Error::ValidationError("差异路径ID与命令载荷不一致".into()));
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

    /// 同一快照内核验固定资格、幂等收据和精确差异，再原子写入补证。
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
        let input = EvidenceWrite {
            db: self.db.clone(),
            difference_id: req.difference_id.clone(),
            statement_id: req.statement_id.clone(),
            expected_difference_version: req.expected_difference_version,
            evidence: evidence.clone(),
            audit,
        };
        input.persist(executor).await?;
        Ok(evidence_result(evidence, "RECORDED", "差异补证已登记，不会直接改变正式差异结论"))
    }

    /// 恢复读取前，在恢复事务内重验差异操作和结算单补证资格。
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

    /// 补证和重放均沿原执行器核验差异操作及实际结算单的补证范围。
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
}

struct EvidenceWrite {
    db: Database,
    difference_id: String,
    statement_id: String,
    expected_difference_version: u64,
    evidence: SupplierSettlementDifferenceEvidence,
    audit: AuditLog,
}

impl EvidenceWrite {
    /// 原补证和审计沿命令入口执行器原子写入，恢复仍由入口负责。
    async fn persist(self, executor: &mut dyn Executor) -> Result<()> {
        SupplierSettlementService::new(self.db.clone())
            .persist_difference_evidence(
                &self.difference_id,
                &self.statement_id,
                self.expected_difference_version,
                &self.evidence,
                executor,
            )
            .await?;
        persist_log(&self.db, &self.audit, executor).await?;
        Ok(())
    }
}
