//! 域 D23 `mall_sync`：mall_sales_sync_job、mall_sales_sync_cursor、mall_sales_order_snapshot、mall_sales_reconciliation_job(+_item)、master_mapping_task（页面：W17）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
