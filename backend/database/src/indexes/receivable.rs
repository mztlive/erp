//! 域 D18 `receivable`：receivable_account、receivable_entry、receivable_funds_review、receivable_entry_offset、customer_receipt、receipt_allocation、invoice、sales_invoice_allocation（页面：W11、W13）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
