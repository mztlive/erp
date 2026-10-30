//! 原采购实际收货和原发货实际验收的窄读取，禁止物流观察代替实收依据。

use erp_core::ids::PurchaseOrderId;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};

use crate::entity::fulfillment::{CustomerAcceptance, CustomerAcceptanceLine, Delivery, PurchaseReceipt};
use crate::repository::FulfillmentExt;
use crate::{Error, Result};

/// 读取指定原发货单，不放宽为同销售商品的其他交付。
/// # 参数
/// 已核验执行持有的原发货身份及调用方事务。
/// # 返回
/// 对应未删除发货记录；缺失由证据规则阻断。
/// # 错误
/// 输入超过一千项或数据库失败时拒绝。
pub async fn deliveries(db: &Database, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<Delivery>> {
    bounded(ids.len())?;
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Ok(db.deliveries().find_many(doc! {"id":{"$in":ids},"deleted_at":0_i64}, executor).await?)
}

/// 读取原验收分配实际引用的验收行。
/// # 参数
/// 分配中已经保存的验收行身份及同一事务。
/// # 返回
/// 原验收行，不按销售 SKU 另找候选。
/// # 错误
/// 范围超限或数据库失败时拒绝。
pub async fn acceptance_lines(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<CustomerAcceptanceLine>> {
    bounded(ids.len())?;
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Ok(db.customer_acceptance_lines().find_many(doc! {"id":{"$in":ids},"deleted_at":0_i64}, executor).await?)
}

/// 读取原验收行归属的正式验收表头。
/// # 参数
/// 反查得到的验收身份及同一事务。
/// # 返回
/// 原验收表头及凭证引用，草稿不被解释为已签收。
/// # 错误
/// 范围超限或数据库失败时拒绝。
pub async fn acceptances(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<CustomerAcceptance>> {
    bounded(ids.len())?;
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Ok(db.customer_acceptances().find_many(doc! {"id":{"$in":ids},"deleted_at":0_i64}, executor).await?)
}

/// 读取本采购全部仍有效的实际入库记录，冲正记录不提供可用收货数量。
/// # 参数
/// 原采购身份及同一事务。
/// # 返回
/// 最多一千项已过账入库，超限明确阻断。
/// # 错误
/// 范围超限或数据库失败时拒绝。
pub async fn purchase_receipts(
    db: &Database,
    purchase: &PurchaseOrderId,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseReceipt>> {
    let rows: Vec<PurchaseReceipt> = mongo_ops::find_many(
        &db.purchase_receipts().collection(),
        doc! {"purchase_order_id":purchase.as_ref(),"status":"POSTED","deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"id":1}).limit(1001).build(),
        executor,
    )
    .await?;
    bounded(rows.len())?;
    Ok(rows)
}

fn bounded(count: usize) -> Result<()> {
    if count > 1000 {
        return Err(Error::BusinessLogicError("原实际收货证据超过本次核验范围，请分批核对原单".into()));
    }
    Ok(())
}
