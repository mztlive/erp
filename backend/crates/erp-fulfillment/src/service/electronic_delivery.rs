//! 电子交付本域查询及事实持久化。

use super::FulfillmentService;
use crate::dto::{ElectronicDeliveryListParams, ElectronicDeliveryView, PageView, SortDir};
use crate::entity::fulfillment::ElectronicDelivery;
use crate::repository::FulfillmentExt;
use crate::{Error, Result};
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

type ElectronicDeliveryFilter = <mongodb::Database as FulfillmentExt>::ElectronicDeliveryFilter;

impl FulfillmentService {
    // ----------------------------------------------------------- electronic_delivery

    /// 分页查询电子交付记录列表（W01 履约任务作业面）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_line_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.electronic_delivery_list",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "electronic_delivery_list"
        )
    )]
    pub async fn electronic_delivery_list(
        &self,
        params: &ElectronicDeliveryListParams,
    ) -> Result<PageView<ElectronicDeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ElectronicDeliveryFilter {
            sales_order_line_id: query.sales_order_line_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .electronic_deliveries()
            .search_electronic_deliveries(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ElectronicDeliveryView {
                id: row.id.clone(),
                fulfillment_no: row.fulfillment_no,
                sales_order_line_id: row.sales_order_line_id.to_string(),
                purchase_order_id: row.purchase_order_id.to_string(),
                purchase_line_sales_allocation_id: row.purchase_line_sales_allocation_id.to_string(),
                quantity: row.quantity,
                result: row.result,
                status: row.status,
                occurred_at: row.occurred_at.unix_secs(),
                recorded_at: row.recorded_at.unix_secs(),
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 按主键查询电子交付记录。
    ///
    /// W01 履约任务以工作项冻结的业务对象主键精确读取，不经列表分页扫描。
    ///
    /// # 参数
    /// * `id` - 电子交付记录主键
    ///
    /// # 返回
    /// 返回电子交付记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 电子交付记录不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.electronic_delivery_detail",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "electronic_delivery_detail"
        )
    )]
    pub async fn electronic_delivery_detail(&self, id: &str) -> Result<ElectronicDeliveryView> {
        let record = self
            .db
            .electronic_deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("电子交付记录不存在".to_string()))?;
        Ok(record.into())
    }
}

impl From<ElectronicDelivery> for ElectronicDeliveryView {
    /// 从电子交付记录实体构造视图。
    fn from(record: ElectronicDelivery) -> Self {
        Self {
            id: record.base.id,
            fulfillment_no: record.fulfillment_no,
            sales_order_line_id: record.sales_order_line_id.to_string(),
            purchase_order_id: record.purchase_order_id.to_string(),
            purchase_line_sales_allocation_id: record.purchase_line_sales_allocation_id.to_string(),
            quantity: record.quantity,
            result: record.result,
            status: record.status,
            occurred_at: record.fact.occurred_at.unix_secs(),
            recorded_at: record.fact.recorded_at.unix_secs(),
            version: record.base.version,
        }
    }
}

impl FulfillmentService {
    /// 在调用方事务中写入已准备的电子交付草稿；不创建第二事务。
    pub async fn persist_created_electronic_delivery(
        &self,
        record: &ElectronicDelivery,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.electronic_deliveries().create(record, executor).await?;
        Ok(())
    }
}

impl FulfillmentService {
    /// 同一事务中读取待确认电子交付并执行原草稿状态守卫。
    pub async fn prepare_electronic_confirmation(
        &self,
        record_id: &erp_core::ids::ElectronicDeliveryId,
        executor: &mut dyn Executor,
    ) -> Result<ElectronicDelivery> {
        let record = self
            .db
            .electronic_deliveries()
            .find_by_id(record_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("电子交付记录不存在".to_string()))?;
        record
            .ensure_confirmable()
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        Ok(record)
    }

    /// 采购资格与分配检查成功后确认本域电子交付事实；不发送外部消息。
    pub async fn persist_electronic_confirmation(
        &self,
        record: &mut ElectronicDelivery,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        record.confirm()?;
        self.db.electronic_deliveries().update(record, executor).await?;
        Ok(())
    }
}
