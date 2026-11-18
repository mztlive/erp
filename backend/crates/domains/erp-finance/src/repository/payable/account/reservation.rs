//! 应付来源上的共享付款申请额度并发保护。

use chrono::Utc;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::money::Amount;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, Repository, Result};

use super::write::{PayableAccountWriteExt, amount_bson};
use crate::entity::payable::PayableAccount;

#[allow(async_fn_in_trait)]
pub trait PayableAccountReservationExt {
    /// 提交付款申请时用来源应付的条件写占用额度，确保已有申请占用与实际已付不会挤占同一余额。
    /// 结果：成功占用返回 true，余额不足或非正金额返回 false。
    /// 传播数据库写入失败。
    ///
    /// 输入约定：
    /// - `id`：应付身份。
    /// - `amount`：本次正数占用。
    /// - `executor`：包含申请写入的同一事务。
    async fn reserve_payment_request(
        &self,
        id: &str,
        amount: &Amount,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 关闭、撤回或驳回申请时，在同一来源写锁下扣减尚未付款的占用，不能释放超过现有占用的金额。
    /// 结果：成功释放返回 true，占用不足或非正金额返回 false。
    /// 传播数据库写入失败。
    ///
    /// 输入约定：
    /// - `id`：应付身份。
    /// - `amount`：本次正数释放。
    /// - `executor`：包含申请版本修改的同一事务。
    async fn release_payment_request(
        &self,
        id: &str,
        amount: &Amount,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 实际付款核销前先从来源应付扣减原申请占用，随后在同一事务入账，避免重复消费同一额度。
    /// 结果：成功扣减返回 true，占用不足或非正金额返回 false。
    /// 传播数据库写入失败。
    ///
    /// 输入约定：
    /// - `id`：应付身份。
    /// - `amount`：本次付款占用。
    /// - `actor`：付款人。
    /// - `executor`：付款、核销、申请和回执共同事务。
    async fn consume_payment_request(
        &self,
        id: &str,
        amount: &Amount,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 付款冲正恢复应付后，才为未关闭的原申请恢复占用；与冲正后的应付写入使用同一事务。
    /// 结果：成功恢复返回 true，当前应付不足或非正金额返回 false。
    /// 传播数据库写入失败。
    ///
    /// 输入约定：
    /// - `id`：应付身份。
    /// - `amount`：本次恢复金额。
    /// - `actor`：冲正人。
    /// - `executor`：已还原有效应付的同一事务。
    async fn restore_payment_request(
        &self,
        id: &str,
        amount: &Amount,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
}

impl PayableAccountReservationExt for Repository<'_, PayableAccount> {
    async fn reserve_payment_request(
        &self,
        id: &str,
        amount: &Amount,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        change(self, id, amount, actor, true, executor).await
    }

    async fn release_payment_request(
        &self,
        id: &str,
        amount: &Amount,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        change(self, id, amount, actor, false, executor).await
    }

    async fn consume_payment_request(
        &self,
        id: &str,
        amount: &Amount,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.release_payment_request(id, amount, actor, executor).await
    }

    async fn restore_payment_request(
        &self,
        id: &str,
        amount: &Amount,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.reserve_payment_request(id, amount, actor, executor).await
    }
}

async fn change(
    repository: &Repository<'_, PayableAccount>,
    id: &str,
    amount: &Amount,
    actor: &str,
    increase: bool,
    executor: &mut dyn Executor,
) -> Result<bool> {
    if *amount <= Amount::zero() {
        return Ok(false);
    }

    let amount = amount_bson(amount)?;
    let filter = guard(id, &amount, increase);
    let delta = if increase {
        doc! {"$add": ["$payment_request_reserved_total", &amount]}
    } else {
        doc! {"$subtract": ["$payment_request_reserved_total", &amount]}
    };

    repository
        .conditional_update(
            filter,
            vec![doc! {"$set": {
                "payment_request_reserved_total": delta, "updated_by": actor,
                "version": {"$add": ["$version", 1]}, "updated_at": Utc::now().timestamp(),
            }}],
            executor,
        )
        .await
}

fn guard(id: &str, amount: &Bson, increase: bool) -> Document {
    let expression = if increase {
        doc! {"$and": [
            {"$gte": ["$open_total", {"$toDecimal": "0"}]},
            {"$gte": ["$payment_request_reserved_total", {"$toDecimal": "0"}]},
            {"$lte": [{"$add": ["$payment_request_reserved_total", amount]}, "$open_total"]},
        ]}
    } else {
        doc! {"$gte": ["$payment_request_reserved_total", amount]}
    };
    doc! {"id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": expression}
}
