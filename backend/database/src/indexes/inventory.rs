//! 域 D17 `inventory`：stock_movement、stock_balance、stock_reservation(+_entry)、stock_adjustment(+_line)（页面：W10）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
