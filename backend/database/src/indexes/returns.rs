//! 域 D21 `returns`：sales_return_case、sales_return_line、purchase_return_order、purchase_return_line、customer_refund、supplier_refund、receipt_reversal、payment_reversal（页面：W05、W09、W11、W12）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
