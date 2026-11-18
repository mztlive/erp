//! 结案文件、账户范围、原执行终态和正式资金共同核验，HTTP输入不能自行解除保护。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_finance::entity::supplier_deposit::{
    DepositReconciliationResolution, DepositReleaseReason, DepositReleaseVerification,
    SupplierDepositReconciliation, SupplierDepositReservation,
};
use erp_finance::repository::SupplierDepositExt;
use erp_finance::service::supplier_deposit::{SupplierDepositService, VerifiedDepositRelease};
use erp_identity::AuthorizationService;
use erp_supply::api_order::{ApiOrderExt, ApiOrderStage, ExecutionFundsFinality};
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::repository::SupplierFulfillmentExt;
use erp_support::FileAssetExt;
use erp_support::authorization::files::require_owned;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::super::access::{account_access, audit};
use super::{
    DepositManagementContext, ReleaseDepositReservation, ResolveDepositReconciliation,
    SupplierDepositManagementProcess,
};
use crate::{Error, Result};

impl SupplierDepositManagementProcess {
    /// 结案原资金差异，完整账户核验通过后恢复新增使用。
    /// # 参数
    /// 原账户、差异记录、所见版本与核验文件及当前认证财务人员。
    /// # 返回
    /// 返回保留原差异、后续核对与核验人依据的记录。
    /// # 错误
    /// 越范围、附件不可用、未决资金或记录变化时整体拒绝。
    pub async fn resolve_reconciliation(
        &self,
        account: &str,
        id: &str,
        command: ResolveDepositReconciliation,
        actor: &AuditActor,
    ) -> Result<SupplierDepositReconciliation> {
        let context = self.command_context(account, actor);
        let id = id.to_owned();
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_resolution(context, &id, command, executor).await })
            })
            .await
    }

    /// 按核实终态释放本执行全部未使用尾款，不扩大或改写原付款批准。
    /// # 参数
    /// 原账户、占用、所见版本及原核对/文件依据和认证操作人。
    /// # 返回
    /// 返回带稳定回执的原占用，账户账面金额保持不变。
    /// # 错误
    /// 未知外部动作、未结清使用、陈旧核对、越范围或文件不可用时拒绝。
    pub async fn release_reservation(
        &self,
        account: &str,
        id: &str,
        command: ReleaseDepositReservation,
        actor: &AuditActor,
    ) -> Result<SupplierDepositReservation> {
        let context = self.command_context(account, actor);
        let id = id.to_owned();
        context
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_release(context, &id, command, executor).await })
            })
            .await
    }
}

async fn apply_resolution(
    context: DepositManagementContext,
    id: &str,
    command: ResolveDepositReconciliation,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositReconciliation> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;
    account_access(&db, &authorization, &account, &actor, "resolve", executor).await?;
    let previous = db
        .supplier_deposit_reconciliations()
        .find_by_id(id, executor)
        .await?
        .filter(|record| record.account_id == account)
        .ok_or_else(|| Error::NotFound("原资金核对记录不存在".into()))?;
    let replay = previous.resolution.is_some();
    if !replay {
        resolution_asset(&db, &authorization, &command.evidence_asset_id, &actor, executor).await?;
    }

    let resolution = DepositReconciliationResolution {
        operation_key: command.operation_key.clone(),
        payload_fingerprint: payload(&(id, &command.balanced_reconciliation_id, &command.evidence_asset_id))?,
        balanced_reconciliation_id: command.balanced_reconciliation_id,
        evidence_asset_id: command.evidence_asset_id,
        actor_id: actor.id().into(),
        resolved_at: Instant::now(),
    };
    let result = SupplierDepositService::new(db.clone())
        .resolve_reconciliation(&account, id, command.expected_version, resolution, executor)
        .await?;
    if !replay {
        audit(&db, &actor, "resolve", &account, executor).await?;
    }
    Ok(result)
}

