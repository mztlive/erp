//! 域 D25 `supplier_api`：supplier_api_connection、supplier_api_capability（页面：W20）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D25 仓储访问器（P2 填充）。
pub trait SupplierApiExt: Sized {}

impl SupplierApiExt for mongodb::Database {}
