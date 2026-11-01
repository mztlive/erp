//! 留抵余额与抵扣记录的拥有仓储。
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::payable::supplier_credit::{SupplierCredit, SupplierCreditUse};
pub const SUPPLIER_CREDITS: &str = "supplier_credits";
pub const SUPPLIER_CREDIT_USES: &str = "supplier_credit_uses";

/// 获取供应商留抵仓储。
/// # 参数
/// * `db` - 数据库。
/// # 返回
/// 当前数据库的仓储。
/// # 错误
/// 无。
pub fn supplier_credits(db: &Database) -> Repository<'_, SupplierCredit> {
    Repository::new(db, SUPPLIER_CREDITS)
}
/// 获取留抵使用事实仓储。
/// # 参数
/// * `db` - 数据库。
/// # 返回
/// 当前数据库的仓储。
/// # 错误
/// 无。
pub fn supplier_credit_uses(db: &Database) -> Repository<'_, SupplierCreditUse> {
    Repository::new(db, SUPPLIER_CREDIT_USES)
}
/// 查询付款申请的留抵使用记录。
/// # 参数
/// * `db` - 数据库。
/// * `id` - 已授权付款申请身份。
/// * `executor` - 原查询事务。
/// # 返回
/// 按创建时间排序的使用记录。
/// # 错误
/// 仓储失败时返回错误。
pub async fn uses_by_request(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierCreditUse>> {
    supplier_credit_uses(db)
        .find_many_sorted(doc! {"payment_request_id": id}, doc! {"created_at": 1, "id": 1}, executor)
        .await
}

/// 查询当前主体可用余额。
/// # 参数
/// * `db` / `executor` - 数据源和执行器。
/// * `supplier` / `payer` - 供应商和付款主体。
/// # 返回
/// 按创建时间排序的余额。
/// # 错误
/// 查询失败时返回错误。
pub async fn available_credits(
    db: &Database,
    supplier: &str,
    payer: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierCredit>> {
    supplier_credits(db).find_many_sorted(doc! {"supplier_id": supplier, "payer_party_id": payer, "$expr": {"$gt": ["$amount", "$used_amount"]}}, doc! {"created_at": 1, "id": 1}, executor).await
}

/// 查询原付款是否已产生留抵关系。
/// # 参数
/// * `db` / `executor` - 原事务。
/// * `payment_id` - 原付款。
/// # 返回
/// 是否存在留抵记录，包括已使用余额。
/// # 错误
/// 查询失败时返回错误。
pub async fn has_payment_credit(
    db: &Database,
    payment_id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    supplier_credits(db).exists(doc! {"original_payment_id": payment_id}, executor).await
}
