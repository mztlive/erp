//! 自动余额执行候选仅来自已批准的原采购付款申请。

use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};

use crate::entity::payable::PurchasePaymentRequest;
use crate::repository::PurchasePaymentRequestExt;
use crate::{Error, Result};

/// 装载同资金账号、本采购生效修订的有界已批准余额申请候选。
/// # 参数
/// * `db` / `account` - 财务数据库和服务器绑定资金账户。
/// * `purchase` / `revision` / `executor` - 原采购、当前修订及共同事务。
/// # 返回
/// 返回最多一百个候选，额度与唯一性由原申请实体核验。
/// # 错误
/// 超限或数据库读取失败时拒绝，不按列表前一百项猜测唯一性。
pub async fn automatic_payment_requests(
    db: &Database,
    account: &str,
    purchase: &str,
    revision: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchasePaymentRequest>> {
    let filter = doc! {"data.execution_method.type": "supplier_deposit", "data.execution_method.account_id": account, "deposit_account.account_id": account,
    "status": {"$in": ["approved", "partially_paid"]}, "lines": {"$elemMatch": {"purchase_order_id": purchase, "purchase_revision_id": revision}}, "deleted_at": 0_i64};
    let rows: Vec<PurchasePaymentRequest> = mongo_ops::find_many(
        &db.purchase_payment_requests().collection(),
        filter,
        FindOptions::builder().sort(doc! {"created_at": 1, "id": 1}).limit(101).build(),
        executor,
    )
    .await?;
    if rows.len() > 100 {
        return Err(Error::BusinessLogicError("本采购已批准余额申请超过上限，请指定出纳核查".into()));
    }
    Ok(rows)
}
