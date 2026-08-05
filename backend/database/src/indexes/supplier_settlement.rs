//! 域 D33 `supplier_settlement`：supplier_settlement_statement、supplier_settlement_item、supplier_settlement_difference（页面：W27）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
