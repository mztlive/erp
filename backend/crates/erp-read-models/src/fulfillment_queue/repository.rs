//! W09 履约责任队列的 MongoDB 页面投影。
//!
//! 查询从当前个人开放的 `FULFILLMENT_OPERATION` WorkItem 出发，以责任事实
//! 作为权限范围，再关联四类履约草稿、来源采购/销售单和仓库。聚合在服务端完成
//! 筛选、指标和分页；客户端不得逐页拉取四个单据列表后自行拼接。

mod page;
mod pipeline;
mod prepayment;

use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItemPriority;
use futures_util::TryStreamExt;
use mongodb::Database;
use persistence_core::{Executor, Result};
use pipeline::fulfillment_queue_pipeline;
use serde::Deserialize;

/// 履约责任队列的仓储筛选；所有字符串均已由 Service 白名单化或规范化。
#[derive(Debug, Clone)]
pub struct FulfillmentQueueFilter {
    /// 当前已认证个人责任人。
    pub owner_user_id: String,
    /// 服务端允许且调用方请求的作业类型稳定代码。
    pub operation_types: Vec<String>,
    /// 精确履约对象；用于工作台单任务聚焦。
    pub operation_id: Option<String>,
    /// 来源销售单。
    pub sales_order_id: Option<String>,
    /// 来源采购单。
    pub purchase_order_id: Option<String>,
    /// 履约仓库。
    pub warehouse_id: Option<String>,
    /// 权限范围内的单号/摘要字面量检索。
    pub query: Option<String>,
    /// 作业日期下界（包含，Unix 秒）。
    pub due_from: Option<i64>,
    /// 作业日期上界（不包含，Unix 秒）。
    pub due_before: Option<i64>,
    /// `SATISFIED`、`BLOCKED` 或空。
    pub gate: Option<String>,
    /// 已检查的分页偏移。
    pub offset: u64,
    /// 单页条数。
    pub page_size: u32,
}

impl FulfillmentQueueFilter {
    /// 构造履约责任队列仓储筛选。
    ///
    /// # 参数
    /// * `owner_user_id` - 当前已认证个人责任人
    ///
    /// # 返回
    /// 返回作业类型、分页全空的筛选。
    ///
    /// # 错误
    /// 无。
    pub fn new(owner_user_id: String) -> Self {
        Self {
            owner_user_id,
            operation_types: Vec::new(),
            operation_id: None,
            sales_order_id: None,
            purchase_order_id: None,
            warehouse_id: None,
            query: None,
            due_from: None,
            due_before: None,
            gate: None,
            offset: 0,
            page_size: 20,
        }
    }

    /// 设置作业类型。
    ///
    /// # 参数
    /// * `operation_types` - 服务端允许且调用方请求的作业类型稳定代码
    ///
    /// # 返回
    /// 返回更新后的筛选。
    ///
    /// # 错误
    /// 无。
    pub fn with_operation_types(mut self, operation_types: Vec<String>) -> Self {
        self.operation_types = operation_types;
        self
    }

    /// 设置精确履约对象与来源单据。
    ///
    /// # 参数
    /// * `operation_id` - 精确履约对象
    /// * `sales_order_id` - 来源销售单
    /// * `purchase_order_id` - 来源采购单
    /// * `warehouse_id` - 履约仓库
    ///
    /// # 返回
    /// 返回更新后的筛选。
    ///
    /// # 错误
    /// 无。
    pub fn with_scope(
        mut self,
        operation_id: Option<String>,
        sales_order_id: Option<String>,
        purchase_order_id: Option<String>,
        warehouse_id: Option<String>,
    ) -> Self {
        self.operation_id = operation_id;
        self.sales_order_id = sales_order_id;
        self.purchase_order_id = purchase_order_id;
        self.warehouse_id = warehouse_id;
        self
    }

    /// 设置检索与先决条件。
    ///
    /// # 参数
    /// * `query` - 单号/摘要字面量检索
    /// * `due_from` - 作业日期下界
    /// * `due_before` - 作业日期上界
    /// * `gate` - 先决条件筛选
    ///
    /// # 返回
    /// 返回更新后的筛选。
    ///
    /// # 错误
    /// 无。
    pub fn with_conditions(
        mut self,
        query: Option<String>,
        due_from: Option<i64>,
        due_before: Option<i64>,
        gate: Option<String>,
    ) -> Self {
        self.query = query;
        self.due_from = due_from;
        self.due_before = due_before;
        self.gate = gate;
        self
    }

