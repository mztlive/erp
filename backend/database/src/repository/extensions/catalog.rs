//! 域 D10 `catalog`：product_category、product_brand、unit_of_measure、sku_attribute、product(+_revision)、sku(+_revision)、voucher_category_profile_revision 等（页面：W14）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D10 仓储访问器（P2 填充）。
pub trait CatalogExt: Sized {}

impl CatalogExt for mongodb::Database {}
