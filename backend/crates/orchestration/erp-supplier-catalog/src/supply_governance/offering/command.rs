//! 供给跨域提交：单域准备和写入委派，根事务内最后写原审计。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_supply::dto::supplier_offering::*;
use erp_supply::service::supplier_offering::CommandPreparation;
use persistence_core::{NoTransaction, Transactional};

use super::SupplierOfferingProcess;
use crate::Result;
impl SupplierOfferingProcess {
    /// 新增公司 SKU 的供应商供给。
    ///
    /// # 参数
    /// * `req` - 供给身份、首版条款和初始可供状态
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回供给、修订和可供投影主键。
    ///
    /// # 错误
    /// 公司 SKU/供应商/连接无效、资质不满足、字段非法或身份重复时返回错误。
    pub async fn create(
        &self,
        req: CreateSupplierOfferingRequest,
        actor: &AuditActor,
    ) -> Result<CreateSupplierOfferingResult> {
        let prepared = self
            .domain()
            .prepare_create(&req, actor, self.qualification.as_ref(), &mut NoTransaction)
            .await?;
        let CommandPreparation::Apply(prepared) = prepared else {
            let CommandPreparation::Replay(result) = prepared else { unreachable!() };
            return Ok(result);
        };
        let fingerprint = prepared.fingerprint.clone();
        let result = prepared.result.clone();
        let audit = actor.clone().resource_log_with_message(
            "supplier_offering.create",
            "supplier_offering",
            prepared.offering.base.id.clone(),
            Some(req.change_reason),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move { super::commit::created(&db, &prepared, &audit, executor).await })
            })
            .await;
        self.domain()
            .resolve_command_result(
                transaction_result,
                result,
                &req.idempotency_key,
                "create_offering",
                &fingerprint,
                &mut NoTransaction,
            )
            .await
    }
    /// 追加新的供给商业条款修订。
    ///
    /// # 参数
    /// * `id` - 供给主键
    /// * `req` - 新条款与期望版本
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回新修订号和供给状态。
    ///
    /// # 错误
    /// 供给不存在、版本冲突、资质不满足或条款非法时返回错误。
    pub async fn revise(
        &self,
        id: &str,
        req: ReviseSupplierOfferingRequest,
        actor: &AuditActor,
    ) -> Result<ReviseSupplierOfferingResult> {
        let prepared = self
            .domain()
            .prepare_revise(id, &req, actor, self.qualification.as_ref(), &mut NoTransaction)
            .await?;
        let CommandPreparation::Apply(mut prepared) = prepared else {
            let CommandPreparation::Replay(result) = prepared else { unreachable!() };
            return Ok(result);
        };
        let fingerprint = prepared.fingerprint.clone();
        let audit = actor.clone().resource_log_with_message(
            "supplier_offering.revise",
            "supplier_offering",
            prepared.offering.base.id.clone(),
            Some(req.change_reason),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move { super::commit::revised(&db, &mut prepared, &audit, executor).await })
            })
            .await;
        self.domain()
            .resolve_written_result(
                transaction_result,
                &req.idempotency_key,
                "revise_offering",
                &fingerprint,
                &mut NoTransaction,
            )
            .await
    }
    /// 更新供给的实时可供状态与数量。
    ///
    /// # 参数
    /// * `id` - 供给主键
    /// * `req` - 新可供事实
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回更新后的状态、版本和来源时间。
    ///
    /// # 错误
    /// 供给/投影不存在、版本冲突、来源时间倒退或数量非法时返回错误。
    pub async fn update_availability(
        &self,
        id: &str,
        req: UpdateSupplierOfferingAvailabilityRequest,
        actor: &AuditActor,
    ) -> Result<UpdateSupplierOfferingAvailabilityResult> {
        let prepared = self.domain().prepare_availability(id, &req, actor, &mut NoTransaction).await?;
        let CommandPreparation::Apply(mut prepared) = prepared else {
            let CommandPreparation::Replay(result) = prepared else { unreachable!() };
            return Ok(result);
        };
        let fingerprint = prepared.fingerprint.clone();
        let audit = actor.clone().resource_log_with_message(
            "supplier_offering.availability.update",
            "supplier_offering",
            id.to_string(),
            Some(req.change_reason),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(
                    async move { super::commit::availability(&db, &mut prepared, &audit, executor).await },
                )
            })
            .await;
        self.domain()
            .resolve_written_result(
                transaction_result,
                &req.idempotency_key,
                "update_offering_availability",
                &fingerprint,
                &mut NoTransaction,
            )
            .await
    }
}
