//! 账号资金调查独立租约；每次领取形成单调 fencing token。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct FundsScanLease {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub owner_id: String,
    pub expires_at: Instant,
    pub fencing_token: u64,
}

impl FundsScanLease {
    pub(crate) fn claim(&mut self, owner: &str, now: Instant, ttl_secs: u32) -> Result<bool> {
        validate_owner(owner)?;
        let expires = expiration(now, ttl_secs)?;
        if self.expires_at > now && self.owner_id != owner {
            return Ok(false);
        }
        let token =
            self.fencing_token.checked_add(1).ok_or_else(|| Error::from("资金调查持有版本已达上限"))?;
        self.owner_id = owner.into();
        self.expires_at = expires;
        self.fencing_token = token;
        Ok(true)
    }

    pub(crate) fn ensure_owner(&self, owner: &str, token: u64, now: Instant) -> Result<()> {
        if self.owner_id != owner || self.fencing_token != token || token == 0 || self.expires_at <= now {
            return Err(Error::from("资金调查持有权已过期或已由其他处理者接管"));
        }
        Ok(())
    }

    pub(crate) fn release(&mut self, owner: &str, token: u64, now: Instant) -> Result<()> {
        if self.owner_id != owner || self.fencing_token != token || token == 0 {
            return Err(Error::from("不能释放其他资金调查处理者的持有权"));
        }
        self.expires_at = self.expires_at.min(now);
        Ok(())
    }
}

pub(crate) fn expiration(now: Instant, ttl_secs: u32) -> Result<Instant> {
    if ttl_secs == 0 || ttl_secs > 900 {
        return Err(Error::from("资金调查持有期限须为1至900秒"));
    }
    let until = now
        .unix_secs()
        .checked_add(i64::from(ttl_secs))
        .ok_or_else(|| Error::from("资金调查持有期限越界"))?;
    Instant::try_from_unix_secs(until)
}
pub(crate) fn validate_owner(owner: &str) -> Result<()> {
    if owner.is_empty() || owner != owner.trim() || owner.len() > 256 {
        return Err(Error::from("资金调查处理者身份无效"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease() -> FundsScanLease {
        FundsScanLease {
            base: BaseModel::fake(),
            account_id: "account".into(),
            owner_id: "worker-1".into(),
            expires_at: Instant::from_unix_secs(100),
            fencing_token: 7,
        }
    }

    #[test]
    fn live_owner_blocks_takeover_and_expired_claim_fences_old_writer() {
        let mut current = lease();
        let original = current.clone();
        assert!(!current.claim("worker-2", Instant::from_unix_secs(99), 30).unwrap());
        assert_eq!(current, original);
        assert!(current.claim("worker-2", Instant::from_unix_secs(100), 30).unwrap());
        assert_eq!(current.fencing_token, 8);
        assert_eq!(current.expires_at, Instant::from_unix_secs(130));
        assert!(current.ensure_owner("worker-1", 7, Instant::from_unix_secs(101)).is_err());
        assert!(current.release("worker-1", 7, Instant::from_unix_secs(101)).is_err());
        current.ensure_owner("worker-2", 8, Instant::from_unix_secs(129)).unwrap();
        assert!(current.ensure_owner("worker-2", 8, Instant::from_unix_secs(130)).is_err());
    }

    #[test]
    fn same_owner_reentry_gets_new_fence_and_bad_inputs_do_not_mutate() {
        let mut current = lease();
        assert!(current.claim("worker-1", Instant::from_unix_secs(50), 20).unwrap());
        assert_eq!(current.fencing_token, 8);
        assert!(current.ensure_owner("worker-1", 7, Instant::from_unix_secs(51)).is_err());
        current.release("worker-1", 8, Instant::from_unix_secs(52)).unwrap();
        assert_eq!(current.expires_at, Instant::from_unix_secs(52));
        let original = current.clone();
        assert!(current.claim("worker-1", Instant::from_unix_secs(52), 0).is_err());
        assert_eq!(current, original);
        assert!(current.claim(" ", Instant::from_unix_secs(52), 30).is_err());
        current.fencing_token = u64::MAX;
        let exhausted = current.clone();
        assert!(current.claim("worker-1", Instant::from_unix_secs(52), 30).is_err());
        assert_eq!(current, exhausted);
    }
}
