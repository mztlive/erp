//! 线下服务履约本域查询及事实持久化。

use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::FulfillmentService;
use crate::Result;
use crate::dto::{ServiceFulfillmentListParams, ServiceFulfillmentView, SortDir};
use crate::entity::fulfillment::ServiceFulfillment;
use crate::repository::FulfillmentExt;

type ServiceFulfillmentFilter = <mongodb::Database as FulfillmentExt>::ServiceFulfillmentFilter;

impl FulfillmentService {
    // ---------------------------------------------------------- service_fulfillment

    /// 分页查询线下服务履约记录列表（W01 履约任务作业面）。
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
        name = "fulfillment.service_fulfillment_list",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "service_fulfillment_list")
    )]
    pub async fn service_fulfillment_list(
        &self,
        params: &ServiceFulfillmentListParams,
    ) -> Result<crate::dto::PageView<ServiceFulfillmentView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ServiceFulfillmentFilter {
            sales_order_line_id: query.sales_order_line_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page =
            self.db.service_fulfillments().search_service_fulfillments(&filter, &mut NoTransaction).await?;
        super::map_search_page(
            async { Ok(page) },
            |row| ServiceFulfillmentView {
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
            },
            filter.page,
            filter.page_size,
        )
        .await
    }

    /// 按主键查询线下服务履约记录。
    ///
    /// W01 履约任务以工作项冻结的业务对象主键精确读取，不经列表分页扫描。
    ///
    /// # 参数
    /// * `id` - 服务履约记录主键
    ///
    /// # 返回
    /// 返回服务履约记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 服务履约记录不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.service_fulfillment_detail",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "service_fulfillment_detail")
    )]
    pub async fn service_fulfillment_detail(&self, id: &str) -> Result<ServiceFulfillmentView> {
        let record = super::find_header_or_not_found(
            self.db.service_fulfillments().find_by_id(id, &mut NoTransaction),
            "服务履约记录不存在",
        )
        .await?;
        Ok(record.into())
    }
}

impl From<ServiceFulfillment> for ServiceFulfillmentView {
    /// 从服务履约记录实体构造视图。
    fn from(record: ServiceFulfillment) -> Self {
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
    /// 在调用方事务中写入已准备的线下服务履约草稿；不创建第二事务。
    pub async fn persist_created_service_fulfillment(
        &self,
        record: &ServiceFulfillment,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.service_fulfillments().create(record, executor).await?;
        Ok(())
    }
}
