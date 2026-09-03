//! 域 D18 `receivable` 仓储：客户对象中心的应收汇总读模型。
//!
//! `impl Repository<ReceivableAccount>` 归属应收域，由
//! `receivable.rs` 经 `customer_center` 子模块挂载；
//! 应收余额、可开票余额、逾期未核销金额和最早逾期日在 MongoDB 内按客户聚合；
//! 金额始终保持 Decimal128，不经过二进制浮点数。

use std::str::FromStr;

use entities::{
    common::time::BusinessDate,
    money::Amount,
    receivable::{EntryDirection, ReceivableAccount},
};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Decimal128, Document};
use mongodb::Database;
use serde::Deserialize;

use crate::executor::Executor;
use crate::repository::extensions::ReceivableExt;
use crate::repository::Repository;
use crate::Result;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;

const RECEIVABLE_ENTRIES: &str = <Database as ReceivableExt>::RECEIVABLE_ENTRIES;
const RECEIVABLE_ENTRY_OFFSETS: &str = <Database as ReceivableExt>::RECEIVABLE_ENTRY_OFFSETS;

/// 客户中心应收汇总行；金额保持定点数值。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CustomerCenterReceivableRow {
    pub receivable_balance: Amount,
    pub overdue_amount: Amount,
    pub open_invoiceable_total: Amount,
    pub earliest_overdue_date: Option<BusinessDate>,
}

impl<'a> Repository<'a, ReceivableAccount> {
    /// 查询指定客户的应收跨账户汇总。
    ///
    /// # 参数
    /// * `customer_id` - 已授权客户 ID
    /// * `today` - 服务端业务日期；逾期上界不接受客户端输入
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回余额、逾期和可开票汇总；无账户时返回全零汇总。
    ///
    /// # 错误
    /// MongoDB 聚合或定点数值反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本域拥有的 `receivable_accounts`、`receivable_entries` 与
    /// `receivable_entry_offsets` 集合，不访问客户集合；调用方负责客户存在性与授权校验。
    pub async fn customer_center_receivable(
        &self,
        customer_id: &str,
        today: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<CustomerCenterReceivableRow> {
        let pipeline = receivable_pipeline(customer_id, &today.to_string());
        let collection = self.collection();
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<CustomerCenterReceivableRow>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            }
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<CustomerCenterReceivableRow>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        rows.into_iter().next().map_or_else(zero_summary, Ok)
    }
}

fn zero_decimal() -> Bson {
    Bson::Decimal128(Decimal128::from_str("0").expect("零必须可表示为 Decimal128"))
}

fn zero_summary() -> Result<CustomerCenterReceivableRow> {
    Ok(CustomerCenterReceivableRow {
        receivable_balance: "0".parse().expect("零必须是合法金额"),
        overdue_amount: "0".parse().expect("零必须是合法金额"),
        open_invoiceable_total: "0".parse().expect("零必须是合法金额"),
        earliest_overdue_date: None,
    })
}

fn receivable_pipeline(customer_id: &str, today: &str) -> Vec<Document> {
    let zero = zero_decimal();
    vec![
        doc! {
            "$match": {
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "customer_id": customer_id,
            }
        },
        doc! {
            "$facet": {
                "accounts": [
                    {
                        "$group": {
                            "_id": null,
                            "receivable_balance": { "$sum": "$open_total" },
                            "open_invoiceable_total": { "$sum": "$open_invoiceable_total" },
                        }
                    },
                ],
                "overdue": [
                    {
                        "$lookup": {
                            "from": RECEIVABLE_ENTRIES,
                            "let": { "account_id": "$id" },
                            "pipeline": [
                                {
                                    "$match": {
                                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                                        "direction": EntryDirection::Increase.as_str(),
                                        "due_date": { "$lt": today },
                                        "$expr": {
                                            "$eq": ["$receivable_account_id", "$$account_id"]
                                        },
                                    }
                                },
                                {
                                    "$project": {
                                        "_id": 0,
                                        "id": 1,
                                        "amount": 1,
                                        "due_date": 1,
                                    }
                                },
                            ],
                            "as": "_overdue_entries",
                        }
                    },
                    { "$unwind": "$_overdue_entries" },
                    {
                        "$lookup": {
                            "from": RECEIVABLE_ENTRY_OFFSETS,
                            "let": { "increase_entry_id": "$_overdue_entries.id" },
                            "pipeline": [
                                {
                                    "$match": {
                                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                                        "$expr": {
                                            "$eq": ["$increase_entry_id", "$$increase_entry_id"]
                                        },
                                    }
                                },
                                {
                                    "$group": {
                                        "_id": null,
                                        "offset_total": { "$sum": "$offset_amount" },
                                    }
                                },
                            ],
                            "as": "_offsets",
                        }
                    },
                    {
                        "$set": {
                            "_outstanding": {
                                "$subtract": [
                                    "$_overdue_entries.amount",
                                    {
                                        "$ifNull": [
                                            { "$arrayElemAt": ["$_offsets.offset_total", 0] },
                                            zero.clone(),
                                        ]
                                    },
                                ]
                            }
                        }
                    },
                    { "$match": { "_outstanding": { "$gt": zero.clone() } } },
                    {
                        "$group": {
                            "_id": null,
                            "overdue_amount": { "$sum": "$_outstanding" },
                            "earliest_overdue_date": { "$min": "$_overdue_entries.due_date" },
                        }
                    },
                ],
            }
        },
        doc! {
            "$project": {
                "_id": 0,
                "receivable_balance": {
                    "$ifNull": [
                        { "$arrayElemAt": ["$accounts.receivable_balance", 0] },
                        zero.clone(),
                    ]
                },
                "open_invoiceable_total": {
                    "$ifNull": [
                        { "$arrayElemAt": ["$accounts.open_invoiceable_total", 0] },
                        zero.clone(),
                    ]
                },
                "overdue_amount": {
                    "$ifNull": [
                        { "$arrayElemAt": ["$overdue.overdue_amount", 0] },
                        zero,
                    ]
                },
                "earliest_overdue_date": {
                    "$arrayElemAt": ["$overdue.earliest_overdue_date", 0]
                },
            }
        },
    ]
}

#[cfg(test)]
mod tests {
    use mongodb::bson::Bson;

    use super::{receivable_pipeline, zero_decimal};

    #[test]
    fn receivable_pipeline_aggregates_decimal_balances_and_offsets() {
        assert!(matches!(zero_decimal(), Bson::Decimal128(_)));
        let rendered = format!("{:?}", receivable_pipeline("customer-1", "2026-08-30"));
        assert!(rendered.contains("customer-1"));
        assert!(rendered.contains("receivable_entries"));
        assert!(rendered.contains("receivable_entry_offsets"));
        assert!(rendered.contains("open_invoiceable_total"));
        assert!(rendered.contains("earliest_overdue_date"));
    }
}
