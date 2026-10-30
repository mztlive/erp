//! 对账结案及风险释放使用账户全范围阻断检查，不用页面最近记录代替。

use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};

use crate::entity::supplier_deposit::{
    FundsEvidenceInbox, FundsScanCheckpoint, SupplierDepositBatch, SupplierDepositLedgerEntry,
    SupplierDepositReconciliation, SupplierDepositReservation, SupplierDepositUsage,
};
use crate::repository::SupplierDepositExt;
use crate::repository::funds_evidence::FundsEvidenceExt;
use crate::{Error, Result};

/// 平衡依据必须晚于全账户最后资金风险水位，已应用收件也不能被旧记录忽略。
/// # 参数
/// 已授权账户、已保存平衡依据和持有账号调查屏障的同一事务。
/// # 返回
/// 无范围外资金发生、晚记账、晚观察或晚核销时成功。
/// # 错误
/// 任一较新资金风险存在或查询失败时拒绝，保持原账户限制。
pub async fn ensure_account_risk_covered(
    db: &Database,
    account: &str,
    basis: &SupplierDepositReconciliation,
    executor: &mut dyn Executor,
) -> Result<()> {
    let ledger_newer = db
        .supplier_deposit_ledger()
        .exists(
            doc! {"account_id":account,"$or":[
                {"evidence.occurred_at":{"$gt":basis.covered_until.unix_secs()}},
                {"created_at":{"$gt":basis.covered_until.unix_secs()}},
                {"updated_at":{"$gt":basis.observed_at.unix_secs()}}
            ]},
            executor,
        )
        .await?;
    let evidence_newer = db
        .funds_evidence_inbox()
        .exists(
            doc! {"account_id":account,"$or":[
                {"last_observed_at":{"$gt":basis.observed_at.unix_secs()}},
                {"input.source_observed_at":{"$gt":basis.observed_at.unix_secs()}},
                {"updated_at":{"$gt":basis.observed_at.unix_secs()}},
                {"input.result.occurred_at":{"$gt":basis.covered_until.unix_secs()}}
            ]},
            executor,
        )
        .await?;
    let usage_newer = db
        .supplier_deposit_usages()
        .exists(
            doc! {
                "account_id":account,"updated_at":{"$gt":basis.observed_at.unix_secs()},
            },
            executor,
        )
        .await?;
    if ledger_newer || evidence_newer || usage_newer {
        return Err(Error::BusinessLogicError(
            "账户存在平衡核对之后的资金风险或处理，请取得更新的完整核对依据".into(),
        ));
    }
    Ok(())
}

/// 由原资金流水定位同账户的唯一使用，不允许按业务名称或金额猜测。
/// # 参数
/// 当前账户、已保存流水身份和原重核事务。
/// # 返回
/// 返回原真实使用或缺失。
/// # 错误
/// 查询失败时拒绝。
pub async fn usage_for_ledger(
    db: &Database,
    account: &str,
    ledger: &str,
    executor: &mut dyn Executor,
) -> Result<Option<SupplierDepositUsage>> {
    Ok(db
        .supplier_deposit_usages()
        .find_one(
            doc! {
                "account_id":account,"ledger_entry_id":ledger,
            },
            executor,
        )
        .await?)
}

