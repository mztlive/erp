//! 域 D13 `sales_order`：sales_order(+_line)、sales_order_working_copy、sales_order_submission、sales_order_revision、goods_service_line_revision、voucher_line_revision（页面：W05）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
