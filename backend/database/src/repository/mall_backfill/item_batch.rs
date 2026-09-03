//! 回填明细批量去重与批量追加（INT-R15）。
//!
//! `submit_backfill_command` 曾对每项事实逐一按业务键查重并逐条创建（N+1 写入），
//! 现由本文件独占两次有界访问：按业务键集合一次批量返回已存在键，再把新增明细
//! 一次批量追加。最终唯一性仍由 `(job_id, business_fact_key)` 唯一索引保证，
//! 批量预查不得替代该约束；并发竞态的 `DuplicateKey` 由调用方事务整体回滚。

use std::collections::HashSet;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::Deserialize;

use super::MallConsumptionBackfillItemRepository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 已存在业务键的最小投影行。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct BusinessFactKeyRow {
    /// 事实身份。
    business_fact_key: String,
}

/// 按首次出现顺序去重业务键（空串原样保留）。
///
/// # 参数
/// * `keys` - 输入业务键集合（可含重复）
///
/// # 返回
/// 返回去重后的业务键集合，顺序与首次出现一致。
///
/// # 错误
/// 无。
///
/// # 约束
/// 纯内存去重；空串不静默丢弃：库内不存在空键故 `$in` 命中为空，
/// 与 Service 装配侧 fail-closed 拒绝空键一致；空输入返回空集合，
/// 调用方据此跳过数据库访问。
fn dedupe_business_fact_keys(keys: &[String]) -> Vec<String> {
    let mut seen = HashSet::with_capacity(keys.len());
    keys.iter()
        .filter(|key| seen.insert((*key).clone()))
        .cloned()
        .collect()
}

