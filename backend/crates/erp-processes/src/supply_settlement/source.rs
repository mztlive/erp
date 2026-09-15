//! 不可变来源证据的审计、根事务与原失败恢复。
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_supply::dto::supplier_settlement::*;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::service::supplier_settlement::source::source_request_hash;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::SupplierSettlementProcess;
use crate::Result;
impl SupplierSettlementProcess {
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
        let request_hash = source_request_hash(&req);
        if let Some(existing) =
            self.domain().replay_source(&req.request_id, &request_hash, &mut NoTransaction).await?
        {
            return Ok(existing);
        }
        let evidence = self
            .domain()
            .build_source_evidence(&req, actor.id(), &mut NoTransaction, request_hash.clone())
            .await?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.source_evidence.record",
            "supplier_settlement_source_evidence",
            evidence.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let evidence_for_tx = evidence.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SupplierSettlementService::new(db.clone())
                        .persist_source_evidence(&evidence_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            if let Some(existing) =
                self.domain().replay_source(&req.request_id, &request_hash, &mut NoTransaction).await?
            {
                return Ok(existing);
            }
            return Err(error);
        }
        Ok(evidence.into())
    }
}
