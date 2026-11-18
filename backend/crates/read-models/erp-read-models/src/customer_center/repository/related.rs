//! 客户对象中心的合同与销售单有界读模型。
//!
//! 单次聚合返回跨页指标和最近摘要；页面不得逐页拉取合同、销售单后自行计数。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_contract::{ContractExt, ContractStatus};
use erp_sales::entity::sales_order::{CloseStatus, CommercialStatus};
use erp_sales::repository::SalesOrderExt;
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
    /// 绑定客户中心的只读仓储，供详情查询统计可见合同和销售单、读取近期单据。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    // 从客户账户发起聚合，以便区分客户不存在和已有客户没有可见关联单据。
    fn collection(&self) -> mongodb::Collection<erp_customer::CustomerAccount> {
        self.db.collection(<Database as erp_customer::CustomerExt>::CUSTOMER_ACCOUNTS)
    }

    /// 为客户中心汇总有效合同、进行中销售单及各自近期摘要。合同和销售分别使用传入的授权条件；某类权限缺失时，该类无可见结果。
    ///
    /// `recent_limit` 限制每类摘要条数，汇总计数仍覆盖整个授权范围。客户不存在时返回 `None`；上限为零、聚合或解码失败时返回错误。
    pub async fn customer_center_related(
        &self,
        customer_id: &str,
        recent_limit: u32,
        contract_scope: Option<&[Document]>,
        sales_scope: Option<&[Document]>,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerCenterRelatedRow>> {
        let recent_limit = i64::from(recent_limit);
        if recent_limit == 0 {
            return Err(Error::EntityMetadataOutOfRange("customer_center_recent_limit"));
        }

        let denied = [doc! { "$match": { "$expr": false } }];
        let pipeline = related_pipeline(
            customer_id,
            recent_limit,
            contract_scope.unwrap_or(&denied),
            sales_scope.unwrap_or(&denied),
        );

        let collection = self.collection();
        let rows = persistence_core::aggregate::rows(
            &collection.clone_with_type::<CustomerCenterRelatedRow>(),
            pipeline,
            executor,
        )
        .await?;
        Ok(rows.into_iter().next())
    }
}

fn related_pipeline(
    customer_id: &str,
    recent_limit: i64,
    contract_scope: &[Document],
    sales_scope: &[Document],
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

// 组装客户合同及销售摘要的聚合管道时，最近业务及数量在同一资源范围内计算，分页不得影响总数。
fn related_lookup(
    collection: &str,
    output: &str,
    scope: &[Document],
    recent_limit: i64,
    projection: Document,
    condition: Document,
) -> Document {
    let mut pipeline = vec![
        doc! {"$match":{"deleted_at":NOT_DELETED_TIMESTAMP_BSON,"$expr":{"$eq":["$customer_id","$$customer_id"]}}},
    ];
    pipeline.extend_from_slice(scope);
    pipeline.extend([
        doc! {"$facet":{"recent":[{"$sort":{"created_at":-1,"id":-1}},{"$limit":recent_limit},{"$project":projection}],"matched":[{"$match":condition},{"$count":"count"}]}},
        doc! {"$project":{"_id":0,"recent":1,"count":{"$ifNull":[{"$arrayElemAt":["$matched.count",0]},0]}}},
    ]);
    doc! {"$lookup":{"from":collection,"let":{"customer_id":"$id"},"as":output,"pipeline":pipeline}}
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{Bson, doc};

    use super::related_pipeline;

    #[test]
    fn related_totals_and_recent_rows_use_each_business_scope() {
        let contract_scope = vec![doc! {"$match": { "id": "contract-a" }}];
        let sales_scope = vec![doc! {"$match": { "sales_owner_user_id": "sales-a" }}];
        let pipeline = related_pipeline("customer-1", 5, &contract_scope, &sales_scope);
        assert_eq!(pipeline[0].get_document("$match").unwrap().get_str("id").unwrap(), "customer-1");
        for (stage, scope) in [(1, contract_scope), (2, sales_scope)] {
            let lookup = pipeline[stage].get_document("$lookup").unwrap();
            let stages = lookup.get_array("pipeline").unwrap();
            assert_eq!(stages[1], Bson::Document(scope[0].clone()));
            let facet = stages[2].as_document().unwrap().get_document("$facet").unwrap();
            let recent = facet.get_array("recent").unwrap();
            assert_eq!(recent[1].as_document().unwrap().get_i64("$limit").unwrap(), 5);
            assert_eq!(facet.get_array("matched").unwrap().len(), 2);
        }
    }
}
