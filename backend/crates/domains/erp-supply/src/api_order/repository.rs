use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::indexes::{named_index, unique_index};
use persistence_core::{Executor, Repository, Result, mongo_ops};

use super::{ApiOrderExecution, ApiOrderSetupReceipt};
use crate::entity::supplier_api::SupplierApiConnection;
use crate::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use crate::repository::{SupplierApiExt, SupplierFulfillmentExt};
use crate::{Error, Result as SupplyResult};

const EXECUTIONS: &str = "supplier_api_order_executions";
const SETUP_RECEIPTS: &str = "supplier_api_order_setup_receipts";

/// 按当前已授权执行集合读取同身份正式供应商订单。
/// 仅指定且未删除的正式订单，不扩大采购范围。
/// 数据库读取失败时向上传播错误。
/// `ids` 为读模型从已授权采购派生的执行身份，空集合不查询。
pub async fn fulfillment_orders_by_execution_ids(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierFulfillmentOrder>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    db.supplier_fulfillment_orders()
        .find_many(doc! {"id":{"$in":ids},"deleted_at":NOT_DELETED_TIMESTAMP_BSON}, executor)
        .await
}

/// 按执行记录反查其所属连接，禁止从前端指定供应商归属。
/// 指定未删除连接，调用方仍须逐执行核对供应商和连接归属。
/// 数据库读取失败时向上传播错误。
/// `ids` 为服务端从已授权执行派生的连接身份，空集合不查询。
pub async fn connections_by_ids(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierApiConnection>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    db.supplier_api_connections()
        .find_many(doc! {"id":{"$in":ids},"deleted_at":NOT_DELETED_TIMESTAMP_BSON}, executor)
        .await
}

/// 按已绑定连接和实际外部子单定位原执行，供消息唤醒原根单。
/// 唯一原执行或 None；未完整关系只能作为唤醒线索。
/// 多个执行争用同一外部身份时拒绝，数据库失败向上传递。
pub async fn for_external_order(
    db: &Database,
    connection: &str,
    external: &str,
    executor: &mut dyn Executor,
) -> SupplyResult<Option<ApiOrderExecution>> {
    if connection.trim().is_empty() || external.trim().is_empty() {
        return Err(Error::ValidationError("供应商消息订单身份无效".into()));
    }

    let rows = mongo_ops::find_many(
        &db.collection::<ApiOrderExecution>(EXECUTIONS),
        doc! {"connection_id":connection,"$or":[
            {"order_tree.nodes.external_order_no":external},
            {"pending_order_tree.nodes.external_order_no":external}
        ]},
        FindOptions::builder().limit(2).build(),
        executor,
    )
    .await?;
    if rows.len() > 1 {
        return Err(Error::ConflictError("供应商子单归属不唯一，请保留消息并核对原单".into()));
    }
    Ok(rows.into_iter().next())
}

/// 接口执行意图的领域仓储访问器。
pub trait ApiOrderExt {
    /// 返回当前数据库的执行意图仓储。
    /// 使用调用方执行器的仓储。
    fn supplier_api_order_executions(&self) -> Repository<'_, ApiOrderExecution>;
    /// 取得不可删除的配送补充命令回执仓储。
    /// 当前数据库回执仓储。
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
/// 当前及历史仍有效的执行；已记载替代版本的记录不再参与变更核验。
/// 数据库读取失败时向上传播错误。
/// `purchase` 是已授权采购身份，读取沿用变更根事务。
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
/// 最多二十条按时间、身份稳定排序的记录。
/// 数据库读取失败时向上传播错误。
pub async fn due(db: &Database, executor: &mut dyn Executor) -> Result<Vec<ApiOrderExecution>> {
    mongo_ops::find_many(
        &db.collection::<ApiOrderExecution>(EXECUTIONS),
        doc! { "stage": { "$nin": ["OBSERVE", "STOPPED"] }, "next_run_at": { "$lte": Instant::now().unix_secs() } },
        FindOptions::builder().sort(doc! {"next_run_at":1,"id":1}).limit(20).build(), executor,
    ).await
}

/// 有界读取实际收货通知调度，观察和停止主状态不影响原通知结果调查。
/// 最多二十条按原调度时刻和身份稳定排序的执行。
/// 数据库读取失败时向上传播错误。
pub async fn receipts_due(db: &Database, executor: &mut dyn Executor) -> Result<Vec<ApiOrderExecution>> {
    mongo_ops::find_many(
        &db.collection::<ApiOrderExecution>(EXECUTIONS),
        doc! {"receipt_next_run_at":{"$type":"number","$lte":Instant::now().unix_secs()},"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"receipt_next_run_at":1,"id":1}).limit(20).build(),
        executor,
    )
    .await
}

/// 从已正式验收的销售行定位原采购执行，不能按 SKU 扩大通知范围。
/// 返回最多一千项原执行，超限时拒绝整次唤醒。
/// 数量超限或数据库失败时拒绝。
/// `lines` 来自实际验收行；空输入不访问数据库。
pub async fn for_sales_lines(
    db: &Database,
    lines: &[String],
    executor: &mut dyn Executor,
) -> SupplyResult<Vec<ApiOrderExecution>> {
    if lines.is_empty() {
        return Ok(vec![]);
    }

    let rows: Vec<ApiOrderExecution> = mongo_ops::find_many(
        &db.collection(EXECUTIONS),
        doc! {"source_lines.sales_order_line_id":{"$in":lines},"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"id":1}).limit(1001).build(),
        executor,
    )
    .await?;
    if rows.len() > 1000 {
        return Err(Error::BusinessLogicError("验收关联的供应商执行过多，请分批核对原记录".into()));
    }
    Ok(rows)
}

/// 按同连接、同地址指纹复用已明确配置的坐标，不猜测地理位置。
/// 最近有请求密文的候选记录。
/// 数据库失败时向上传播错误。
/// `fingerprint` 为应用密钥生成的地址指纹，不包含收件人明文。
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
/// 仅该采购范围的执行事实。
/// 数据库查询失败时向上传播错误。
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
/// 索引全部建立成功。
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
            named_index("idx_supplier_api_execution_receipt_due", doc! {"receipt_next_run_at":1,"id":1}),
            named_index(
                "idx_supplier_api_execution_receipt_sales",
                doc! {"source_lines.sales_order_line_id":1},
            ),
            named_index(
                "idx_supplier_api_execution_child",
                doc! {"connection_id":1,"order_tree.nodes.external_order_no":1},
            ),
            named_index(
                "idx_supplier_api_execution_pending_child",
                doc! {"connection_id":1,"pending_order_tree.nodes.external_order_no":1},
            ),
            named_index(
                "idx_supplier_api_execution_address",
                doc! {"connection_id":1,"address_fingerprint":1,"updated_at":-1},
            ),
        ])
        .await?;
    Ok(())
}
