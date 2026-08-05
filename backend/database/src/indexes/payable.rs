//! 域 D19 `payable`：payable_account、payable_entry、payable_entry_offset、supplier_payment、payment_allocation、purchase_invoice_allocation（页面：W12）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
