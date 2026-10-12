//! 采购发件记录的拥有仓储；主键唯一索引保证同命令只发起一次。
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::purchase_email::PurchaseEmail;

/// 发件记录集合名。
pub const PURCHASE_EMAILS: &str = "purchase_emails";

/// 获取发件记录仓储；只按确定性主键读取，不扫描集合。
/// # 参数
/// `db` 为业务数据库。
/// # 返回
/// 返回使用调用方 Executor 的仓储。
/// # 错误
/// 无。
pub fn purchase_emails(db: &Database) -> Repository<'_, PurchaseEmail> {
    Repository::new(db, PURCHASE_EMAILS)
}

/// 查询本采购单最近 20 次发送，使用采购单与创建时间索引。
/// # 参数
/// `db` 为数据库，`id` 为已授权采购单，`executor` 为读取执行器。
/// # 返回
/// 最新在前的发件记录。
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn recent(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<Vec<PurchaseEmail>> {
    mongo_ops::find_many(
        &db.collection::<PurchaseEmail>(PURCHASE_EMAILS),
        doc! { "purchase_order_id": id },
        FindOptions::builder().sort(doc! { "created_at": -1, "id": -1 }).limit(20).build(),
        executor,
    )
    .await
}
