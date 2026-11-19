//! 供应商账号绑定集合及其固定归属查询。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::portal::PortalBinding;

/// 外部账号绑定仓储。
pub type PortalBindingRepository<'a> = Repository<'a, PortalBinding>;

/// 统一取得身份域持有的供应商门户绑定集合。
pub trait PortalIdentityExt {
    /// 绑定集合名，由仓储与索引共同使用。
    const PORTAL_BINDINGS: &'static str = "supplier_portal_bindings";

    /// 在处理供应商门户归属时取得账号绑定仓储。
    fn portal_bindings(&self) -> PortalBindingRepository<'_>;
}

impl PortalIdentityExt for Database {
    // 使用 `PORTAL_BINDINGS` 固定集合名，不由组合层重新拼接。
    fn portal_bindings(&self) -> PortalBindingRepository<'_> {
        PortalBindingRepository::new(self, Self::PORTAL_BINDINGS)
    }
}

/// 为供应商账号绑定提供单账号和单供应商归属查询。
#[allow(async_fn_in_trait)]
pub trait PortalBindingRepositoryExt {
    /// 用调用方事务按稳定账号 ID 查唯一未删除绑定，包含已撤销关系；未知账号返回 `None`，持久化失败直接返回。
    async fn binding_for_account(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PortalBinding>>;

    /// 用调用方事务读取已获授权的指定供应商全部未删除实名绑定，包含启用和停用账号；不得扩大到其他供应商，持久化失败直接返回。
    async fn supplier_bindings(
        &self,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PortalBinding>>;
}

impl PortalBindingRepositoryExt for PortalBindingRepository<'_> {
    // 只按 `account_id` 取唯一未删除记录，不以启用状态隐藏撤销历史。
    async fn binding_for_account(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PortalBinding>> {
        self.find_one_by_field("account_id", account_id, executor).await
    }

    // 在 MongoDB 中按供应商 ID 限定未删除绑定，不通过内存过滤其他供应商。
    async fn supplier_bindings(
        &self,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PortalBinding>> {
        self.find_many(doc! { "supplier_id": supplier_id }, executor).await
    }
}
