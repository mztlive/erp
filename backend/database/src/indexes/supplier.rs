//! 域 D09 `supplier`：supplier_account、supplier_commercial_profile_revision、supplier_capability、supplier_qualification、supplier_rating_revision 等（页面：W14）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