    /// 设置分页。
    ///
    /// # 参数
    /// * `offset` - 已检查的分页偏移
    /// * `page_size` - 单页条数
    ///
    /// # 返回
    /// 返回更新后的筛选。
    ///
    /// # 错误
    /// 无。
    pub fn with_paging(mut self, offset: u64, page_size: u32) -> Self {
        self.offset = offset;
        self.page_size = page_size;
        self
    }
}

/// 履约责任队列当前页的一行。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FulfillmentQueueItemRow {
    pub work_item_id: String,
    pub task_version: u64,
    pub subject_version: String,
    pub owner_role: String,
    pub owner_organization_id: String,
    pub priority: WorkItemPriority,
    pub reason_code: String,
    pub impact_summary: String,
    pub work_item_created_at: u64,
    pub operation_id: String,
    pub operation_type: String,
    pub business_object_type: String,
    pub summary: String,
    pub edit_version: u64,
    pub due_at: i64,
    pub sales_order_id: Option<String>,
    pub sales_order_no: Option<String>,
    pub purchase_order_id: Option<String>,
    pub purchase_order_no: Option<String>,
    pub warehouse_id: Option<String>,
    pub warehouse_label: Option<String>,
    pub sales_order_line_id: Option<String>,
    pub purchase_line_sales_allocation_id: Option<String>,
    pub quantity: Option<String>,
    pub result: Option<String>,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
    pub gate_state: String,
    pub gate_required_amount: Option<String>,
    pub gate_effective_paid_amount: Option<String>,
}

/// 作业类型跨页计数。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FulfillmentQueueMetricRow {
    pub operation_type: String,
    pub count: i64,
}

/// 履约责任队列仓储结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FulfillmentQueueRepositoryPage {
    pub items: Vec<FulfillmentQueueItemRow>,
    pub total: i64,
    pub metrics: Vec<FulfillmentQueueMetricRow>,
}

#[derive(Debug, Deserialize)]
struct CountRow {
    count: i64,
}

#[derive(Debug, Default, Deserialize)]
struct FulfillmentQueueFacetRow {
    #[serde(default)]
    items: Vec<FulfillmentQueueItemRow>,
    #[serde(default)]
    total: Vec<CountRow>,
    #[serde(default)]
    metrics: Vec<FulfillmentQueueMetricRow>,
}

/// Owned read repository for the fulfillment queue page projection.
pub struct FulfillmentQueueRepository<'a> {
    db: &'a Database,
}

impl<'a> FulfillmentQueueRepository<'a> {
    /// Bind the repository to a database.
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    ///
    /// # 返回
    /// 返回履约队列只读仓储。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 工作项集合。
    ///
    /// # 返回
    /// 返回工作项 Mongo 集合。
    ///
    /// # 错误
    /// 无。
    fn collection(&self) -> mongodb::Collection<erp_workflow::entity::work_item::WorkItem> {
        self.db.collection(<Database as WorkItemExt>::WORK_ITEMS)
    }

    /// 查询当前个人责任范围内的履约页面投影。
    ///
    /// # 参数
    /// * `filter` - 已由 Service 校验的筛选和分页
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前页、跨页总数、类型指标和仓库选项。
    ///
    /// # 错误
    /// 聚合管道构造、MongoDB 执行或类型化反序列化失败时返回错误。
    pub async fn search_fulfillment_queue(
        &self,
        filter: &FulfillmentQueueFilter,
        executor: &mut dyn Executor,
    ) -> Result<FulfillmentQueueRepositoryPage> {
        let pipeline = fulfillment_queue_pipeline(filter)?;
        let collection = self.collection();
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<FulfillmentQueueFacetRow>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            },
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<FulfillmentQueueFacetRow>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            },
        };
        let facet = rows.into_iter().next().unwrap_or_default();
        Ok(FulfillmentQueueRepositoryPage {
            items: facet.items,
            total: facet.total.first().map_or(0, |row| row.count),
            metrics: facet.metrics,
        })
    }
}
