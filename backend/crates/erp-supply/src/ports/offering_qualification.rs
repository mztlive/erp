//! 供给消费的公司SKU和供应商资格端口；实现位于组合层。
use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{SkuId, SupplierAccountId};
use persistence_core::Executor;

use crate::portal::QuoteTargetVersion;
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
    ///
    /// # 参数
    /// * `supplier_id` - 供应商账号。
    /// * `sku_id` - 公司 SKU。
    /// * `on_date` - 校验所用业务日。
    /// * `executor` - 调用方执行器，本方法不创建事务。
    ///
    /// # 返回
    /// 当前能力满足时成功。
    ///
    /// # 错误
    /// 校验未通过或沿 `executor` 读取失败时返回 `Self::Error`。
    async fn ensure_qualified(
        &self,
        supplier_id: &SupplierAccountId,
        sku_id: &SkuId,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> std::result::Result<(), Self::Error>;
}

/// 首次报价消费的当前正式商品依据；实现必须沿调用方执行器读取。
#[async_trait]
pub trait PortalQuoteQualificationPort: QualificationPort {
    /// 读取当前启用 SKU、所属启用商品、有效修订与启用基础单位的版本。
    /// # 参数
    /// `sku_id` 是供应商实际选择目标；`executor` 是当前写事务。
    /// # 返回
    /// 返回引用归属已核对的当前正式依据。
    /// # 错误
    /// 任一正式事实缺失、停用或引用归属不符时拒绝。
    async fn quote_target(
        &self,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> std::result::Result<QuoteTargetVersion, Self::Error>;

    /// 按原报价依据重验当前公司商品，不自动替换原版本。
    /// # 参数
    /// 精确 SKU、供应商已核对的版本和当前执行器。
    /// # 返回
    /// 版本与引用全部一致时成功。
    /// # 错误
    /// 目标失效或已变化时拒绝继续提交或确认。
    async fn ensure_quote_target(
        &self,
        sku_id: &SkuId,
        expected: &QuoteTargetVersion,
        executor: &mut dyn Executor,
    ) -> std::result::Result<(), Self::Error> {
        let current = self.quote_target(sku_id, executor).await?;
        expected.ensure_current(&current)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Error, Result};

    struct Marker;
    impl Executor for Marker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct TargetPort {
        pointer: usize,
        current: QuoteTargetVersion,
    }
    #[async_trait]
    impl QualificationPort for TargetPort {
        type Error = Error;
        async fn ensure_qualified(
            &self,
            _: &SupplierAccountId,
            _: &SkuId,
            _: BusinessDate,
            _: &mut dyn Executor,
        ) -> Result<()> {
            Ok(())
        }
    }
    #[async_trait]
    impl PortalQuoteQualificationPort for TargetPort {
        async fn quote_target(
            &self,
            sku_id: &SkuId,
            executor: &mut dyn Executor,
        ) -> Result<QuoteTargetVersion> {
            assert_eq!(sku_id.as_ref(), "sku");
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
            Ok(self.current.clone())
        }
    }
    fn target() -> QuoteTargetVersion {
        QuoteTargetVersion {
            sku_version: 1,
            sku_revision_id: "sr".into(),
            sku_revision_version: 1,
            product_id: "p".into(),
            product_version: 1,
            product_revision_id: "pr".into(),
            product_revision_version: 1,
            unit_id: "u".into(),
            unit_version: 1,
        }
    }

    #[tokio::test]
    async fn quote_verification_keeps_caller_executor_and_does_not_replace_supplier_basis() {
        let mut executor = Marker;
        let expected = target();
        let mut port = TargetPort { pointer: &mut executor as *mut Marker as usize, current: target() };
        port.ensure_quote_target(&SkuId::new("sku"), &expected, &mut executor).await.unwrap();
        port.current.product_version = 2;
        let error = port.ensure_quote_target(&SkuId::new("sku"), &expected, &mut executor).await.unwrap_err();
        assert!(matches!(error, Error::ConflictError(_)));
        assert_eq!(expected.product_version, 1);
    }
}
