//! 采购责任解析和规则维护实际消费的外域事实合同。

use async_trait::async_trait;
use erp_core::ids::{ProductCategoryId, SkuId};
use erp_identity::AccountSummary;
use persistence_core::Executor;

use crate::entity::procurement_responsibility::ProcurementCatalogBundle;

/// 目录和身份事实只读端口；实现不得授权，也不得替换事务执行器。
#[async_trait]
pub trait ProcurementResponsibilityFactsPort: Send + Sync {
    /// 批量加载目录图；完整性和分类环由采购领域检查。
    ///
    /// 返回 SKU、商品、修订与可达分类组成的目录包。目录读取失败时返回仓储错误。
    ///
    /// 使用调用方执行器；实现不得替换。
    async fn load_catalog(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<ProcurementCatalogBundle>;
    /// 批量加载指定负责人；保留缺失值并返回身份域账号摘要。
    ///
    /// 返回读取到的负责人事实，不补造缺失账号；后台资格由摘要方法统一判断。账号读取失败时返回仓储错误。
    ///
    /// 使用调用方执行器；实现不得替换。
    async fn load_owners(
        &self,
        owner_ids: &[String],
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<Vec<AccountSummary>>;
    /// 读取指定负责人，不按姓名或当前规则推断身份。
    ///
    /// 找到账号时返回其资格事实；账号不存在时返回 `None`。账号读取失败时返回仓储错误。
    ///
    /// 使用调用方执行器；实现不得替换。
    async fn load_owner(
        &self,
        owner_id: &str,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<Option<AccountSummary>>;
    /// 在原前置校验或调用方事务中检查 SKU 引用是否存在。
    ///
    /// 引用存在时为 `true`，否则为 `false`。读取失败时返回仓储错误。
    ///
    /// 使用调用方执行器；实现不得替换。
    async fn sku_exists(&self, id: &SkuId, executor: &mut dyn Executor) -> persistence_core::Result<bool>;
    /// 在原前置校验或调用方事务中检查分类引用是否存在。
    ///
    /// 引用存在时为 `true`，否则为 `false`。读取失败时返回仓储错误。
    ///
    /// 使用调用方执行器；实现不得替换。
    async fn category_exists(
        &self,
        id: &ProductCategoryId,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<bool>;
}
