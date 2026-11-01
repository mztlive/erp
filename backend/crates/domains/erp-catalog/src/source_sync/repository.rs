//! 商品来源基线和确认回执由商品域拥有。
use mongodb::Database;
use persistence_core::Repository;

use super::{ProductSource, SourceResolutionReceipt};

/// 来源资料集合访问器。
pub trait ProductSourceExt {
    const PRODUCT_SOURCES: &'static str = "product_sources";
    const SOURCE_RESOLUTION_RECEIPTS: &'static str = "product_source_resolution_receipts";
    /// 获取每件商品唯一来源的仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 带标准乐观锁的来源仓储。
    /// # 错误
    /// 无。
    fn product_sources(&self) -> Repository<'_, ProductSource>;
    /// 获取稳定操作回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 不依赖展示审计的回执仓储。
    /// # 错误
    /// 无。
    fn source_resolution_receipts(&self) -> Repository<'_, SourceResolutionReceipt>;
}
impl ProductSourceExt for Database {
    fn product_sources(&self) -> Repository<'_, ProductSource> {
        Repository::new(self, Self::PRODUCT_SOURCES)
    }
    fn source_resolution_receipts(&self) -> Repository<'_, SourceResolutionReceipt> {
        Repository::new(self, Self::SOURCE_RESOLUTION_RECEIPTS)
    }
}
