//! W27 结算差异的不可变补证强命令。
//!
//! 审计和根事务由本流程组合；本域复验与 CAS 由供应链服务完成。
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_supply::dto::supplier_settlement::*;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::service::supplier_settlement::evidence::{evidence_command_hash, evidence_result};
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::SupplierSettlementProcess;
use crate::{Error, Result};
impl SupplierSettlementProcess {
    /// 为一个精确差异追加不可变证据引用与业务意见。
    ///
    /// 请求 ID 是数据库唯一幂等键；同一请求 ID 的载荷摘要不一致时冲突。补证不会
    /// 直接改变差异结论，正式处理仍需单独差异决定命令。
    pub async fn append_difference_evidence(
        &self,
        difference_id: &str,
        req: SettlementDifferenceEvidenceRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDifferenceEvidenceResult> {
        req.validate()?;
        if difference_id != req.difference_id {
            return Err(Error::ValidationError("差异路径ID与命令载荷不一致".to_string()));
        }
        let command_hash = evidence_command_hash(&req);
        if let Some(result) = self
            .domain()
            .replay_difference_evidence(&req.request_id, &req, &command_hash, &mut NoTransaction)
            .await?
        {
            return Ok(result);
        }
        let evidence = self
            .domain()
            .prepare_difference_evidence(difference_id, &req, actor.id(), &command_hash, &mut NoTransaction)
            .await?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.difference_evidence.append",
            "supplier_settlement_difference",
            difference_id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let evidence_for_tx = evidence.clone();
        let expected_difference_version = req.expected_difference_version;
        let difference_id_for_tx = difference_id.to_string();
        let statement_id_for_tx = req.statement_id.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SupplierSettlementService::new(db.clone())
                        .persist_difference_evidence(
                            &difference_id_for_tx,
                            &statement_id_for_tx,
                            expected_difference_version,
                            &evidence_for_tx,
                            session,
                        )
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            if let Some(result) = self
                .domain()
                .replay_difference_evidence(&evidence.request_id, &req, &command_hash, &mut NoTransaction)
                .await?
            {
                return Ok(result);
            }
            return Err(error);
        }
        Ok(evidence_result(evidence, "RECORDED", "差异补证已登记，不会直接改变正式差异结论"))
    }
}