async fn apply_release(
    context: DepositManagementContext,
    id: &str,
    command: ReleaseDepositReservation,
    executor: &mut dyn Executor,
) -> Result<SupplierDepositReservation> {
    let DepositManagementContext { db, authorization, account, actor, .. } = context;
    let funds = account_access(&db, &authorization, &account, &actor, "release", executor).await?;
    let verification = DepositReleaseVerification {
        reconciliation_id: command.reconciliation_id,
        evidence_asset_id: command.evidence_asset_id,
        reason: command.reason,
        verified_at: Instant::now(),
    };
    let service = SupplierDepositService::new(db.clone());
    if let Some(result) = service
        .verified_release_result(&account, id, &command.operation_key, &verification, actor.id(), executor)
        .await?
    {
        return Ok(result);
    }

    if funds.reconciliation_required {
        return Err(Error::BusinessLogicError("账户仍待对账，请先完成差异结案".into()));
    }
    resolution_asset(&db, &authorization, &verification.evidence_asset_id, &actor, executor).await?;
    let last_external_fact_at = release_finality(&db, &account, id, command.reason, executor).await?;

    let result = service
        .release_verified_reservation(
            VerifiedDepositRelease {
                account: &account,
                reservation_id: id,
                expected_version: command.expected_version,
                operation_key: &command.operation_key,
                verification,
                last_external_fact_at,
                actor_id: actor.id(),
            },
            executor,
        )
        .await?;
    audit(&db, &actor, "release", &account, executor).await?;
    Ok(result)
}

async fn release_finality(
    db: &Database,
    account: &str,
    id: &str,
    reason: DepositReleaseReason,
    executor: &mut dyn Executor,
) -> Result<Instant> {
    let reservation = db
        .supplier_deposit_reservations()
        .find_by_id(id, executor)
        .await?
        .filter(|record| record.account_id == account)
        .ok_or_else(|| Error::NotFound("原资金占用不存在".into()))?;
    let mut execution = db
        .supplier_api_order_executions()
        .find_by_id(&reservation.basis.execution_group_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原采购执行不存在".into()))?;
    if execution.purchase_order_id != reservation.basis.purchase_order_id
        || execution.purchase_order_revision_id != reservation.basis.purchase_revision_id
    {
        return Err(Error::ConflictError("原资金占用与采购执行版本不一致".into()));
    }

    let order = db
        .supplier_fulfillment_orders()
        .find_by_id(&execution.base.id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原供应商履约单不存在".into()))?;
    let mut tracking = db.supplier_order_tracking().find_by_id(&execution.base.id, executor).await?;
    let observation = match tracking.as_ref().and_then(|value| value.latest_observation_id.as_deref()) {
        Some(id) => db.supplier_order_observations().find_by_id(id, executor).await?,
        None => None,
    };
    let expected = match reason {
        DepositReleaseReason::FailedWithoutCharge => ExecutionFundsFinality::NeverSubmitted,
        DepositReleaseReason::ChargesSettled => ExecutionFundsFinality::Completed,
        DepositReleaseReason::CancelledAndReturned => ExecutionFundsFinality::Cancelled,
    };
    let time = execution.ensure_funds_finality(expected, &order, tracking.as_ref(), observation.as_ref())?;

    execution.stage = ApiOrderStage::Stopped;
    execution.blocker = Some("本执行资金风险已核实，剩余占用已按原依据释放".into());
    db.supplier_api_order_executions().update(&mut execution, executor).await?;
    if let Some(tracking) = &mut tracking {
        db.supplier_order_tracking().update(tracking, executor).await?;
    }
    Ok(time)
}

async fn resolution_asset(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut asset = db
        .file_assets()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("资金核验附件不存在，请重新上传".into()))?;
    if !asset.is_usable_at(Instant::now()) {
        return Err(Error::Forbidden("资金核验附件不可用，请通过安全检查后重试".into()));
    }
    require_owned(authorization, actor, &[id], executor).await?;
    db.file_assets().update(&mut asset, executor).await?;
    Ok(())
}

fn payload(value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| Error::Internal("资金结案请求无法编码".into()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
