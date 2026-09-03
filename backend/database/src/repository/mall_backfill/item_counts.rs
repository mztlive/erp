//! 回填明细计数查询（INT-R14）。
//!
//! `backfill_job_detail` 曾以“第一页一条记录仅取 `total`”的方式统计明细数，
//! 现由本文件的 `count_by_job` 独占：同一作业过滤、同一软删除语义的单次计数。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};

use super::MallConsumptionBackfillItemRepository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 构建按作业统计明细数的过滤条件（与列表查询同语义）。
///
/// # 参数
/// * `job_id` - 回填批次主键字符串
///
/// # 返回
/// 返回作业等值与未删除标记的组合过滤。
///
/// # 错误
/// 无。
///
/// # 约束
/// 纯过滤构造；软删除、作业归属语义与 `search_backfill_items` 一致。
fn count_by_job_filter(job_id: &str) -> Document {
    doc! {
        "job_id": job_id,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

impl<'a> MallConsumptionBackfillItemRepository<'a> {
    /// 按回填批次统计明细总数（INT-R14）。
    ///
    /// 过滤固定为作业等值加未删除标记，与 `search_backfill_items` 的计数口径
    /// 一致；空作业返回零，不区分“作业不存在”与“作业无明细”（缺失判定留 Service）。
    /// 全部使用调用方执行器，事务内调用看到同一事务未提交写入；本方法不自行
    /// 开启或提交事务。
    ///
    /// # 参数
    /// * `job_id` - 回填批次 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该批次的明细总数。
    ///
    /// # 错误
    /// MongoDB 统计失败时返回错误。
    ///
    /// # 约束
    /// 不返回 services DTO、HTTP View 或授权结论；排序与分页不适用（N/A）。
    pub async fn count_by_job(
        &self,
        job_id: &entities::ids::MallConsumptionBackfillJobId,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        mongo_ops::count_documents(&self.collection(), count_by_job_filter(job_id.as_ref()), executor).await
    }
}

#[cfg(test)]
mod tests {
    use super::count_by_job_filter;

    /// happy path：计数过滤固定作业等值与未删除标记。
    #[test]
    fn count_filter_pins_job_and_not_deleted() {
        let document = count_by_job_filter("job-1");
        assert_eq!(document.get_str("job_id").unwrap(), "job-1");
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
    }

    /// 边界：不同作业互不可见（过滤携带作业身份）。
    #[test]
    fn count_filter_is_scoped_by_job() {
        let first = count_by_job_filter("job-1");
        let second = count_by_job_filter("job-2");
        assert_ne!(
            first.get_str("job_id").unwrap(),
            second.get_str("job_id").unwrap()
        );
    }
}

/// INT-R14 真实 MongoDB 验收（隔离库，`#[ignore]` 由 Quality 单独执行）。
#[cfg(test)]
mod mongo_tests {
    use entities::ids::{InboxMessageId, MallConsumptionBackfillItemId, MallConsumptionBackfillJobId};
    use entities::mall_backfill::{
        BackfillCostBasis, BackfillItemResult, MallConsumptionBackfillItem, MallConsumptionBackfillItemData,
    };
    use mongodb::bson::doc;

    /// 构造指定批次与业务键的明细夹具。
    fn fixture_item(job_id: &str, key: &str, seq: &str) -> MallConsumptionBackfillItem {
        MallConsumptionBackfillItem::new(
            MallConsumptionBackfillItemId::new(format!("bi-{seq}")),
            MallConsumptionBackfillItemData {
                job_id: MallConsumptionBackfillJobId::new(job_id),
                business_fact_key: key.to_string(),
                source_event_reference: format!("src-{seq}"),
                inbox_message_id: InboxMessageId::new(format!("inbox-{seq}")),
                mall_order_fact_id: None,
                result: BackfillItemResult::Duplicate,
                cost_basis: BackfillCostBasis::None,
                error_code: None,
                error_detail: None,
            },
        )
        .expect("回填明细夹具构造失败")
    }

    /// 空批次、多条、逻辑删除与跨批次隔离的实时计数。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 计数与列表总数一致、软删除排除、跨批次隔离时通过。
    ///
    /// # 错误
    /// 计数偏离、软删除计入或跨批次泄漏时测试失败。
    ///
    /// # 约束
    /// 仅验证 `mall_consumption_backfill_items` 集合；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn count_by_job_matches_live_fixtures() {
        use super::super::MallConsumptionBackfillItemFilter;
        use crate::repository::extensions::MallBackfillExt;
        use crate::{ensure_indexes, NoTransaction};
        use test_support::{require_mongo, TestDb};

        require_mongo!(async {
            let fixture = TestDb::new("backfill_item_count")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let items = fixture.db().mall_consumption_backfill_items();
            for item in [
                fixture_item("job-count-1", "k1", "c1"),
                fixture_item("job-count-1", "k2", "c2"),
                fixture_item("job-count-1", "k3", "c3"),
                fixture_item("job-count-2", "k1", "c4"),
            ] {
                items
                    .create(&item, &mut NoTransaction)
                    .await
                    .expect("回填明细写入失败");
            }
            fixture
                .db()
                .collection::<mongodb::bson::Document>(
                    <mongodb::Database as MallBackfillExt>::MALL_CONSUMPTION_BACKFILL_ITEMS,
                )
                .update_one(
                    doc! { "id": "bi-c3" },
                    doc! { "$set": { "deleted_at": 1_700_000_001_i64 } },
                )
                .await
                .expect("逻辑删除夹具失败");

            let first = items
                .count_by_job(
                    &MallConsumptionBackfillJobId::new("job-count-1"),
                    &mut NoTransaction,
                )
                .await
                .expect("计数失败");
            assert_eq!(first, 2, "逻辑删除不得计入");
            let second = items
                .count_by_job(
                    &MallConsumptionBackfillJobId::new("job-count-2"),
                    &mut NoTransaction,
                )
                .await
                .expect("计数失败");
            assert_eq!(second, 1, "同键跨批次互不可见");
            let missing = items
                .count_by_job(
                    &MallConsumptionBackfillJobId::new("job-missing"),
                    &mut NoTransaction,
                )
                .await
                .expect("空批次计数失败");
            assert_eq!(missing, 0);

            let page = items
                .search_backfill_items(
                    &MallConsumptionBackfillItemFilter {
                        job_id: Some(MallConsumptionBackfillJobId::new("job-count-1")),
                        result: None,
                        cost_basis: None,
                        page: 1,
                        page_size: 10,
                        sort_by: None,
                        sort_ascending: false,
                    },
                    &mut NoTransaction,
                )
                .await
                .expect("列表总数对拍失败");
            assert_eq!(page.total, first as i64, "计数必须与列表总数一致");
        });
    }
}
