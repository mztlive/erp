//! 供应商账号绑定集合及其固定归属查询。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::portal::PortalBinding;

/// 外部账号绑定仓储。
pub type PortalBindingRepository<'a> = Repository<'a, PortalBinding>;

/// 身份域持有的供应商账号绑定集合访问器。
pub trait PortalIdentityExt {
    /// 绑定集合名，由仓储与索引共同使用。
    const PORTAL_BINDINGS: &'static str = "supplier_portal_bindings";

    /// 获取供应商账号绑定仓储。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 本域绑定仓储。
    /// # 错误
    /// 无。
    fn portal_bindings(&self) -> PortalBindingRepository<'_>;
}

impl PortalIdentityExt for Database {
    /// 绑定访问固定集合，禁止在组合层重新拼接集合名。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `PORTAL_BINDINGS` 集合上的绑定仓储。
    ///
    /// # 错误
    /// 不返回错误。
    fn portal_bindings(&self) -> PortalBindingRepository<'_> {
        PortalBindingRepository::new(self, Self::PORTAL_BINDINGS)
    }
}

/// 绑定仓储的账号与供应商归属查询。
#[allow(async_fn_in_trait)]
pub trait PortalBindingRepositoryExt {
    /// 查询指定账号的未删除绑定，包含已撤销关系。
    ///
    /// # 参数
    /// `account_id` 为账号稳定身份；`executor` 为调用方事务。
    /// # 返回
    /// 唯一绑定；未知账号返回空。
    /// # 错误
    /// 持久化访问失败时返回错误。
    async fn binding_for_account(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PortalBinding>>;

    /// 查询某家供应商全部未删除实名账号绑定。
    ///
    /// # 参数
    /// `supplier_id` 为服务端已授权目标；`executor` 为调用方事务。
    /// # 返回
    /// 包含启用及停用绑定，不扩大到其他供应商。
    /// # 错误
    /// 持久化访问失败时返回错误。
    async fn supplier_bindings(
        &self,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PortalBinding>>;
}

impl PortalBindingRepositoryExt for PortalBindingRepository<'_> {
    /// 查询唯一账号绑定。只按 `account_id` 取一条记录，不按 `active` 收窄。
    ///
    /// # 参数
    /// * `account_id` - 账号稳定身份。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 命中未删除绑定时返回该记录，已撤销关系仍包含在内；没有匹配时返回 `None`。
    ///
    /// # 错误
    /// 仓储查询失败时返回对应错误。
    async fn binding_for_account(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PortalBinding>> {
        self.find_one_by_field("account_id", account_id, executor).await
    }

    /// 将供应商归属条件下推到 MongoDB，不在内存中排除其他供应商。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回该供应商全部未删除绑定，包含启用和停用。
    ///
    /// # 错误
    /// 持久化访问失败时返回错误。
    async fn supplier_bindings(
        &self,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PortalBinding>> {
        self.find_many(doc! { "supplier_id": supplier_id }, executor).await
    }
}
