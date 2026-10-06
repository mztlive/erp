//! 按原顺序登记四组供应链索引。

use crate::portal::indexes as portal_indexes;

mod command_receipt;
mod handover_receipt;
pub mod supplier_api;
pub mod supplier_fulfillment;
pub mod supplier_offering;
pub mod supplier_settlement;

/// 创建供应链所有原有命名索引。
///
/// # 参数
/// * `db` - 目标数据库。
/// # 返回
/// 逐集合索引登记成功。
/// # 错误
/// 数据冲突或数据库失败。
pub async fn ensure(db: &mongodb::Database) -> persistence_core::Result<()> {
    supplier_api::ensure(db).await?;
    supplier_offering::ensure(db).await?;
    supplier_fulfillment::ensure(db).await?;
    supplier_settlement::ensure(db).await?;
    command_receipt::ensure(db).await?;
    handover_receipt::ensure(db).await?;
    portal_indexes::ensure(db).await?;
    Ok(())
}
