//! 不可变来源证据的审计、根事务与原失败恢复。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_identity::AccessTarget;
use erp_permissions::supplier_settlement;
use erp_supplier_orders::supplier_after_sales::settlement_source;
use erp_supply::dto::supplier_settlement::*;
use erp_supply::service::supplier_settlement::source::source_request_hash;
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SupplierSettlementProcess;
use crate::{Error, Result};
impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 逐行核验并登记不可变来源批次，供结算试算回读。
    /// 成本及退款三元组仅从 D32 正式事实派生；客户端只能补充运费、服务费、取消时间和供应商账单行，且必须提供正式证据引用。
    ///
    /// 周期、策略、订单行归属、期间内事实或金额恒等不符时拒绝；幂等键不能用于不同载荷，写入失败后只读查证原请求结果。
    pub async fn record_source_evidence(
        &self,
        req: RecordSettlementSourceEvidenceRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementSourceEvidenceView> {
        req.validate()?;
        let process = self.clone();
        let command = req.clone();
        let writer = actor.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.record_source(&command, &writer, executor).await })
            })
            .await;

        let Err(error) = result else { return result };
        let process = self.clone();
        let actor = actor.clone();
        let recovered = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process
                        .authorization
                        .require(
                            &actor,
                            &supplier_settlement::SOURCE_EVIDENCE_RECORD,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;
                    Ok::<_, Error>(
                        process
                            .domain()
                            .replay_source(&req.request_id, &source_request_hash(&req), executor)
                            .await?,
                    )
                })
            })
            .await?;
        match recovered {
            Some(result) => Ok(result),
            None => Err(error),
        }
    }

    // 在原事务核验录入资格并查找同载荷回执；首次录入时核对来源、关联正式证据、保存批次和审计，保证依据与批次一起提交。
    async fn record_source(
        &self,
        req: &RecordSettlementSourceEvidenceRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementSourceEvidenceView> {
        self.authorization
            .require(actor, &supplier_settlement::SOURCE_EVIDENCE_RECORD, AccessTarget::Operation, executor)
            .await?;

        let request_hash = source_request_hash(req);
        if let Some(existing) = self.domain().replay_source(&req.request_id, &request_hash, executor).await? {
            return Ok(existing);
        }

        let evidence = self.domain().build_source_evidence(req, actor.id(), executor, request_hash).await?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.source_evidence.record",
            "supplier_settlement_source_evidence",
            evidence.base.id.clone(),
        )?;

        settlement_source::link(&self.db, &evidence, executor).await?;
        self.domain().persist_source_evidence(&evidence, executor).await?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(evidence.into())
    }
}
