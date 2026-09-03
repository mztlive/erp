//! 销售单应收三金额摘要：Decimal128 聚合，禁止加载完整子账。
//!
//! `$match` 前缀命中 `uk_receivable_accounts_sales_order`（`sales_order_id + account_seq`）。

use entities::ids::SalesOrderId;
use entities::money::Amount;
use entities::receivable::{ReceivableAccount, SalesOrderReceivableAmountSummary};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use serde::Deserialize;

use super::super::Repository;
use crate::executor::Executor;
use crate::Result;

/// 销售单应收合计聚合行（Decimal128 求和结果）。
#[derive(Debug, Deserialize)]
struct SalesOrderAmountSummaryRow {
    /// 未删除子账数量。
    account_count: i64,
    /// 已核销含税合计。
    settled_total: Amount,
    /// 净已开含税合计。
    invoiced_total: Amount,
    /// 含税应收合计。
    gross_total: Amount,
}

impl<'a> Repository<'a, ReceivableAccount> {
    /// 按销售单聚合已核销、已开票与含税应收合计。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回强类型三金额摘要；无未删除子账时 `account_count = 0` 且金额为精确零。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或 Decimal128 反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 必须使用 Decimal128 `$sum`，不得加载完整实体后在内存折叠；软删除子账排除。
    pub async fn sales_order_amount_summary(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<SalesOrderReceivableAmountSummary> {
        let pipeline = sales_order_amount_summary_pipeline(sales_order_id);
        let rows = match executor.session() {
            Some(session) => {
                self.collection()
                    .aggregate(pipeline)
                    .with_type::<SalesOrderAmountSummaryRow>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            }
            None => {
                self.collection()
                    .aggregate(pipeline)
                    .with_type::<SalesOrderAmountSummaryRow>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        Ok(summary_from_rows(rows))
    }
}

/// 构造销售单应收三金额聚合管道。
///
/// # 参数
/// * `sales_order_id` - 来源销售单
///
/// # 返回
/// 返回 `$match` + `$group` + `$project` 三段管道。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// `$match` 使用 `sales_order_id` 等值与未删除过滤，命中身份索引前缀。
fn sales_order_amount_summary_pipeline(sales_order_id: &SalesOrderId) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "sales_order_id": sales_order_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        },
        doc! {
            "$group": {
                "_id": mongodb::bson::Bson::Null,
                "account_count": { "$sum": 1_i64 },
                "settled_total": { "$sum": "$settled_total" },
                "invoiced_total": { "$sum": "$invoiced_total" },
                "gross_total": { "$sum": "$gross_total" },
            }
        },
        doc! { "$project": { "_id": 0 } },
    ]
}

