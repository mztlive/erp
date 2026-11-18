use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::indexes::{named_index, unique_index, unique_partial_index};
use persistence_core::{Executor, Repository, Result, mongo_ops};

use super::{RefundExecution, SettlementEvidence, SettlementScan};

pub const REFUNDS: &str = "supplier_api_refunds";
pub const SCANS: &str = "supplier_api_settlement_scans";
pub const EVIDENCE: &str = "supplier_api_settlement_evidence";

pub trait AfterSalesExt {
    /// 退款写意图仓储。
    fn api_refunds(&self) -> Repository<'_, RefundExecution>;
    /// 结算检查点仓储。
    fn api_settlement_scans(&self) -> Repository<'_, SettlementScan>;
    /// 不可变来源流水仓储。
    fn api_settlement_evidence(&self) -> Repository<'_, SettlementEvidence>;
}
impl AfterSalesExt for Database {
    fn api_refunds(&self) -> Repository<'_, RefundExecution> {
        Repository::new(self, REFUNDS)
    }
    fn api_settlement_scans(&self) -> Repository<'_, SettlementScan> {
        Repository::new(self, SCANS)
    }
    fn api_settlement_evidence(&self) -> Repository<'_, SettlementEvidence> {
        Repository::new(self, EVIDENCE)
    }
}

/// 查询最多20个到期退款，未支持和最终来源均不自动重试。
/// 有界到期队列。
/// 数据库失败时向上传播错误。
pub async fn refunds_due(db: &Database, executor: &mut dyn Executor) -> Result<Vec<RefundExecution>> {
    mongo_ops::find_many(
        &db.collection(REFUNDS),
        doc! {"next_run_at":{"$type":"number","$lte":Instant::now().unix_secs()},"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"next_run_at":1,"id":1}).limit(20).build(),
        executor,
    )
    .await
}

/// 读取原订单最多1000笔退款；超限由调用方拒绝新申请。
/// 包含未决及终态，不过滤累计占用证据。
/// 数据库失败时向上传播错误。
/// 数据库、订单及原事务执行器。
pub async fn refunds_for_order(
    db: &Database,
    order: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<RefundExecution>> {
    mongo_ops::find_many(
        &db.collection(REFUNDS),
        doc! {"request.order.merchant_order_no":order},
        FindOptions::builder().limit(1001).build(),
        executor,
    )
    .await
}

/// 读取一个连接最多50项未关联流水，供已有结算来源录入引用。
/// 只返回当前连接来源，不等同于已过账。
/// 数据库失败时向上传播错误。
/// 数据库、已授权连接和执行器。
pub async fn pending_settlements(
    db: &Database,
    connection: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SettlementEvidence>> {
    mongo_ops::find_many(
        &db.collection(EVIDENCE),
        doc! {"connection_id":connection,"source_evidence_id":null},
        FindOptions::builder().sort(doc! {"id":1}).limit(50).build(),
        executor,
    )
    .await
}

/// 登记新增集合唯一及有界查询索引，不变更历史集合。
/// 索引创建完成。
/// 索引冲突或数据库失败。
pub async fn ensure_indexes(db: &Database) -> Result<()> {
    db.collection::<RefundExecution>(REFUNDS)
        .create_indexes(vec![
            unique_index("uniq_api_refund_id", doc! {"id":1}),
            unique_partial_index(
                "uniq_api_refund_receipt",
                doc! {"connection_id":1,"receipt.value.external_refund_no":1},
                doc! {"receipt.value.external_refund_no":{"$type":"string"}},
            ),
            named_index("idx_api_refund_due", doc! {"next_run_at":1,"id":1}),
            named_index("idx_api_refund_order", doc! {"request.order.merchant_order_no":1}),
        ])
        .await?;
    db.collection::<SettlementScan>(SCANS)
        .create_indexes(vec![unique_index("uniq_api_settlement_scan_id", doc! {"id":1})])
        .await?;
    db.collection::<SettlementEvidence>(EVIDENCE)
        .create_indexes(vec![
            unique_index("uniq_api_settlement_evidence_id", doc! {"id":1}),
            unique_index(
                "uniq_api_settlement_external",
                doc! {"connection_id":1,"evidence.value.external_entry_id":1},
            ),
            named_index("idx_api_settlement_pending", doc! {"connection_id":1,"source_evidence_id":1,"id":1}),
        ])
        .await?;
    Ok(())
}
