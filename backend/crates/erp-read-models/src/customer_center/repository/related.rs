//! 客户对象中心的合同与销售单有界读模型。
//!
//! 单次聚合返回跨页指标和最近摘要；页面不得逐页拉取合同、销售单后自行计数。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_contract::{ContractExt, ContractStatus};
use erp_sales::entity::sales_order::{CloseStatus, CommercialStatus};
use erp_sales::repository::SalesOrderExt;
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Error, Executor, Result};
use serde::Deserialize;

const CONTRACTS: &str = <Database as ContractExt>::CONTRACTS;
const SALES_ORDERS: &str = <Database as SalesOrderExt>::SALES_ORDERS;

/// 客户中心最近合同摘要。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CustomerCenterContractRow {
    pub id: String,
    pub contract_no: String,
    pub status: ContractStatus,
}

/// 客户中心最近销售单摘要。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CustomerCenterSalesOrderRow {
    pub id: String,
    pub order_no: String,
    pub commercial_status: CommercialStatus,
    pub close_status: CloseStatus,
    pub created_at: u64,
}

/// 客户中心合同与销售读模型。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CustomerCenterRelatedRow {
    pub active_contract_count: i64,
    pub in_progress_sales_order_count: i64,
    #[serde(default)]
    pub contracts: Vec<CustomerCenterContractRow>,
    #[serde(default)]
    pub sales_orders: Vec<CustomerCenterSalesOrderRow>,
}

/// Owned read repository for customer-center related aggregates.
pub struct CustomerCenterRepository<'a> {
    db: &'a Database,
}

impl<'a> CustomerCenterRepository<'a> {
    /// Bind the repository to a database.
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    ///
    /// # 返回
    /// 返回客户中心只读仓储。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 客户账号集合。
    ///
    /// # 返回
    /// 返回客户账号 Mongo 集合。
    ///
    /// # 错误
    /// 无。
    fn collection(&self) -> mongodb::Collection<erp_customer::CustomerAccount> {
        self.db.collection(<Database as erp_customer::CustomerExt>::CUSTOMER_ACCOUNTS)
    }

    /// 查询指定客户的关联业务跨页指标与最近摘要。
    ///
    /// # 参数
    /// * `customer_id` - 已由 Handler 校验数据范围的客户 ID
    /// * `recent_limit` - 每类最近摘要上限
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 客户存在时返回单行读模型；客户不存在时返回 `None`。
    ///
    /// # 错误
    /// 摘要上限溢出、MongoDB 聚合或反序列化失败时返回错误。
    pub async fn customer_center_related(
        &self,
        customer_id: &str,
        recent_limit: u32,
        contract_scope: &Document,
        sales_scope: &Document,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerCenterRelatedRow>> {
        let recent_limit = i64::from(recent_limit);
        if recent_limit == 0 {
            return Err(Error::EntityMetadataOutOfRange("customer_center_recent_limit"));
        }
        let pipeline = related_pipeline(customer_id, recent_limit, contract_scope, sales_scope);
        let collection = self.collection();
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<CustomerCenterRelatedRow>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            },
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<CustomerCenterRelatedRow>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            },
        };
        Ok(rows.into_iter().next())
    }
}

fn related_pipeline(
    customer_id: &str,
    recent_limit: i64,
    contract_scope: &Document,
    sales_scope: &Document,
) -> Vec<Document> {
    vec![
        doc! { "$match": { "id": customer_id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON } },
        related_lookup(
            CONTRACTS,
            "_contracts",
            contract_scope,
            recent_limit,
            doc! { "_id": 0, "id": 1, "contract_no": 1, "status": 1 },
            doc! { "status": ContractStatus::Effective.as_str() },
        ),
        related_lookup(
            SALES_ORDERS,
            "_sales_orders",
            sales_scope,
            recent_limit,
            doc! { "_id": 0, "id": 1, "order_no": 1, "commercial_status": 1, "close_status": 1, "created_at": 1 },
            doc! { "commercial_status": { "$ne": CommercialStatus::Voided.as_str() }, "close_status": { "$ne": CloseStatus::Closed.as_str() } },
        ),
        doc! { "$project": { "_id": 0,
            "active_contract_count": { "$ifNull": [{ "$arrayElemAt": ["$_contracts.count", 0] }, 0] },
            "in_progress_sales_order_count": { "$ifNull": [{ "$arrayElemAt": ["$_sales_orders.count", 0] }, 0] },
            "contracts": { "$ifNull": [{ "$arrayElemAt": ["$_contracts.recent", 0] }, []] },
            "sales_orders": { "$ifNull": [{ "$arrayElemAt": ["$_sales_orders.recent", 0] }, []] },
        } },
    ]
}

/// 最近业务及数量在同一资源范围内计算，分页不得影响总数。
fn related_lookup(
    collection: &str,
    output: &str,
    scope: &Document,
    recent_limit: i64,
    projection: Document,
    condition: Document,
) -> Document {
    doc! { "$lookup": { "from": collection, "let": { "customer_id": "$id" }, "as": output,
        "pipeline": [
            { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": { "$eq": ["$customer_id", "$$customer_id"] }, "$and": [scope] } },
            { "$facet": {
                "recent": [{ "$sort": { "created_at": -1, "id": -1 } }, { "$limit": recent_limit }, { "$project": projection }],
                "matched": [{ "$match": condition }, { "$count": "count" }],
            } },
            { "$project": { "_id": 0, "recent": 1, "count": { "$ifNull": [{ "$arrayElemAt": ["$matched.count", 0] }, 0] } } },
        ]
    } }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{Bson, doc};

    use super::related_pipeline;

    #[test]
    fn related_totals_and_recent_rows_use_each_business_scope() {
        let contract_scope = doc! { "id": "contract-a" };
        let sales_scope = doc! { "sales_owner_user_id": "sales-a" };
        let pipeline = related_pipeline("customer-1", 5, &contract_scope, &sales_scope);
        assert_eq!(pipeline[0].get_document("$match").unwrap().get_str("id").unwrap(), "customer-1");
        for (stage, scope) in [(1, contract_scope), (2, sales_scope)] {
            let lookup = pipeline[stage].get_document("$lookup").unwrap();
            let stages = lookup.get_array("pipeline").unwrap();
            let filter = stages[0].as_document().unwrap().get_document("$match").unwrap();
            assert_eq!(filter.get_array("$and").unwrap()[0], Bson::Document(scope));
            let facet = stages[1].as_document().unwrap().get_document("$facet").unwrap();
            let recent = facet.get_array("recent").unwrap();
            assert_eq!(recent[1].as_document().unwrap().get_i64("$limit").unwrap(), 5);
            assert_eq!(facet.get_array("matched").unwrap().len(), 2);
        }
    }
}