/// 把聚合行映射为摘要；空结果为精确零且无子账。
///
/// # 参数
/// * `rows` - `$group` 结果，至多一行
///
/// # 返回
/// 返回强类型摘要。
fn summary_from_rows(rows: Vec<SalesOrderAmountSummaryRow>) -> SalesOrderReceivableAmountSummary {
    let Some(row) = rows.into_iter().next() else {
        return SalesOrderReceivableAmountSummary::empty();
    };
    let account_count = u32::try_from(row.account_count).unwrap_or(u32::MAX);
    if account_count == 0 {
        return SalesOrderReceivableAmountSummary::empty();
    }
    SalesOrderReceivableAmountSummary {
        account_count,
        settled_total: row.settled_total,
        invoiced_total: row.invoiced_total,
        gross_total: row.gross_total,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use mongodb::bson::{doc, Bson};

    use super::{sales_order_amount_summary_pipeline, summary_from_rows, SalesOrderAmountSummaryRow};
    use entities::ids::{
        CustomerAccountId, PartyId, ReceivableAccountId, SalesOrderId, SalesOrderRevisionId,
    };
    use entities::money::Amount;
    use entities::receivable::{
        AccountReviewStatus, ReceivableAccount, ReceivableAccountData, SalesOrderReceivableAmountSummary,
    };
    use entities::sales_order::BusinessType;

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn account(id: &str, seq: u32, settled: &str, invoiced: &str, gross: &str) -> ReceivableAccount {
        ReceivableAccount::new(
            ReceivableAccountId::new(id),
            ReceivableAccountData {
                sales_order_id: SalesOrderId::new("so-1"),
                account_seq: seq,
                customer_id: CustomerAccountId::new("cust-1"),
                counterparty_party_id: PartyId::new("party-1"),
                source_sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                review_status: AccountReviewStatus::initial_for_sales_business_type(
                    BusinessType::GoodsService,
                ),
                reviewed_by: None,
                reviewed_at: None,
                review_evidence_reference: None,
                gross_total: amt(gross),
                settled_total: amt(settled),
                invoiceable_total: amt(gross),
                invoiced_total: amt(invoiced),
            },
            "tester",
        )
        .unwrap()
    }

    #[test]
    fn pipeline_matches_sales_order_and_sums_decimal_fields() {
        let pipeline = sales_order_amount_summary_pipeline(&SalesOrderId::new("so-1"));
        let matched = pipeline[0].get_document("$match").expect("过滤阶段");
        assert_eq!(matched.get_str("sales_order_id").unwrap(), "so-1");
        assert_eq!(matched.get_i64("deleted_at").expect("未删除条件"), 0);
        let group = pipeline[1].get_document("$group").expect("分组阶段");
        assert!(matches!(group.get("_id").expect("分组键"), Bson::Null));
        assert_eq!(
            group
                .get_document("settled_total")
                .unwrap()
                .get_str("$sum")
                .unwrap(),
            "$settled_total"
        );
        assert_eq!(
            group
                .get_document("invoiced_total")
                .unwrap()
                .get_str("$sum")
                .unwrap(),
            "$invoiced_total"
        );
        assert_eq!(
            group
                .get_document("gross_total")
                .unwrap()
                .get_str("$sum")
                .unwrap(),
            "$gross_total"
        );
    }

    #[test]
    fn empty_rows_map_to_zero_without_accounts() {
        let summary = summary_from_rows(Vec::new());
        assert_eq!(summary, SalesOrderReceivableAmountSummary::empty());
        assert!(!summary.has_accounts());
    }

    #[test]
    fn decimal128_row_deserializes_and_matches_entity_exact_add() {
        let document = doc! {
            "account_count": 2_i64,
            "settled_total": { "$numberDecimal": "10.03" },
            "invoiced_total": { "$numberDecimal": "1.10" },
            "gross_total": { "$numberDecimal": "23.38" },
        };
        let row: SalesOrderAmountSummaryRow =
            mongodb::bson::deserialize_from_document(document).expect("合法 Decimal128 必须成功");
        let aggregated = summary_from_rows(vec![row]);
        let one = account("ra-1", 1, "10.01", "1.10", "20.05");
        let two = account("ra-2", 2, "0.02", "0.00", "3.33");
        assert_eq!(
            aggregated,
            SalesOrderReceivableAmountSummary::from_accounts([&one, &two])
        );
    }

    #[test]
    fn zero_amount_row_still_counts_as_present() {
        let document = doc! {
            "account_count": 1_i64,
            "settled_total": { "$numberDecimal": "0.00" },
            "invoiced_total": { "$numberDecimal": "0.00" },
            "gross_total": { "$numberDecimal": "0.00" },
        };
        let row: SalesOrderAmountSummaryRow =
            mongodb::bson::deserialize_from_document(document).expect("合法 Decimal128 必须成功");
        let summary = summary_from_rows(vec![row]);
        assert!(summary.has_accounts());
        assert_eq!(summary.account_count, 1);
        assert_eq!(summary.gross_total, amt("0.00"));
    }

    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn sales_order_amount_summary_matches_entity_add_and_hits_identity_index() {
        use crate::repository::extensions::ReceivableExt;
        use crate::{ensure_indexes, NoTransaction};
        use test_support::{require_mongo, TestDb};

        require_mongo!(async {
            let fixture = TestDb::new("recv_so_amount_summary")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let accounts = fixture.db().receivable_accounts();

            let empty = accounts
                .sales_order_amount_summary(&SalesOrderId::new("so-missing"), &mut NoTransaction)
                .await
                .expect("无子账读取失败");
            assert_eq!(empty, SalesOrderReceivableAmountSummary::empty());

            let one = account("ra-1", 1, "10.01", "1.10", "20.05");
            let two = account("ra-2", 2, "0.02", "0.00", "3.33");
            let single = {
                let mut account = account("ra-single", 1, "7.77", "2.22", "15.50");
                account.sales_order_id = SalesOrderId::new("so-single");
                account
            };
            let zero = {
                let mut account = account("ra-3", 3, "0.00", "0.00", "0.00");
                account.sales_order_id = SalesOrderId::new("so-zero");
                account
            };
            let other = {
                let mut account = account("ra-other", 1, "9.99", "0.00", "9.99");
                account.sales_order_id = SalesOrderId::new("so-other");
                account
            };
            for item in [&one, &two, &single, &zero, &other] {
                accounts
                    .create(item, &mut NoTransaction)
                    .await
                    .expect("子账写入失败");
            }
            let mut deleted = account("ra-deleted", 4, "50.00", "0.00", "50.00");
            accounts
                .create(&deleted, &mut NoTransaction)
                .await
                .expect("待删子账写入失败");
            accounts
                .soft_delete(&mut deleted, &mut NoTransaction)
                .await
                .expect("软删除失败");

            let summary = accounts
                .sales_order_amount_summary(&SalesOrderId::new("so-1"), &mut NoTransaction)
                .await
                .expect("摘要读取失败");
            assert_eq!(
                summary,
                SalesOrderReceivableAmountSummary::from_accounts([&one, &two])
            );

            let single_summary = accounts
                .sales_order_amount_summary(&SalesOrderId::new("so-single"), &mut NoTransaction)
                .await
                .expect("单子账摘要读取失败");
            assert_eq!(
                single_summary,
                SalesOrderReceivableAmountSummary::from_accounts([&single])
            );
            assert_eq!(single_summary.account_count, 1);
            assert_eq!(single_summary.settled_total, amt("7.77"));
            assert_eq!(single_summary.invoiced_total, amt("2.22"));
            assert_eq!(single_summary.gross_total, amt("15.50"));

            let zero_summary = accounts
                .sales_order_amount_summary(&SalesOrderId::new("so-zero"), &mut NoTransaction)
                .await
                .expect("零额摘要读取失败");
            assert!(zero_summary.has_accounts());
            assert_eq!(zero_summary.account_count, 1);
            assert_eq!(zero_summary.gross_total, amt("0.00"));

            let explain = fixture
                .db()
                .run_command(doc! {
                    "explain": {
                        "aggregate": <mongodb::Database as ReceivableExt>::RECEIVABLE_ACCOUNTS,
                        "pipeline": sales_order_amount_summary_pipeline(&SalesOrderId::new("so-1")),
                        "cursor": {},
                    },
                    "verbosity": "executionStats",
                })
                .await
                .expect("explain 失败");
            let rendered = format!("{explain:?}");
            assert!(
                rendered.contains("IXSCAN"),
                "销售单应收摘要必须 IXSCAN：{rendered}"
            );
            assert!(
                !rendered.contains("COLLSCAN"),
                "销售单应收摘要不得 COLLSCAN：{rendered}"
            );
            assert!(
                rendered.contains("uk_receivable_accounts_sales_order"),
                "销售单应收摘要必须命中 sales_order_id + account_seq 索引：{rendered}"
            );
        });
    }
}
