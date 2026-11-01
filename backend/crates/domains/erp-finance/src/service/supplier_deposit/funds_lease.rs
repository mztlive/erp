//! 多实例账号调查的领取、事务写屏障及受控释放。

use entity_core::BaseModel;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use super::{SupplierDepositService, fingerprint, require_transaction};
use crate::entity::supplier_deposit::{FundsScanLease, funds_lease_expiration, validate_funds_lease_owner};
use crate::repository::FundsEvidenceExt;
use crate::{Error, Result};

impl SupplierDepositService {
    /// 领取账号调查持有权；账户停用不阻断原资金事实调查。
    /// # 参数
    /// * `account_id` / `owner` - 已存在资金账户及本次处理者身份。
    /// * `now` / `ttl_secs` / `executor` - 服务器时间、期限和领取事务。
    /// # 返回
    /// 返回新的单调持有版本；其他持有者尚未到期时返回None。
    /// # 错误
    /// 未知账户、身份期限非法、版本溢出或数据库失败时拒绝。
    pub async fn claim_funds_lease(
        &self,
        account_id: &str,
        owner: &str,
        now: Instant,
        ttl_secs: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<FundsScanLease>> {
        require_transaction(executor)?;
        validate_funds_lease_owner(owner)?;
        let expires_at = funds_lease_expiration(now, ttl_secs)?;
        self.account(account_id, executor).await?;
        let id = lease_id(account_id)?;
        if let Some(mut lease) = self.db.funds_scan_leases().find_by_id(&id, executor).await? {
            if !lease.claim(owner, now, ttl_secs)? {
                return Ok(None);
            }
            self.db.funds_scan_leases().update(&mut lease, executor).await?;
            return Ok(Some(lease));
        }
        let lease = FundsScanLease {
            base: BaseModel::new(id),
            account_id: account_id.into(),
            owner_id: owner.into(),
            expires_at,
            fencing_token: 1,
        };
        self.db.funds_scan_leases().create(&lease, executor).await?;
        Ok(Some(lease))
    }

    /// 在收件或记账同一事务写入租约CAS，阻断旧持有者提交。
    /// # 参数
    /// * `account_id` / `owner` / `fencing_token` - 原领取的完整身份。
    /// * `now` / `executor` - 当前服务器时间及业务写入同一执行器。
    /// # 返回
    /// 当前持有者写屏障成功；不隐式延长期限。
    /// # 错误
    /// 已过期、持有者变化或CAS竞争时拒绝整个调用方事务。
    pub async fn assert_funds_lease(
        &self,
        account_id: &str,
        owner: &str,
        fencing_token: u64,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        require_transaction(executor)?;
        let mut lease = self
            .db
            .funds_scan_leases()
            .find_by_id(&lease_id(account_id)?, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("账号资金调查持有权不存在".into()))?;
        lease.ensure_owner(owner, fencing_token, now)?;
        self.db.funds_scan_leases().update(&mut lease, executor).await?;
        Ok(())
    }

    /// 仅由原持有版本释放账号调查，旧执行者不能释放新处理者。
    /// # 参数
    /// * `account_id` / `owner` / `fencing_token` - 原领取身份。
    /// * `now` / `executor` - 服务器时间和释放事务。
    /// # 返回
    /// 原租约到期，原证据全部保留。
    /// # 错误
    /// 持有者或版本不一致、数据库失败时拒绝。
    pub async fn release_funds_lease(
        &self,
        account_id: &str,
        owner: &str,
        fencing_token: u64,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        require_transaction(executor)?;
        let mut lease = self
            .db
            .funds_scan_leases()
            .find_by_id(&lease_id(account_id)?, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("账号资金调查持有权不存在".into()))?;
        lease.release(owner, fencing_token, now)?;
        self.db.funds_scan_leases().update(&mut lease, executor).await?;
        Ok(())
    }
}

fn lease_id(account: &str) -> Result<String> {
    Ok(format!("funds-lease-{}", fingerprint(&account)?))
}
