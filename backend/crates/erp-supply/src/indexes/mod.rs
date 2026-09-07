//! 按原顺序登记四组供应链索引。

pub mod supplier_api;
pub mod supplier_fulfillment;
pub mod supplier_offering;
pub mod supplier_settlement;

/// 创建供应链所有原有命名索引。
pub async fn ensure(db: &mongodb::Database) -> persistence_core::Result<()> {
    supplier_api::ensure(db).await?;
    supplier_offering::ensure(db).await?;
    supplier_fulfillment::ensure(db).await?;
    supplier_settlement::ensure(db).await?;
    Ok(())
}
