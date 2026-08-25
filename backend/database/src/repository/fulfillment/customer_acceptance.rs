//! `customer_acceptance` 客户验收单仓储：列表投影查询与按验收单号身份查询。

use entities::common::time::Instant;
use entities::fulfillment::{AcceptanceResult, CustomerAcceptance, CustomerAcceptanceState};
use entities::ids::{CustomerAcceptanceId, SalesOrderId};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::sort_doc;
use crate::executor::Executor;
use crate::repository::{PageResult, Pagination, QueryFilter};
use crate::{mongo_ops, Repository, Result};

/// 客户验收单排序白名单（查询与测试共用）。
const CUSTOMER_ACCEPTANCE_SORT_FIELDS: &[&str] = &["accepted_at", "created_at"];

/// 客户验收单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAcceptanceRow {
    /// 实体主键。
    pub id: String,
    /// 客户验收单号。
    pub acceptance_no: String,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 验收时间。
    pub accepted_at: Instant,
    /// 验收结果。
    pub result: AcceptanceResult,
    /// 当前状态。
    pub status: CustomerAcceptanceState,
    /// 误录验收的反向事实。
    pub reversal_of_acceptance_id: Option<CustomerAcceptanceId>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 客户验收单列表筛选条件。
#[derive(Debug, Clone)]
pub struct CustomerAcceptanceFilter {
    /// 销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 单据状态；`None` 表示不筛选。
    pub status: Option<CustomerAcceptanceState>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `accepted_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for CustomerAcceptanceFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for CustomerAcceptanceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, CustomerAcceptance> {
    /// 按客户验收单号查询未删除验收单。
    ///
    /// # 参数
    /// * `acceptance_no` - 客户验收单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除验收单；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_acceptance_no(
        &self,
        acceptance_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAcceptance>> {
        self.find_one_by_field("acceptance_no", acceptance_no, executor)
            .await
    }

    /// 分页检索客户验收单列表（投影查询）。
    ///
    /// 只返回 [`CustomerAcceptanceRow`] 所需的列表字段，不加载整文档；排序
    /// 字段走白名单映射（`accepted_at`/`created_at`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_customer_acceptances(
        &self,
        filter: &CustomerAcceptanceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerAcceptanceRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                CUSTOMER_ACCEPTANCE_SORT_FIELDS,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(customer_acceptance_projection())
            .build();
        let collection = self.collection().clone_with_type::<CustomerAcceptanceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 客户验收单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn customer_acceptance_projection() -> Document {
    doc! {
        "id": 1,
        "acceptance_no": 1,
        "sales_order_id": 1,
        "accepted_at": 1,
        "result": 1,
        "status": 1,
        "reversal_of_acceptance_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{customer_acceptance_projection, sort_doc, CUSTOMER_ACCEPTANCE_SORT_FIELDS};
    use mongodb::bson::doc;

    #[test]
    fn projection_exposes_exact_list_fields() {
        let projection = customer_acceptance_projection();
        let keys: Vec<&str> = projection.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "id",
                "acceptance_no",
                "sales_order_id",
                "accepted_at",
                "result",
                "status",
                "reversal_of_acceptance_id",
                "version",
                "created_at",
            ],
            "列表投影必须精确等于 Row 字段集合"
        );
    }

    #[test]
    fn sort_whitelist_maps_accepted_at_and_defaults_to_created_at() {
        assert_eq!(
            sort_doc(Some("accepted_at"), true, CUSTOMER_ACCEPTANCE_SORT_FIELDS),
            doc! { "accepted_at": 1 },
            "白名单内字段按调用方方向排序"
        );
        assert_eq!(
            sort_doc(Some("未知字段"), false, CUSTOMER_ACCEPTANCE_SORT_FIELDS),
            doc! { "created_at": -1 },
            "白名单外的字段名回落默认排序并保留调用方方向"
        );
    }
}