/// 构建批量已存在键查询的过滤条件。
///
/// # 参数
/// * `job_id` - 回填批次主键字符串
/// * `keys` - 去重后的业务键集合（非空）
///
/// # 返回
/// 返回作业等值、业务键 `$in` 与未删除标记的组合过滤。
///
/// # 错误
/// 无。
///
/// # 约束
/// 纯过滤构造；软删除语义与单键查询一致。
fn existing_keys_filter(job_id: &str, keys: &[String]) -> Document {
    doc! {
        "job_id": job_id,
        "business_fact_key": { "$in": keys },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

impl<'a> MallConsumptionBackfillItemRepository<'a> {
    /// 按批次与业务键集合一次批量返回已存在的业务键（INT-R15）。
    ///
    /// 空输入直接返回空集合且不访问数据库；输入重复键自动去重，返回集合为输入
    /// 的子集，不承诺顺序。缺失键由 Service 解释为可新增。全部使用调用方执行器，
    /// 事务内调用看到同一事务未提交写入；本方法不自行开启或提交事务。
    ///
    /// # 参数
    /// * `job_id` - 回填批次 ID
    /// * `business_fact_keys` - 待判定的业务键集合（可含重复）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该批次已存在的业务键集合。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 只返回最小键投影；不返回 services DTO、HTTP View 或授权结论；预查结果
    /// 不得替代 `(job_id, business_fact_key)` 唯一索引的最终约束。
    pub async fn list_existing_keys(
        &self,
        job_id: &entities::ids::MallConsumptionBackfillJobId,
        business_fact_keys: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let unique = dedupe_business_fact_keys(business_fact_keys);
        if unique.is_empty() {
            return Ok(Vec::new());
        }
        let rows = mongo_ops::find_many(
            &self.collection().clone_with_type::<BusinessFactKeyRow>(),
            existing_keys_filter(job_id.as_ref(), &unique),
            FindOptions::builder()
                .projection(doc! { "_id": 0, "business_fact_key": 1 })
                .build(),
            executor,
        )
        .await?;
        Ok(rows.into_iter().map(|row| row.business_fact_key).collect())
    }

    /// 在调用方执行器内有序批量追加回填明细（INT-R15）。
    ///
    /// 空集合直接返回且不访问数据库；调用方按稳定的事实顺序传入，MongoDB 的
    /// ordered 插入保证首个失败后不继续写入，完整原子性仍由调用方事务负责。
    /// 唯一冲突透出 [`crate::Error::DuplicateKey`]，由 Service 映射为整批回滚。
    ///
    /// # 参数
    /// * `items` - 按业务事实顺序排列的新增明细
    /// * `executor` - 调用方事务或非事务执行器
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    ///
    /// # 约束
    /// 不开事务、不提交事务；确定性写入顺序由调用方保证。
    pub async fn create_many_ordered(
        &self,
        items: &[entities::mall_backfill::MallConsumptionBackfillItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        mongo_ops::insert_many(&self.collection(), items.to_vec(), executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{dedupe_business_fact_keys, existing_keys_filter};

    /// happy path：去重保持首次顺序，空串原样保留（库内无空键，命中为空）。
    #[test]
    fn dedupe_keeps_first_order_and_preserves_blanks() {
        let keys = vec![
            "k1".to_string(),
            "k2".to_string(),
            "k1".to_string(),
            String::new(),
            "k3".to_string(),
        ];
        assert_eq!(
            dedupe_business_fact_keys(&keys),
            vec![
                "k1".to_string(),
                "k2".to_string(),
                String::new(),
                "k3".to_string()
            ]
        );
    }

    /// 边界：空输入返回空集合（调用方可跳过数据库访问）。
    #[test]
    fn dedupe_empty_yields_empty() {
        assert!(dedupe_business_fact_keys(&[]).is_empty());
    }

    /// happy path：批量过滤固定作业、`$in` 键集合与未删除标记。
    #[test]
    fn existing_keys_filter_pins_job_keys_and_not_deleted() {
        let document = existing_keys_filter("job-1", &["k1".to_string(), "k2".to_string()]);
        assert_eq!(document.get_str("job_id").unwrap(), "job-1");
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        let keys = document.get_document("business_fact_key").unwrap();
        assert_eq!(keys.len(), 1);
    }
}

/// INT-R15 真实 MongoDB 验收（隔离库，`#[ignore]` 由 Quality 单独执行）。
#[cfg(test)]
mod mongo_tests {
    use entities::ids::{InboxMessageId, MallConsumptionBackfillItemId, MallConsumptionBackfillJobId};
    use entities::mall_backfill::{
        BackfillCostBasis, BackfillItemResult, MallConsumptionBackfillItem, MallConsumptionBackfillItemData,
    };

    /// 构造指定批次与业务键的去重明细夹具。
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

    /// 库内重复一次批量返回子集：命中去重、缺失与跨批次隔离。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 返回集合为输入子集且跨批次键不可见时通过。
    ///
    /// # 错误
    /// 子集语义偏离或跨批次泄漏时测试失败。
    ///
    /// # 约束
    /// 仅验证 `mall_consumption_backfill_items` 集合；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn list_existing_keys_returns_subset_scoped_by_job() {
        use crate::repository::extensions::MallBackfillExt;
        use crate::{ensure_indexes, NoTransaction};
        use test_support::{require_mongo, TestDb};

        require_mongo!(async {
            let fixture = TestDb::new("backfill_existing_keys")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let items = fixture.db().mall_consumption_backfill_items();
            for item in [
                fixture_item("job-1", "k1", "s1"),
                fixture_item("job-1", "k2", "s2"),
                fixture_item("job-2", "k1", "s3"),
            ] {
                items
                    .create(&item, &mut NoTransaction)
                    .await
                    .expect("回填明细写入失败");
            }

            let mut existing = items
                .list_existing_keys(
                    &MallConsumptionBackfillJobId::new("job-1"),
                    &[
                        "k1".to_string(),
                        "k2".to_string(),
                        "k-missing".to_string(),
                        "k1".to_string(),
                    ],
                    &mut NoTransaction,
                )
                .await
                .expect("批量已存在键读取失败");
            existing.sort();
            assert_eq!(existing, vec!["k1".to_string(), "k2".to_string()]);

            let other = items
                .list_existing_keys(
                    &MallConsumptionBackfillJobId::new("job-missing"),
                    &["k1".to_string()],
                    &mut NoTransaction,
                )
                .await
                .expect("空批次读取失败");
            assert!(other.is_empty());
        });
    }

    /// 同一批次重复业务键的批量追加必须命中唯一索引。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 冲突透出 `DuplicateKey` 且首条已写入行保留时通过。
    ///
    /// # 错误
    /// 未拒绝重复或错误类型非 `DuplicateKey` 时测试失败。
    ///
    /// # 约束
    /// 仅验证 `uk_mall_consumption_backfill_items_key`；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn create_many_ordered_rejects_duplicate_key() {
        use crate::repository::extensions::MallBackfillExt;
        use crate::{ensure_indexes, NoTransaction};
        use test_support::{require_mongo, TestDb};

        require_mongo!(async {
            let fixture = TestDb::new("backfill_duplicate_key")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let items = fixture.db().mall_consumption_backfill_items();
            items
                .create(&fixture_item("job-1", "k1", "s1"), &mut NoTransaction)
                .await
                .expect("首条明细写入失败");

            let err = items
                .create_many_ordered(&[fixture_item("job-1", "k1", "s2")], &mut NoTransaction)
                .await
                .expect_err("重复业务键必须拒绝");
            assert!(
                matches!(err, crate::Error::DuplicateKey(_)),
                "重复键必须透出 DuplicateKey：{err:?}"
            );
            let count = items
                .count_by_job(&MallConsumptionBackfillJobId::new("job-1"), &mut NoTransaction)
                .await
                .expect("计数失败");
            assert_eq!(count, 1);
        });
    }

    /// 事务内第二次写入冲突必须整体回滚，零部分提交。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 事务返回 `DuplicateKey` 且事后计数为零时通过。
    ///
    /// # 错误
    /// 未回滚或残留部分写入时测试失败。
    ///
    /// # 约束
    /// 事务边界仍由调用方持有；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn second_write_conflict_rolls_back_whole_transaction() {
        use crate::repository::extensions::MallBackfillExt;
        use crate::{ensure_indexes, NoTransaction, Transactional};
        use test_support::{require_mongo, TestDb};

        require_mongo!(async {
            let fixture = TestDb::new("backfill_tx_rollback")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");

            let db = fixture.db().clone();
            let outcome: crate::Result<()> = fixture
                .client()
                .with_transaction(|session| {
                    Box::pin(async move {
                        let items = db.mall_consumption_backfill_items();
                        items.create(&fixture_item("job-1", "k1", "s1"), session).await?;
                        items.create(&fixture_item("job-1", "k1", "s2"), session).await?;
                        Ok(())
                    })
                })
                .await;
            assert!(
                matches!(outcome, Err(crate::Error::DuplicateKey(_))),
                "事务内重复键必须透出 DuplicateKey：{outcome:?}"
            );

            let count = fixture
                .db()
                .mall_consumption_backfill_items()
                .count_by_job(&MallConsumptionBackfillJobId::new("job-1"), &mut NoTransaction)
                .await
                .expect("回滚后计数失败");
            assert_eq!(count, 0, "任一失败必须零写入");
        });
    }

    /// 双会话并发同键竞态恰好一方获胜，且无部分写入。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 恰好一个事务提交、事后计数为一时通过。
    ///
    /// # 错误
    /// 双胜、双败或残留多行时测试失败。
    ///
    /// # 约束
    /// 并发仲裁只靠唯一索引；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn concurrent_same_key_race_has_exactly_one_winner() {
        use crate::repository::extensions::MallBackfillExt;
        use crate::{ensure_indexes, NoTransaction, Transactional};
        use test_support::{require_mongo, TestDb};

        require_mongo!(async {
            let fixture = TestDb::new("backfill_concurrent_race")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");

            let race = |seq: &'static str| {
                let client = fixture.client().clone();
                let db = fixture.db().clone();
                tokio::spawn(async move {
                    let item = fixture_item("job-race", "k-race", seq);
                    client
                        .with_transaction(|session| {
                            Box::pin(async move {
                                db.mall_consumption_backfill_items()
                                    .create(&item, session)
                                    .await?;
                                Ok::<(), crate::Error>(())
                            })
                        })
                        .await
                })
            };
            let (first, second) = tokio::join!(race("t1"), race("t2"));
            let first = first.expect("并发任务 join 失败");
            let second = second.expect("并发任务 join 失败");
            assert!(
                first.is_ok() != second.is_ok(),
                "并发同键必须恰好一方获胜：{first:?} / {second:?}"
            );

            let count = fixture
                .db()
                .mall_consumption_backfill_items()
                .count_by_job(&MallConsumptionBackfillJobId::new("job-race"), &mut NoTransaction)
                .await
                .expect("竞态后计数失败");
            assert_eq!(count, 1, "并发竞态必须只留一行");
        });
    }
}
