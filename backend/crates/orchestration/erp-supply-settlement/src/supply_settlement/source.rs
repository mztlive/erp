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
    /// 录入一个经服务端逐行核验、不可变且可幂等恢复的来源证据批次。
    ///
    /// 订单成本和退款三元组只从 D32 正式事实派生；客户端只能补充仓库尚无模型的
    /// 运费、服务费、取消时间与供应商账单行，并必须携带正式证据引用。
    ///
    /// # 参数
    /// * `req` - 客户端来源证据命令
    /// * `actor` - 已鉴权记录人
    ///
    /// # 返回
    /// 返回新建或幂等恢复的不可变来源证据概要。
    ///
    /// # 错误
    /// 策略/周期非法、订单与行不精确配对、事实不在周期内、金额恒等失败或幂等键
    /// 被不同命令复用时 fail-closed。
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

    /// 全部来源校验、正式关联和证据写入沿用入口快照。
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
