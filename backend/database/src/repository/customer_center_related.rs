//! 客户对象中心的合同与销售单有界读模型。
//!
//! 单次聚合返回跨页指标和最近摘要；页面不得逐页拉取合同、销售单后自行计数。

use entities::{
    contract::ContractStatus,
    customer::CustomerAccount,
    sales_order::{CloseStatus, CommercialStatus},
};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde::Deserialize;

use super::extensions::{ContractExt, SalesOrderExt};
use super::Repository;
use crate::executor::Executor;
use crate::{Error, Result};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;

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

impl<'a> Repository<'a, CustomerAccount> {
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
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerCenterRelatedRow>> {
        let recent_limit = i64::from(recent_limit);
        if recent_limit == 0 {
            return Err(Error::EntityMetadataOutOfRange("customer_center_recent_limit"));
        }
        let pipeline = related_pipeline(customer_id, recent_limit);
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
            }
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<CustomerCenterRelatedRow>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        Ok(rows.into_iter().next())
    }
}

fn related_pipeline(customer_id: &str, recent_limit: i64) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "id": customer_id,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        },
        doc! {
            "$lookup": {
                "from": CONTRACTS,
                "let": { "customer_id": "$id" },
                "pipeline": [
                    {
                        "$match": {
                            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                            "$expr": { "$eq": ["$customer_id", "$$customer_id"] },
                        }
                    },
                    {
                        "$facet": {
                            "recent": [
                                { "$sort": { "created_at": -1, "id": -1 } },
                                { "$limit": recent_limit },
                                {
                                    "$project": {
                                        "_id": 0,
                                        "id": 1,
                                        "contract_no": 1,
                                        "status": 1,
                                    }
                                },
                            ],
                            "active": [
                                { "$match": { "status": ContractStatus::Effective.as_str() } },
                                { "$count": "count" },
                            ],
                        }
                    },
                    {
                        "$project": {
                            "_id": 0,
                            "recent": 1,
                            "active_count": {
                                "$ifNull": [
                                    { "$arrayElemAt": ["$active.count", 0] },
                                    0,
                                ]
                            },
                        }
                    },
                ],
                "as": "_contracts",
            }
        },
        doc! {
            "$lookup": {
                "from": SALES_ORDERS,
                "let": { "customer_id": "$id" },
                "pipeline": [
                    {
                        "$match": {
                            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                            "$expr": { "$eq": ["$customer_id", "$$customer_id"] },
                        }
                    },
                    {
                        "$facet": {
                            "recent": [
                                { "$sort": { "created_at": -1, "id": -1 } },
                                { "$limit": recent_limit },
                                {
                                    "$project": {
                                        "_id": 0,
                                        "id": 1,
                                        "order_no": 1,
                                        "commercial_status": 1,
                                        "close_status": 1,
                                        "created_at": 1,
                                    }
                                },
                            ],
                            "in_progress": [
                                {
                                    "$match": {
                                        "commercial_status": {
                                            "$ne": CommercialStatus::Voided.as_str(),
                                        },
                                        "close_status": { "$ne": CloseStatus::Closed.as_str() },
                                    }
                                },
                                { "$count": "count" },
                            ],
                        }
                    },
                    {
                        "$project": {
                            "_id": 0,
                            "recent": 1,
                            "in_progress_count": {
                                "$ifNull": [
                                    { "$arrayElemAt": ["$in_progress.count", 0] },
                                    0,
                                ]
                            },
                        }
                    },
                ],
                "as": "_sales_orders",
            }
        },
        doc! {
            "$project": {
                "_id": 0,
                "active_contract_count": {
                    "$ifNull": [
                        { "$arrayElemAt": ["$_contracts.active_count", 0] },
                        0,
                    ]
                },
                "in_progress_sales_order_count": {
                    "$ifNull": [
                        { "$arrayElemAt": ["$_sales_orders.in_progress_count", 0] },
                        0,
                    ]
                },
                "contracts": {
                    "$ifNull": [
                        { "$arrayElemAt": ["$_contracts.recent", 0] },
                        [],
                    ]
                },
                "sales_orders": {
                    "$ifNull": [
                        { "$arrayElemAt": ["$_sales_orders.recent", 0] },
                        [],
                    ]
                },
            }
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::related_pipeline;

    #[test]
    fn related_pipeline_is_bounded_and_counts_across_pages() {
        let rendered = format!("{:?}", related_pipeline("customer-1", 5));
        assert!(rendered.contains("customer-1"));
        assert!(rendered.contains("contracts"));
        assert!(rendered.contains("sales_orders"));
        assert!(rendered.contains("active_count"));
        assert!(rendered.contains("in_progress_count"));
        assert!(rendered.contains("Int64(5)"));
    }
}
