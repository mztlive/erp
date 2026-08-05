//! 域 D16 `fulfillment`：purchase_receipt(+_line)、delivery(+_line)、electronic_delivery、service_fulfillment、customer_acceptance(+_line)、acceptance_fulfillment_allocation（页面：W06、W09）。P0 预声明空 ensure；P2 落地数据模型 §6 必需索引（唯一约束用唯一索引）。

/// 创建本域集合的幂等命名索引（P2 填充）。
pub(crate) async fn ensure(_db: &mongodb::Database) -> crate::Result<()> {
    Ok(())
}
