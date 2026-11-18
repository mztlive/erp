//! 供给跨域提交：单域准备和写入委派，根事务内最后写原审计。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_supply::dto::supplier_offering::*;
use erp_supply::service::supplier_offering::CommandPreparation;
use persistence_core::Transactional;

use super::{SupplierOfferingProcess, recovery};
use crate::{Error, Result};
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
        let recovery_request = req.clone();
        let recovery_actor = actor.clone();
        let domain = self.domain();
        let qualification = self.qualification.clone();
        let actor = actor.clone();
        let db = self.db.clone();
        let transaction_result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let prepared =
                        domain.prepare_create(&req, &actor, qualification.as_ref(), executor).await?;
                    match prepared {
                        CommandPreparation::Replay(result) => Ok(result),
                        CommandPreparation::Apply(prepared) => {
                            let audit = actor.clone().resource_log_with_message(
                                "supplier_offering.create",
                                "supplier_offering",
                                prepared.offering.base.id.clone(),
                                Some(req.change_reason),
                            )?;
                            super::commit::created(&db, &prepared, &audit, executor).await?;
                            Ok::<_, Error>(prepared.result)
                        },
                    }
                })
            })
            .await;

        recovery::resolve(transaction_result, || self.created_result(recovery_request, recovery_actor)).await
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
        let recovery_request = req.clone();
        let recovery_actor = actor.clone();
        let recovery_id = id.to_owned();
        let id = id.to_owned();
        let domain = self.domain();
        let qualification = self.qualification.clone();
        let actor = actor.clone();
        let db = self.db.clone();
        let transaction_result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let prepared =
                        domain.prepare_revise(&id, &req, &actor, qualification.as_ref(), executor).await?;
                    match prepared {
                        CommandPreparation::Replay(result) => Ok(result),
                        CommandPreparation::Apply(mut prepared) => {
                            let audit = actor.clone().resource_log_with_message(
                                "supplier_offering.revise",
                                "supplier_offering",
                                prepared.offering.base.id.clone(),
                                Some(req.change_reason),
                            )?;
                            super::commit::revised(&db, &mut prepared, &audit, executor).await
                        },
                    }
                })
            })
            .await;

        recovery::resolve(transaction_result, || {
            self.revised_result(recovery_id, recovery_request, recovery_actor)
        })
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
        let recovery_request = req.clone();
        let recovery_actor = actor.clone();
        let recovery_id = id.to_owned();
        let id = id.to_owned();
        let domain = self.domain();
        let actor = actor.clone();
        let db = self.db.clone();
        let transaction_result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let prepared = domain.prepare_availability(&id, &req, &actor, executor).await?;
                    match prepared {
                        CommandPreparation::Replay(result) => Ok(result),
                        CommandPreparation::Apply(mut prepared) => {
                            let audit = actor.clone().resource_log_with_message(
                                "supplier_offering.availability.update",
                                "supplier_offering",
                                id,
                                Some(req.change_reason),
                            )?;
                            super::commit::availability(&db, &mut prepared, &audit, executor).await
                        },
                    }
                })
            })
            .await;

        recovery::resolve(transaction_result, || {
            self.availability_result(recovery_id, recovery_request, recovery_actor)
        })
        .await
    }
}