/// 读取全部仍有剩余占用的原记录，历史占用不因页面最新一百条而失去办理入口。
/// # 参数
/// 已授权账户与当前只读执行器。
/// # 返回
/// 返回按创建时间及身份排序的最多一千项剩余占用。
/// # 错误
/// 超过上限或查询失败时拒绝，不能截断后冒充完整候选。
pub async fn outstanding_reservations(
    db: &Database,
    account: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierDepositReservation>> {
    let rows: Vec<SupplierDepositReservation> = mongo_ops::find_many(
        &db.supplier_deposit_reservations().collection(),
        doc! {"account_id":account,"deleted_at":0_i64,
        "$expr":{"$gt":["$amount",{"$add":["$consumed_amount","$released_amount"]}]}},
        FindOptions::builder().sort(doc! {"created_at":1,"id":1}).limit(1001).build(),
        executor,
    )
    .await?;
    if rows.len() > 1000 {
        return Err(Error::BusinessLogicError("账户待处理占用超过上限，请先分批核实原执行".into()));
    }
    Ok(rows)
}

/// 以账户和外部交易联合读取充值批次，外部交易号不跨账户去重。
/// # 参数
/// 已核验账户、稳定交易身份及同一重核事务。
/// # 返回
/// 返回当前账户的唯一到账批次或缺失。
/// # 错误
/// 查询失败时拒绝。
pub async fn recharge_batch_for_transaction(
    db: &Database,
    account: &str,
    transaction: &str,
    executor: &mut dyn Executor,
) -> Result<Option<SupplierDepositBatch>> {
    Ok(db
        .supplier_deposit_batches()
        .find_one(
            doc! {
                "account_id":account,"evidence.external_transaction_id":transaction,
            },
            executor,
        )
        .await?)
}

/// 读取本账户全部待人工核验收件，超过有界上限时拒绝整次结案。
/// # 参数
/// 当前账户与已授权结案事务。
/// # 返回
/// 返回最多一千条待核验原证据，不能只读取页面最近记录。
/// # 错误
/// 超限或读取失败时拒绝。
pub async fn reviewed_funds_evidence(
    db: &Database,
    account: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<FundsEvidenceInbox>> {
    let rows: Vec<FundsEvidenceInbox> = mongo_ops::find_many(
        &db.funds_evidence_inbox().collection(),
        doc! {"account_id":account,"state":"needs_review","deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"id":1}).limit(1001).build(),
        executor,
    )
    .await?;
    if rows.len() > 1000 {
        return Err(Error::BusinessLogicError("待人工核验资金超过上限，请先分批处理原资金".into()));
    }
    Ok(rows)
}

/// 读取原占用全部资金方向中的最后发生时刻，包含返款而非仅原始扣款。
/// # 参数
/// 已核验账户、可选原占用身份和当前事务。
/// # 返回
/// 返回最后扣款/返款时刻；没有资金使用时为空。
/// # 错误
/// 查询失败时拒绝。
pub async fn latest_reservation_funds_time(
    db: &Database,
    account: &str,
    reservation: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Option<Instant>> {
    let Some(reservation) = reservation else {
        return Ok(None);
    };
    let rows: Vec<SupplierDepositLedgerEntry> = mongo_ops::find_many(
        &db.supplier_deposit_ledger().collection(),
        doc! {"account_id":account,"reservation_id":reservation,"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"evidence.occurred_at":-1,"id":-1}).limit(1).build(),
        executor,
    )
    .await?;
    Ok(rows.first().map(|entry| entry.evidence.occurred_at))
}

/// 账户任一未应用资金、身份冲突或未完成核销都会阻止解除新增使用限制。
/// # 参数
/// `db`、`account`、`executor` 为同一结案事务的拥有域数据源。
/// # 返回
/// 全账户无未决资金事实时成功。
/// # 错误
/// 任一未决项、真实资金未核销或查询失败时拒绝。
pub async fn ensure_account_funds_resolved(
    db: &Database,
    account: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let pending = db
        .funds_evidence_inbox()
        .exists(doc! {"account_id":account,"state":{"$ne":"applied"}}, executor)
        .await?;
    let conflicts = db.funds_evidence_conflicts().exists(doc! {"account_id":account}, executor).await?;
    let unsettled = db
        .supplier_deposit_usages()
        .exists(
            doc! {"account_id":account,"$expr":{"$ne":[
                {"$subtract":["$amount","$returned_amount"]},
                {"$subtract":["$settled_amount","$reversed_settlement_amount"]}
            ]}},
            executor,
        )
        .await?;
    if pending || conflicts || unsettled {
        return Err(Error::BusinessLogicError(
            "账户仍有未应用资金、来源冲突或未完成核销，请先处理原记录".into(),
        ));
    }
    Ok(())
}

/// 核对资金查询已连续覆盖所选对账范围，未完成窗口不作为无扣款证明。
/// # 参数
/// `from`、`through` 为已持久化的对账范围，其他参数来自当前事务。
/// # 返回
/// 固定资金窗口完整、连续且均已完成可靠收件时成功。
/// # 错误
/// 覆盖有缺口、窗口未结束、记录超限或查询失败时拒绝。
pub async fn ensure_funds_window_covered(
    db: &Database,
    account: &str,
    from: Instant,
    through: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let windows: Vec<FundsScanCheckpoint> = mongo_ops::find_many(
        &db.funds_scan_checkpoints().collection(),
        doc! {"account_id":account,"window.since":{"$lte":through.unix_secs()},"window.through":{"$gte":from.unix_secs()},"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"window.since":1,"window.through":1}).limit(1001).build(),
        executor,
    ).await?;
    ensure_contiguous_windows(&windows, from, through)
}

/// 有界读取全部未结案差异，恢复账户不得忽略页面范围外的历史记录。
/// # 参数
/// 账户身份与当前结案事务执行器。
/// # 返回
/// 返回未结案差异；超过一千项时拒绝恢复而非截断。
/// # 错误
/// 数量超限或读取失败时拒绝。
pub async fn unresolved_reconciliations(
    db: &Database,
    account: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierDepositReconciliation>> {
    let rows: Vec<SupplierDepositReconciliation> = mongo_ops::find_many(
        &db.supplier_deposit_reconciliations().collection(),
        doc! {"account_id":account,"resolved_by":null,"deleted_at":0_i64,
        "$or":[{"unresolved_transaction_ids.0":{"$exists":true}},
            {"$expr":{"$ne":["$remote_balance","$local_balance_at_cutoff"]}}]},
        FindOptions::builder().sort(doc! {"covered_from":1,"id":1}).limit(1001).build(),
        executor,
    )
    .await?;
    if rows.len() > 1000 {
        return Err(Error::BusinessLogicError("未结案对账记录超过上限，请先分批核实原差异".into()));
    }
    Ok(rows)
}

fn ensure_contiguous_windows(windows: &[FundsScanCheckpoint], from: Instant, through: Instant) -> Result<()> {
    if windows.len() > 1000 || from >= through {
        return Err(Error::BusinessLogicError("资金查询覆盖范围无效或过大".into()));
    }
    let mut covered = from;
    for window in windows {
        if !window.finished || !window.read_complete {
            return Err(Error::BusinessLogicError("所选范围仍有未完成资金查询，不能解除风险限制".into()));
        }
        if window.window.since > covered {
            break;
        }
        if let Some(until) = window.completed_through {
            covered = covered.max(until.min(window.window.through));
        }
    }
    if covered < through {
        return Err(Error::BusinessLogicError("资金证据尚未连续覆盖所选核对范围，请先补查原资金明细".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;

    use super::*;
    use crate::entity::supplier_deposit::FundsScanWindow;

    fn checkpoint(since: i64, through: i64) -> FundsScanCheckpoint {
        FundsScanCheckpoint {
            base: BaseModel::fake(),
            account_id: "account".into(),
            source_account_reference: "external".into(),
            scan_key: format!("{since}-{through}"),
            window: FundsScanWindow {
                since: Instant::from_unix_secs(since),
                through: Instant::from_unix_secs(through),
            },
            cursor: None,
            page_number: 1,
            read_complete: true,
            finished: true,
            completed_through: Some(Instant::from_unix_secs(through)),
            pending_inbox_ids: vec![],
            last_page: None,
        }
    }
    #[test]
    fn incomplete_or_gapped_windows_cannot_prove_account_scope() {
        let (from, through) = (Instant::from_unix_secs(1), Instant::from_unix_secs(10));
        assert!(ensure_contiguous_windows(&[checkpoint(1, 5), checkpoint(5, 10)], from, through).is_ok());
        assert!(ensure_contiguous_windows(&[checkpoint(1, 5), checkpoint(6, 10)], from, through).is_err());
        let mut pending = checkpoint(1, 10);
        pending.finished = false;
        assert!(ensure_contiguous_windows(&[pending], from, through).is_err());
        assert!(ensure_contiguous_windows(&[], from, through).is_err());
    }
}
