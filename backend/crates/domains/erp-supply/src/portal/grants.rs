//! 定向报价资格的精确领域仓储入口，所有写入复用调用方执行器。

mod repository;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use id_generator::next_id;
use persistence_core::Executor;

use self::repository::PortalQuoteGrantRepository;
use super::{PortalOfferingService, PortalSupplyExt, QuoteAccessGrant};
use crate::{Error, Result};

impl PortalOfferingService {
    /// 读取精确供应商与SKU的报价开放事实。
    /// # 参数
    /// 供应商、SKU及当前执行器。
    /// # 返回
    /// 返回现有资格记录，缺失为空。
    /// # 错误
    /// 持久化读取失败时返回错误。
    pub async fn quote_access(
        &self,
        supplier_id: &str,
        sku_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<QuoteAccessGrant>> {
        PortalQuoteGrantRepository::new(&self.db).get(supplier_id, sku_id, executor).await
    }

    /// 列出精确供应商的定向报价资格及历史。
    /// # 参数
    /// 供应商及当前执行器；对象授权由调用方在相同执行器完成。
    /// # 返回
    /// 返回该供应商全部资格记录。
    /// # 错误
    /// 持久化读取失败时返回错误。
    pub async fn quote_access_list(
        &self,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<QuoteAccessGrant>> {
        PortalQuoteGrantRepository::new(&self.db).list(supplier_id, executor).await
    }

    /// 按原版本更新定向报价资格并追加变更历史。
    /// # 参数
    /// 精确供应商/SKU、新状态、预期版本、真实内部身份和调用方执行器。
    /// # 返回
    /// 返回已保存资格的新版本。
    /// # 错误
    /// 版本冲突、外部身份或持久化失败时拒绝。
    pub async fn quote_access_update(
        &self,
        supplier_id: &str,
        sku_id: &str,
        active: bool,
        expected_version: Option<u64>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<QuoteAccessGrant> {
        let repo = self.db.portal_quote_grants();
        let mut grant = match self.quote_access(supplier_id, sku_id, executor).await? {
            Some(mut grant) => {
                if expected_version != Some(grant.base.version) {
                    return Err(Error::ConflictError("报价开放版本已变化".into()));
                }
                grant.set_active(active, actor, Instant::now())?;
                repo.update(&mut grant, executor).await?;
                grant
            },
            None => {
                if expected_version.is_some() {
                    return Err(Error::ConflictError("报价开放记录已变化".into()));
                }
                let mut grant = QuoteAccessGrant::new(next_id(), supplier_id, sku_id, actor)?;
                grant.set_active(active, actor, Instant::now())?;
                repo.create(&grant, executor).await?;
                grant
            },
        };
        grant.active = active;
        Ok(grant)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::*;

    #[test]
    fn reactivation_preserves_original_opening_and_revocation_evidence() {
        let owner = AuditActor::new("buyer-a".into(), "buyer-a".into(), AccountKind::Admin);
        let other = AuditActor::new("buyer-b".into(), "buyer-b".into(), AccountKind::Admin);
        let mut grant = QuoteAccessGrant::new("grant".into(), "supplier", "sku", &owner).unwrap();
        let opened_at = grant.opened_at;
        grant.set_active(false, &other, Instant::from_unix_secs(100)).unwrap();
        grant.set_active(true, &other, Instant::from_unix_secs(101)).unwrap();
        assert_eq!(grant.opened_by, "buyer-a");
        assert_eq!(grant.opened_at, opened_at);
        assert_eq!(grant.revoked_by.as_deref(), Some("buyer-b"));
        assert_eq!(grant.revoked_at, Some(Instant::from_unix_secs(100)));
        assert_eq!(grant.history.len(), 3);
        assert!(grant.history[0].active);
        assert!(!grant.history[1].active);
        assert!(grant.history[2].active);
        let external = AuditActor::new("supplier-account".into(), "external".into(), AccountKind::Supplier);
        assert!(grant.set_active(false, &external, Instant::from_unix_secs(102)).is_err());
        assert_eq!(grant.history.len(), 3);
    }
}
