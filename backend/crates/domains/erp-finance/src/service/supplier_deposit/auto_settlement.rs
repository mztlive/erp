//! 已保存真实扣款的自动核销；失败保留原使用，不在本入口扣款。

use persistence_core::Executor;

use super::{SupplierDepositService, identity, require_transaction};
use crate::entity::supplier_deposit::{
    DepositAutomaticSettlementPlan, SupplierDepositUsage, validate_deposit_actor,
};
use crate::repository::SupplierDepositExt;
use crate::repository::supplier_deposit_settlement::{
    automatic_settlement_allocations, automatic_settlement_entries, automatic_settlement_offsets,
};
use crate::{Error, Result};

impl SupplierDepositService {
    /// 将已保存真实扣款按原批准和全部来源开放分录自动核销。
    /// # 参数
    /// * `usage_id` / `operation_key` - 原真实使用及稳定自动核销命令。
    /// * `actor` - 实际系统执行身份，不冒充人工出纳。
    /// * `executor` - 使用、原申请和正式核销共同事务。
    /// # 返回
    /// 返回完整核销后的使用；重放原命令直接恢复原回执。
    /// # 错误
    /// 原扣款、批准、专属额度或完整开放金额不足时拒绝，原扣款不被删除。
    pub async fn settle_verified_usage(
        &self,
        usage_id: &str,
        operation_key: &str,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositUsage> {
        require_transaction(executor)?;
        validate_deposit_actor(actor)?;
        if operation_key.trim().is_empty()
            || operation_key != operation_key.trim()
            || operation_key.len() > 230
        {
            return Err(Error::ValidationError("自动核销原操作身份无效".into()));
        }
        let usage = self
            .db
            .supplier_deposit_usages()
            .find_by_id(usage_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("已核实预存款使用不存在".into()))?;
        let operation = format!("verified-usage:{operation_key}");
        let id = identity(&usage.account_id, &operation)?;
        if let Some(receipt) = self.db.supplier_deposit_settlements().find_by_id(&id, executor).await? {
            receipt.require_automatic_usage(&usage, &operation)?;
            return Ok(usage);
        }
        let reservation = self.reservation(&usage.reservation_id, executor).await?;
        let account = self.account(&usage.account_id, executor).await?;
        let request = self
            .request_for_reservation(&account.base.id, &account.identity, &reservation.basis, executor)
            .await?;
        let entries =
            automatic_settlement_entries(&self.db, &reservation.basis.payable_account_id, executor).await?;
        let allocations = automatic_settlement_allocations(&self.db, &entries, executor).await?;
        let offsets = automatic_settlement_offsets(&self.db, &entries, executor).await?;
        let plan = DepositAutomaticSettlementPlan::from_facts(
            &usage,
            &reservation,
            &request,
            &entries,
            &allocations,
            &offsets,
        )?;
        self.settle_usage(usage_id, &operation, &plan.allocations, actor, executor).await
    }
}
