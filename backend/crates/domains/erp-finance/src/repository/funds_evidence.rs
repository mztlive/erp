//! 资金证据和固定窗口检查点的拥有仓储及有界查询。

use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, mongo_ops};

use crate::entity::supplier_deposit::{
    FundsBalanceObservation, FundsEvidenceConflict, FundsEvidenceInbox, FundsScanCheckpoint, FundsScanLease,
};
use crate::{Error, Result};

pub trait FundsEvidenceExt {
    const FUNDS_SCAN_LEASES: &'static str = "supplier_funds_scan_leases";
    const FUNDS_EVIDENCE_INBOX: &'static str = "supplier_funds_evidence_inbox";
    const FUNDS_EVIDENCE_CONFLICTS: &'static str = "supplier_funds_evidence_conflicts";
    const FUNDS_SCAN_CHECKPOINTS: &'static str = "supplier_funds_scan_checkpoints";
    const FUNDS_BALANCE_OBSERVATIONS: &'static str = "supplier_funds_balance_observations";

    /// 获取账号调查租约，不删除原金融证据。
    /// 返回账号级唯一租约仓储。
    fn funds_scan_leases(&self) -> Repository<'_, FundsScanLease>;
    /// 为不可覆盖来源身份的资金证据读写提供绑定当前数据库的仓储，由调用方控制执行器。
    /// 返回拥有域仓储。
    fn funds_evidence_inbox(&self) -> Repository<'_, FundsEvidenceInbox>;
    /// 为资金身份冲突的独立证据读写提供绑定当前数据库的仓储，由调用方控制执行器。
    /// 返回独立冲突证据仓储。
    fn funds_evidence_conflicts(&self) -> Repository<'_, FundsEvidenceConflict>;
    /// 为固定查询窗口检查点读写提供绑定当前数据库的仓储，由调用方控制执行器。
    /// 返回可恢复检查点仓储。
    fn funds_scan_checkpoints(&self) -> Repository<'_, FundsScanCheckpoint>;
    /// 为只追加的余额观察读写提供绑定当前数据库的仓储，由调用方控制执行器。
    /// 返回余额观察仓储。
    fn funds_balance_observations(&self) -> Repository<'_, FundsBalanceObservation>;
}

impl FundsEvidenceExt for Database {
    fn funds_scan_leases(&self) -> Repository<'_, FundsScanLease> {
        Repository::new(self, Self::FUNDS_SCAN_LEASES)
    }
    fn funds_evidence_inbox(&self) -> Repository<'_, FundsEvidenceInbox> {
        Repository::new(self, Self::FUNDS_EVIDENCE_INBOX)
    }
    fn funds_evidence_conflicts(&self) -> Repository<'_, FundsEvidenceConflict> {
        Repository::new(self, Self::FUNDS_EVIDENCE_CONFLICTS)
    }
    fn funds_scan_checkpoints(&self) -> Repository<'_, FundsScanCheckpoint> {
        Repository::new(self, Self::FUNDS_SCAN_CHECKPOINTS)
    }
    fn funds_balance_observations(&self) -> Repository<'_, FundsBalanceObservation> {
        Repository::new(self, Self::FUNDS_BALANCE_OBSERVATIONS)
    }
}

/// 优先恢复本账户未完成窗口，否则返回最新已完成窗口；不猜测未发生扣款。
/// 返回唯一未完成检查点或最新完成检查点。
/// 数据库或反序列化失败时拒绝。
///
/// 输入约定：
/// - `db` / `account` / `executor`：已授权账户及查询执行器。
pub async fn funds_scan_checkpoint(
    db: &Database,
    account: &str,
    executor: &mut dyn Executor,
) -> Result<Option<FundsScanCheckpoint>> {
    let repository = db.funds_scan_checkpoints();
    let active: Vec<FundsScanCheckpoint> = mongo_ops::find_many(
        &repository.collection(),
        doc! {"account_id":account,"finished":false,"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"created_at":-1,"id":-1}).limit(1).build(),
        executor,
    )
    .await?;
    if let Some(checkpoint) = active.into_iter().next() {
        return Ok(Some(checkpoint));
    }

    let completed: Vec<FundsScanCheckpoint> = mongo_ops::find_many(
        &repository.collection(),
        doc! {"account_id":account,"finished":true,"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"completed_through":-1,"created_at":-1,"id":-1}).limit(1).build(),
        executor,
    )
    .await?;
    Ok(completed.into_iter().next())
}

/// 有界恢复未应用的稳定资金交易，待人工核对线索保留但不作为自动应用候选。
/// 返回按原观察时间和身份排序的待应用证据。
/// 上限非法或读取失败时拒绝。
///
/// 输入约定：
/// - `db` / `account` / `limit` / `executor`：当前账户和1至1000条上限。
pub async fn pending_funds_evidence(
    db: &Database,
    account: &str,
    limit: u32,
    executor: &mut dyn Executor,
) -> Result<Vec<FundsEvidenceInbox>> {
    if limit == 0 || limit > 1000 {
        return Err(Error::ValidationError("资金证据读取上限须为1至1000".into()));
    }
    Ok(mongo_ops::find_many(
        &db.funds_evidence_inbox().collection(),
        doc! {"account_id":account,"state":"received","deleted_at":0_i64},
        FindOptions::builder()
            .sort(doc! {"input.source_observed_at":1,"id":1})
            .limit(i64::from(limit))
            .build(),
        executor,
    )
    .await?)
}

/// 按账户和真实交易稳定身份读取证据。
/// 返回唯一交易证据或缺失。
/// 查询失败时拒绝。
///
/// 输入约定：
/// - `db` / `account` / `transaction` / `executor`：原交易归属。
pub async fn funds_evidence_by_transaction(
    db: &Database,
    account: &str,
    transaction: &str,
    executor: &mut dyn Executor,
) -> Result<Option<FundsEvidenceInbox>> {
    Ok(db
        .funds_evidence_inbox()
        .find_one(
            doc! {"account_id":account,"source_key.kind":"transaction","source_key.value":transaction},
            executor,
        )
        .await?)
}

/// 按精确来源订单读取资金线索，不把订单状态解释为付款。
/// 返回最多1000条订单关联证据。
/// 超限或查询失败时拒绝。
///
/// 输入约定：
/// - `db` / `account` / `order` / `executor`：已核验来源订单归属。
pub async fn funds_evidence_by_order(
    db: &Database,
    account: &str,
    order: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<FundsEvidenceInbox>> {
    let rows: Vec<FundsEvidenceInbox> = mongo_ops::find_many(
        &db.funds_evidence_inbox().collection(),
        doc! {"account_id":account,"input.result.external_order_id":order,"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"input.source_observed_at":1,"id":1}).limit(1001).build(),
        executor,
    )
    .await?;
    if rows.len() > 1000 {
        return Err(Error::BusinessLogicError("订单资金线索超过上限，请按时间范围核查".into()));
    }
    Ok(rows)
}

pub(crate) async fn page_evidence(
    db: &Database,
    account: &str,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<FundsEvidenceInbox>> {
    if ids.len() > 1000 {
        return Err(Error::ValidationError("资金证据页超过上限".into()));
    }
    Ok(db.funds_evidence_inbox().find_many(doc! {"account_id":account,"id":{"$in":ids}}, executor).await?)
}
