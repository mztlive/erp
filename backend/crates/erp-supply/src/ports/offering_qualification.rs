//! 供给消费的公司SKU和供应商资格端口；实现位于组合层。
use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{SkuId, SupplierAccountId};
use persistence_core::Executor;
/// 保留原catalog/supplier错误类别的资格读取合同。
#[async_trait]
pub trait QualificationPort: Send + Sync {
    /// 实际组合错误；本域准备的错误保持原类别转换。
    type Error: From<crate::Error>
        + From<erp_core::Error>
        + From<persistence_core::Error>
        + From<validator::ValidationErrors>
        + Send;
    /// 在调用方执行器上校验SKU及供应商当前能力；不创建事务。
    async fn ensure_qualified(
        &self,
        supplier_id: &SupplierAccountId,
        sku_id: &SkuId,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> std::result::Result<(), Self::Error>;
}
