use erp_core::ids::SalesOrderId;
use mongodb::bson::doc;
use persistence_core::{Executor, Result};

use crate::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use crate::repository::owned::WorkItemRepository;

impl<'a> WorkItemRepository<'a> {
    /// 查询同一责任键下全部开放采购履约任务。
    ///
    /// # 参数
    /// * `responsibility_key` - 采购单责任键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按创建时间、任务 ID 稳定排序的开放履约任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 查询固定限制 `FULFILLMENT_OPERATION + OPEN`，供采购单责任转交原子级联。
    pub async fn list_open_fulfillment_by_responsibility_key(
        &self,
        responsibility_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "work_item_type": WorkItemType::FulfillmentOperation.as_str(),
                "responsibility_key": responsibility_key,
                "status": WorkItemStatus::Open.as_str(),
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 查询具体账号拥有的开放供给分配任务。
    ///
    /// # 参数
    /// * `owner_user_id` - 当前已认证采购账号
    /// * `sales_order_id` - 可选来源销售单筛选
    /// * `work_item_id` - 可选任务 ID 筛选
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按创建时间升序排列的开放供给分配任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_open_procurement_owned_by(
        &self,
        owner_user_id: &str,
        sales_order_id: Option<&str>,
        work_item_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        let mut filter = doc! {
            "work_item_type": WorkItemType::ProcurementOrderCreation.as_str(),
            "business_object_type": "sales_order",
            "status": WorkItemStatus::Open.as_str(),
            "owner_user_id": owner_user_id,
        };
        if let Some(sales_order_id) = sales_order_id {
            filter.insert("business_object_id", sales_order_id);
        }
        if let Some(work_item_id) = work_item_id {
            filter.insert("id", work_item_id);
        }
        self.find_many_sorted(filter, doc! { "created_at": 1 }, executor).await
    }

    /// 查询销售单全部供给分配任务并把最新任务排在前面。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单稳定身份
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按更新时间、创建时间倒序排列的全部生命周期任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_procurement_by_sales_order_newest_first(
        &self,
        sales_order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "business_object_type": "sales_order",
                "business_object_id": sales_order_id,
                "work_item_type": WorkItemType::ProcurementOrderCreation.as_str(),
            },
            doc! { "updated_at": -1, "created_at": -1 },
            executor,
        )
        .await
    }

    /// 列出指定销售责任范围的开放供给分配任务。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单
    /// * `responsibility_key` - 冻结责任范围键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的开放任务；调用方据此处理幂等与异常重复。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `work_items` 集合，按业务对象引用过滤销售单，不访问销售单集合。
    pub async fn list_open_procurement_by_responsibility(
        &self,
        sales_order_id: &SalesOrderId,
        responsibility_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many(
            doc! {
                "business_object_type": "sales_order",
                "business_object_id": sales_order_id.to_string(),
                "work_item_type": WorkItemType::ProcurementOrderCreation.as_str(),
                "responsibility_key": responsibility_key,
                "status": WorkItemStatus::Open.as_str(),
            },
            executor,
        )
        .await
    }
}
