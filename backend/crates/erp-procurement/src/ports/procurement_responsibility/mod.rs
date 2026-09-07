//! 采购责任解析和规则维护实际消费的外域事实合同。

use crate::entity::facts::IdentityOwnerFact;
use crate::entity::procurement_responsibility::ProcurementCatalogBundle;
use async_trait::async_trait;
use erp_core::ids::{ProductCategoryId, SkuId};
use persistence_core::Executor;

/// 目录和身份事实只读端口；实现不得授权，也不得替换事务执行器。
#[async_trait]
pub trait ProcurementResponsibilityFactsPort: Send + Sync {
    /// 批量加载目录图；完整性和分类环由采购领域检查。
    async fn load_catalog(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<ProcurementCatalogBundle>;
    /// 批量加载指定负责人；保留缺失值和账号资格事实供采购判断。
    async fn load_owners(
        &self,
        owner_ids: &[String],
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<Vec<IdentityOwnerFact>>;
    /// 读取指定负责人，不按姓名或当前规则推断身份。
    async fn load_owner(
        &self,
        owner_id: &str,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<Option<IdentityOwnerFact>>;
    /// 在原前置校验或调用方事务中检查 SKU 引用存在性。
    async fn sku_exists(&self, id: &SkuId, executor: &mut dyn Executor) -> persistence_core::Result<bool>;
    /// 在原前置校验或调用方事务中检查分类引用存在性。
    async fn category_exists(
        &self,
        id: &ProductCategoryId,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<bool>;
}
