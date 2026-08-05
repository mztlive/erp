//! 域 D32 `supplier_fulfillment`：supplier_fulfillment_order、supplier_fulfillment_item、supplier_order_action(+_line)、supplier_order_status_history、supplier_refund_fact、supplier_refund_allocation（页面：W26）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
