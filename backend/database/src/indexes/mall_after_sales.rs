//! 域 D30 `mall_after_sales`：mall_after_sales_request(+_line)、mall_refund(+_line)、mall_refund_allocation、mall_balance_restoration(+_allocation)（页面：W25）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
