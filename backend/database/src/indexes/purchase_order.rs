//! 域 D15 `purchase_order`：purchase_order、purchase_order_submission、purchase_order_revision、purchase_line_sales_allocation、purchase_change_order 等（页面：W08）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
