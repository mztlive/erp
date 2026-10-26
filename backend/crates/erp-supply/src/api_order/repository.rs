use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::indexes::{named_index, unique_index};
use persistence_core::{Executor, Repository, Result, mongo_ops};

use super::{ApiOrderExecution, ApiOrderSetupReceipt};

const EXECUTIONS: &str = "supplier_api_order_executions";
const SETUP_RECEIPTS: &str = "supplier_api_order_setup_receipts";

/// 接口执行意图的领域仓储访问器。
pub trait ApiOrderExt {
    /// 返回当前数据库的执行意图仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 使用调用方执行器的仓储。
    /// # 错误
    /// 不返回错误。
    fn supplier_api_order_executions(&self) -> Repository<'_, ApiOrderExecution>;
    /// 取得不可删除的配送补充命令回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库回执仓储。
    /// # 错误
    /// 不返回错误。
    fn supplier_api_order_setup_receipts(&self) -> Repository<'_, ApiOrderSetupReceipt>;
}
impl ApiOrderExt for Database {
    fn supplier_api_order_executions(&self) -> Repository<'_, ApiOrderExecution> {
        Repository::new(self, EXECUTIONS)
    }
    fn supplier_api_order_setup_receipts(&self) -> Repository<'_, ApiOrderSetupReceipt> {
        Repository::new(self, SETUP_RECEIPTS)
    }
}

/// 取得同一采购尚未由新版本明确退役的执行，避免历史意图重复阻断变更。
/// # 参数
/// `purchase` 是已授权采购身份，读取沿用变更根事务。
/// # 返回
/// 当前及历史仍有效的执行；已记载替代版本的记录不再参与变更核验。
/// # 错误
/// 数据库读取失败时返回原仓储错误。
pub async fn active_for_purchase(
    db: &Database,
    purchase: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<ApiOrderExecution>> {
    db.supplier_api_order_executions()
        .find_many(doc! {"purchase_order_id":purchase,"superseded_by_purchase_revision_id":null}, executor)
        .await
}

/// 有界取得到期执行记录，观察状态由独立跟踪器处理。
/// # 参数
/// `executor` 为调用方执行器。
/// # 返回
/// 最多二十条按时间、身份稳定排序的记录。
/// # 错误
/// 数据库读取失败时返回错误。
pub async fn due(db: &Database, executor: &mut dyn Executor) -> Result<Vec<ApiOrderExecution>> {
    mongo_ops::find_many(
        &db.collection::<ApiOrderExecution>(EXECUTIONS),
        doc! { "stage": { "$nin": ["OBSERVE", "STOPPED"] }, "next_run_at": { "$lte": Instant::now().unix_secs() } },
        FindOptions::builder().sort(doc! {"next_run_at":1,"id":1}).limit(20).build(), executor,
    ).await
}

/// 按同连接、同地址指纹复用已明确配置的坐标，不猜测地理位置。
/// # 参数
/// `fingerprint` 为应用密钥生成的地址指纹，不包含收件人明文。
/// # 返回
/// 最近有请求密文的候选记录。
/// # 错误
/// 数据库失败时返回错误。
pub async fn reusable_address(
    db: &Database,
    connection: &str,
    fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<ApiOrderExecution>> {
    mongo_ops::find_many(&db.collection::<ApiOrderExecution>(EXECUTIONS),
        doc! {"connection_id":connection,"address_fingerprint":fingerprint,"request_encrypted":{"$type":"string"}},
        FindOptions::builder().sort(doc! {"updated_at":-1}).limit(5).build(),executor).await
}

/// 读取一张正式采购下的执行关联，可进一步限定冻结采购修订。
/// # 参数
/// `purchase` 为已核验采购身份，`revision` 为精确版本或全部历史。
/// # 返回
/// 仅该采购范围的执行事实。
/// # 错误
/// 数据库查询失败时返回错误。
pub async fn for_purchase(
    db: &Database,
    purchase: &str,
    revision: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<ApiOrderExecution>> {
    let mut filter = doc! {"purchase_order_id":purchase};
    if let Some(revision) = revision {
        filter.insert("purchase_order_revision_id", revision);
    }
    db.supplier_api_order_executions().find_many(filter, executor).await
}

/// 增量登记执行身份、调度及地址复用索引。
/// # 参数
/// `db` 为当前应用数据库。
/// # 返回
/// 索引全部建立成功。
/// # 错误
/// 索引冲突或数据库错误时拒绝启动。
pub async fn ensure_indexes(db: &Database) -> Result<()> {
    db.collection::<Document>(SETUP_RECEIPTS)
        .create_indexes(vec![unique_index("uk_supplier_api_order_setup_receipt_id", doc! {"id":1})])
        .await?;
    db.collection::<Document>(EXECUTIONS)
        .create_indexes(vec![
            unique_index("uk_supplier_api_order_execution_id", doc! {"id":1}),
            named_index("idx_supplier_api_execution_due", doc! {"stage":1,"next_run_at":1,"id":1}),
            named_index("idx_supplier_api_execution_purchase", doc! {"purchase_order_id":1}),
            named_index(
                "idx_supplier_api_execution_address",
                doc! {"connection_id":1,"address_fingerprint":1,"updated_at":-1},
            ),
        ])
        .await?;
    Ok(())
}
