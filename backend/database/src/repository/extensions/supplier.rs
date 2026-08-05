//! 域 D09 `supplier`：supplier_account、supplier_commercial_profile_revision、supplier_capability、supplier_qualification、supplier_rating_revision 等（页面：W14）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D09 仓储访问器（P2 填充）。
pub trait SupplierExt: Sized {}

impl SupplierExt for mongodb::Database {}
