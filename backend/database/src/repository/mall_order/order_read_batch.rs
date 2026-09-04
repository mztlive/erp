//! 商城订单读批量查询（INT-R01/R03/R04/R05 仓储所有权）。
//!
//! 当前页 N+1 与全量扫描只归属本模块；Service 只解释持久化事实，不做
//! 内存分组、去重、合并与链尾聚合。

use std::collections::{HashMap, HashSet};

use entities::ids::{MallConsumptionEntryId, MallOrderId, MallPaymentSourceId};
use entities::mall_order::{MallConsumptionCostAssessment, MallConsumptionEntry, MallPaymentSource};
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use super::{
    MallConsumptionCostAssessmentRepository, MallConsumptionEntryRepository, MallOrderFactRepository,
    MallOrderFactRow,
};
use crate::executor::Executor;
use crate::repository::Repository;
use crate::{mongo_ops, Result};

/// 对订单业务键去重并保持首次出现顺序。
///
/// # 参数
/// * `keys` - `(商城, 订单号)` 业务键集合
///
/// # 返回
/// 返回去重后的业务键列表。
fn unique_order_keys(keys: &[(String, String)]) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for key in keys {
        if seen.insert(key.clone()) {
            unique.push(key.clone());
        }
    }
    unique
}

/// 对字符串集合去重并保持首次出现顺序。
///
/// # 参数
/// * `values` - 待去重的字符串集合
///
/// # 返回
/// 返回去重后的字符串列表。
fn unique_strings(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            unique.push(value.clone());
        }
    }
    unique
}

/// 构造按订单业务键批量查询事实投影行的过滤文档（INT-R01）。
///
/// # 用途
/// 将当前页订单业务键展开为 `$or` 精确分支，供单次查询替代全量翻页；
/// 过滤文档纯构造，便于单元测试锁定 mall 隔离与软删除语义。
///
/// # 参数
/// * `keys` - 去重后的 `(商城, 订单号)` 业务键集合
///
/// # 返回
/// 返回含 `$or` 分支与软删除排除的过滤文档。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 每个分支同时限定商城与订单号，跨 mall 同订单号互不干扰；不做分页截断。
fn fact_row_batch_filter(keys: &[(String, String)]) -> Document {
    let branches: Vec<Document> = keys
        .iter()
        .map(|(mall_id, external_order_no)| {
            doc! {
                "mall_id": mall_id,
                "external_order_no": external_order_no,
            }
        })
        .collect();
    doc! {
        "$or": branches,
        "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回事实投影批量查询的稳定排序文档（INT-R01）。
///
/// # 返回
/// 返回 `occurred_at` 升序、同秒按稳定 `id` 升序的排序文档。
fn fact_row_batch_sort() -> Document {
    doc! { "occurred_at": 1, "id": 1 }
}

/// 构造按订单 ID 集合批量查询支付来源的过滤文档（INT-R03）。
///
/// # 用途
/// 将当前页订单 ID 展开为 `$in` 精确匹配，供单次查询消除列表 N+1；
/// 过滤文档纯构造，便于单元测试锁定软删除语义。
///
/// # 参数
/// * `ids` - 去重后的订单 ID 字符串集合
///
/// # 返回
/// 返回含 `$in` 匹配与软删除排除的过滤文档。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 只匹配未删除来源；稳定排序见 [`payment_source_batch_sort`]。
fn payment_source_batch_filter(ids: &[String]) -> Document {
    doc! {
        "mall_order_id": { "$in": ids },
        "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回支付来源批量查询的稳定排序文档（INT-R03）。
///
/// # 返回
/// 返回订单分组、组内按 `source_no` 升序的排序文档。
fn payment_source_batch_sort() -> Document {
    doc! { "mall_order_id": 1, "source_no": 1 }
}

/// 构造按支付来源 ID 集合批量查询消费事实的过滤文档（INT-R04）。
///
/// # 用途
/// 将支付来源 ID 展开为 `$in` 精确匹配，供单次查询替代逐来源读取；
/// 过滤文档纯构造，便于单元测试锁定软删除语义。
///
/// # 参数
/// * `ids` - 去重后的支付来源 ID 字符串集合
///
/// # 返回
/// 返回含 `$in` 匹配与软删除排除的过滤文档。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 只匹配未删除消费；多来源共享结果由调用方去重保持一致。
fn entry_batch_filter(ids: &[String]) -> Document {
    doc! {
        "mall_payment_source_id": { "$in": ids },
        "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回消费事实批量查询的稳定排序文档（INT-R04）。
///
/// # 返回
/// 返回 `occurred_at` 升序、同秒按稳定 `id` 升序的排序文档。
fn entry_batch_sort() -> Document {
    doc! { "occurred_at": 1, "id": 1 }
}

/// 构造按消费 ID 集合批量查询成本评估的过滤文档（INT-R05）。
///
/// # 用途
/// 将消费 ID 展开为 `$in` 精确匹配，供单次查询替代逐 entry 读链；
/// 过滤文档纯构造，便于单元测试锁定软删除语义。
///
/// # 参数
/// * `ids` - 去重后的消费 ID 字符串集合
///
/// # 返回
/// 返回含 `$in` 匹配与软删除排除的过滤文档。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 只匹配未删除评估；最新语义（最大评估号）由 [`select_latest_assessments`] 聚合。
fn assessment_batch_filter(ids: &[String]) -> Document {
    doc! {
        "mall_consumption_entry_id": { "$in": ids },
        "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回成本评估批量查询的稳定排序文档（INT-R05）。
///
/// # 返回
/// 返回评估号升序、同号按稳定 `id` 升序的排序文档。
fn assessment_batch_sort() -> Document {
    doc! { "assessment_no": 1, "id": 1 }
}

/// 从评估链按消费挑选最新评估（最大评估号，同号取稳定 ID 最大者）。
///
/// # 参数
/// * `chain` - 同批查回的评估链（任意顺序）
///
/// # 返回
/// 返回消费 ID 字符串到最新评估的映射。
fn select_latest_assessments(
    chain: Vec<MallConsumptionCostAssessment>,
) -> HashMap<String, MallConsumptionCostAssessment> {
    let mut latest: HashMap<String, MallConsumptionCostAssessment> = HashMap::new();
    for assessment in chain {
        let key = assessment.mall_consumption_entry_id.to_string();
        match latest.get(&key) {
            Some(current)
                if (current.assessment_no, current.base.id.clone())
                    >= (assessment.assessment_no, assessment.base.id.clone()) =>
            {
                continue;
            }
            _ => {
                latest.insert(key, assessment);
            }
        }
    }
    latest
}

impl MallOrderFactRepository<'_> {
    /// 按当前页订单业务键集合批量返回事实投影行（INT-R01）。
    ///
    /// # 用途
    /// 以一次 `$or` 查询替代按页扫描全部商城事实；保留 mall 过滤与稳定顺序。
    ///
    /// # 参数
    /// * `self` - 关键事实只读追加仓储
    /// * `keys` - 当前页订单业务键集合；为空时返回空列表，不访问数据库
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的事实投影行；排序合同为 `occurred_at` 升序，同秒按稳定 `id` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取失败时返回错误。
    ///
    /// # 关键约束
    /// 精确键匹配，不做分页截断；软删除事实排除；不返回 Service DTO；不开事务。
    pub async fn list_fact_rows_by_order_keys(
        &self,
        keys: &[(String, String)],
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallOrderFactRow>> {
        let unique = unique_order_keys(keys);
        if unique.is_empty() {
            return Ok(Vec::new());
        }
        let options = FindOptions::builder()
            .sort(fact_row_batch_sort())
            .projection(super::order_fact_projection())
            .build();
        let collection = self.collection().clone_with_type::<MallOrderFactRow>();
        mongo_ops::find_many(&collection, fact_row_batch_filter(&unique), options, executor).await
    }
}

impl<'a> Repository<'a, MallPaymentSource> {
    /// 按订单 ID 集合一次批量返回支付来源（INT-R03）。
    ///
    /// # 用途
    /// 以一次 `$in` 查询替代订单列表逐行查询，消除当前页 N+1。
    ///
    /// # 参数
    /// * `self` - 支付来源仓储
    /// * `order_ids` - 当前页订单 ID 集合；为空时返回空映射，不访问数据库
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回订单 ID 字符串到支付来源列表的映射；同订单内按 `source_no` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 关键约束
    /// 空输入、去重、缺项（无来源订单无条目）由本方法保证；稳定顺序固定；
    /// 不返回 Service DTO；不开事务。
    pub async fn list_by_orders(
        &self,
        order_ids: &[MallOrderId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<MallPaymentSource>>> {
        let ids: Vec<String> = unique_strings(&order_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>());
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<MallPaymentSource> = mongo_ops::find_many(
            &self.collection(),
            payment_source_batch_filter(&ids),
            FindOptions::builder().sort(payment_source_batch_sort()).build(),
            executor,
        )
        .await?;
        let mut grouped: HashMap<String, Vec<MallPaymentSource>> = HashMap::new();
        for row in rows {
            grouped
                .entry(row.mall_order_id.to_string())
                .or_default()
                .push(row);
        }
        Ok(grouped)
    }
}

impl MallConsumptionEntryRepository<'_> {
    /// 按支付来源 ID 集合批量查询消费事实（INT-R04）。
    ///
    /// # 用途
    /// 以一次 `$in` 查询替代逐来源读取；排序合同固定，不由 Service 合并。
    ///
    /// # 参数
    /// * `self` - 消费事实只读追加仓储
    /// * `source_ids` - 支付来源 ID 集合；为空时返回空列表，不访问数据库
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重后的消费事实；排序合同为 `occurred_at` 升序，同秒按稳定 `id` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 关键约束
    /// 空集合、重复 source 去重、多来源共享结果一致；软删除排除；不开事务。
    pub async fn list_by_payment_sources(
        &self,
        source_ids: &[MallPaymentSourceId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallConsumptionEntry>> {
        let ids: Vec<String> =
            unique_strings(&source_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>());
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<MallConsumptionEntry> = mongo_ops::find_many(
            &self.collection(),
            entry_batch_filter(&ids),
            FindOptions::builder().sort(entry_batch_sort()).build(),
            executor,
        )
        .await?;
        let mut seen = HashSet::new();
        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            if seen.insert(row.base.id.clone()) {
                entries.push(row);
            }
        }
        Ok(entries)
    }
}

impl MallConsumptionCostAssessmentRepository<'_> {
    /// 按消费 ID 集合批量返回最新评估投影（INT-R05）。
    ///
    /// # 用途
    /// 以一次 `$in` 查询替代逐 entry 读取完整评估链；最新项按最大评估号聚合。
    ///
    /// # 参数
    /// * `self` - 成本评估只读追加仓储
    /// * `entry_ids` - 消费事实 ID 集合；为空时返回空映射，不访问数据库
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回消费 ID 字符串到最新评估的映射；无评估的消费无条目。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 关键约束
    /// 最新语义固定为最大 `assessment_no`，同号取稳定 ID 最大者；
    /// 不返回 Service DTO；不开事务。
    pub async fn list_latest_by_entries(
        &self,
        entry_ids: &[MallConsumptionEntryId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, MallConsumptionCostAssessment>> {
        let ids: Vec<String> = unique_strings(&entry_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>());
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let chain: Vec<MallConsumptionCostAssessment> = mongo_ops::find_many(
            &self.collection(),
            assessment_batch_filter(&ids),
            FindOptions::builder().sort(assessment_batch_sort()).build(),
            executor,
        )
        .await?;
        Ok(select_latest_assessments(chain))
    }
}

#[cfg(test)]
mod tests {
    use entities::common::time::Instant;
    use entities::ids::MallConsumptionEntryId;
    use entities::mall_order::types::CostBasis;
    use entities::mall_order::{MallConsumptionCostAssessment, MallConsumptionCostAssessmentData};

    use super::{
        assessment_batch_filter, assessment_batch_sort, entry_batch_filter, entry_batch_sort,
        fact_row_batch_filter, fact_row_batch_sort, payment_source_batch_filter, payment_source_batch_sort,
        select_latest_assessments, unique_order_keys, unique_strings,
    };
    use mongodb::bson::doc;

    /// 构造测试评估。
    ///
    /// # 参数
    /// * `id` - 评估主键
    /// * `entry` - 所属消费
    /// * `assessment_no` - 同消费递增评估号
    ///
    /// # 返回
    /// 返回链首 `NONE` 评估（评估号大于 1 时引用固定前驱）。
    fn assessment(id: &str, entry: &str, assessment_no: u32) -> MallConsumptionCostAssessment {
        MallConsumptionCostAssessment::new(
            entities::ids::MallConsumptionCostAssessmentId::new(id),
            MallConsumptionCostAssessmentData {
                mall_consumption_entry_id: MallConsumptionEntryId::new(entry),
                assessment_no,
                cost_basis: CostBasis::None,
                basis_source_type: None,
                basis_source_id: None,
                basis_source_line_id: None,
                basis_source_version: None,
                source_snapshot_hash: None,
                gross_amount: None,
                net_amount: None,
                tax_amount: None,
                tax_inclusion: None,
                input_tax_rate: None,
                delta_cost_entry_id: None,
                supersedes_assessment_id: if assessment_no == 1 {
                    None
                } else {
                    Some(entities::ids::MallConsumptionCostAssessmentId::new("ca-root"))
                },
                assessed_at: Instant::from_unix_secs(1_700_000_100),
                assessed_by: "cost-team".to_string(),
            },
        )
        .unwrap()
    }

    /// 去重：重复业务键与字符串只保留首次，空输入返回空且保序。
    #[test]
    fn unique_helpers_deduplicate_keep_order_and_allow_empty() {
        let keys = unique_order_keys(&[
            ("mall-a".to_string(), "SO-1".to_string()),
            ("mall-a".to_string(), "SO-1".to_string()),
            ("mall-b".to_string(), "SO-1".to_string()),
        ]);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], ("mall-a".to_string(), "SO-1".to_string()));
        assert!(unique_order_keys(&[]).is_empty());

        assert_eq!(
            unique_strings(&["s-1".to_string(), "s-1".to_string(), "s-2".to_string()]),
            vec!["s-1".to_string(), "s-2".to_string()]
        );
        assert!(unique_strings(&[]).is_empty());
    }

    /// 最新评估：多版本取最大评估号；无评估的消费无条目；空链返回空映射。
    #[test]
    fn select_latest_assessments_picks_max_number_per_entry() {
        let chain = vec![
            assessment("ca-1", "ce-1", 1),
            assessment("ca-2", "ce-1", 3),
            assessment("ca-3", "ce-1", 2),
            assessment("ca-4", "ce-2", 1),
        ];
        let latest = select_latest_assessments(chain);
        assert_eq!(latest.len(), 2);
        assert_eq!(latest["ce-1"].assessment_no, 3);
        assert_eq!(latest["ce-2"].assessment_no, 1);
        assert!(!latest.contains_key("ce-missing"));
        assert!(select_latest_assessments(Vec::new()).is_empty());
    }

    /// 边界序号：同号多版本取稳定 ID 最大者，与 `max assessment_no` 等价。
    #[test]
    fn select_latest_assessments_breaks_number_ties_by_stable_id() {
        let chain = vec![assessment("ca-a", "ce-1", 2), assessment("ca-b", "ce-1", 2)];
        let latest = select_latest_assessments(chain);
        assert_eq!(latest["ce-1"].base.id, "ca-b");
    }

    /// 事实批量过滤：每个分支同时限定商城与订单号，跨 mall 同订单号互不干扰；
    /// 软删除排除；排序为发生时间升序、同秒稳定 ID 升序。
    #[test]
    fn fact_row_batch_filter_isolates_malls_and_excludes_deleted() {
        let filter = fact_row_batch_filter(&[
            ("mall-a".to_string(), "SO-1".to_string()),
            ("mall-b".to_string(), "SO-1".to_string()),
        ]);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        let branches = filter.get_array("$or").unwrap();
        assert_eq!(branches.len(), 2);
        let first = branches[0].as_document().unwrap();
        assert_eq!(first.get_str("mall_id").unwrap(), "mall-a");
        assert_eq!(first.get_str("external_order_no").unwrap(), "SO-1");
        let second = branches[1].as_document().unwrap();
        assert_eq!(second.get_str("mall_id").unwrap(), "mall-b");
        assert_eq!(second.get_str("external_order_no").unwrap(), "SO-1");
        assert_eq!(fact_row_batch_sort(), doc! { "occurred_at": 1, "id": 1 });
    }

    /// 来源批量过滤：`$in` 精确匹配订单集合且排除软删除；
    /// 排序按订单分组、组内来源序号升序。
    #[test]
    fn payment_source_batch_filter_excludes_deleted_and_keeps_order_sort() {
        let ids = vec!["order-1".to_string(), "order-2".to_string()];
        let filter = payment_source_batch_filter(&ids);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        let matched = filter
            .get_document("mall_order_id")
            .unwrap()
            .get_array("$in")
            .unwrap();
        assert_eq!(matched.len(), 2);
        assert_eq!(
            payment_source_batch_sort(),
            doc! { "mall_order_id": 1, "source_no": 1 }
        );
    }

    /// 消费批量过滤：`$in` 精确匹配来源集合且排除软删除；
    /// 排序为发生时间升序、同秒稳定 ID 升序。
    #[test]
    fn entry_batch_filter_excludes_deleted_and_keeps_occurred_sort() {
        let ids = vec!["ps-1".to_string()];
        let filter = entry_batch_filter(&ids);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        let matched = filter
            .get_document("mall_payment_source_id")
            .unwrap()
            .get_array("$in")
            .unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(entry_batch_sort(), doc! { "occurred_at": 1, "id": 1 });
    }

    /// 评估批量过滤：`$in` 精确匹配消费集合且排除软删除；
    /// 排序为评估号升序、同号稳定 ID 升序。
    #[test]
    fn assessment_batch_filter_excludes_deleted_and_keeps_number_sort() {
        let ids = vec!["ce-1".to_string(), "ce-2".to_string()];
        let filter = assessment_batch_filter(&ids);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        let matched = filter
            .get_document("mall_consumption_entry_id")
            .unwrap()
            .get_array("$in")
            .unwrap();
        assert_eq!(matched.len(), 2);
        assert_eq!(assessment_batch_sort(), doc! { "assessment_no": 1, "id": 1 });
    }

    /// 空批量不得触碰执行器的断言执行器。
    ///
    /// 任何会话获取尝试直接 panic；空集合用例以此证明零数据库访问。
    struct NeverTouchExecutor;

    impl crate::executor::Executor for NeverTouchExecutor {
        /// 获取会话（空批量路径不可达）。
        ///
        /// # 返回
        /// 永不返回，直接 panic。
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            panic!("空批量不得访问执行器或数据库");
        }
    }

    /// 构造未连接的测试数据库句柄（懒客户端，不建立连接）。
    ///
    /// # 返回
    /// 返回指向隔离库名的数据库句柄。
    async fn unit_db() -> mongodb::Database {
        let options = mongodb::options::ClientOptions::parse("mongodb://127.0.0.1:27017")
            .await
            .expect("测试客户端选项解析失败");
        let client = mongodb::Client::with_options(options).expect("测试客户端构造失败");
        client.database("int_mall_order_read_batch_unit")
    }

    /// 空集合批量读取直接返回，不访问数据库（INT-R01/R03/R04/R05 零输入维度）。
    ///
    /// 断言执行器证明空路径零数据库访问；四个批量入口均在触及执行器前短路。
    #[tokio::test]
    async fn empty_batches_succeed_without_database_access() {
        use crate::repository::extensions::MallOrderExt;

        use super::super::{MallConsumptionCostAssessmentRepository, MallConsumptionEntryRepository};

        let db = unit_db().await;
        assert!(db
            .mall_order_facts()
            .list_fact_rows_by_order_keys(&[], &mut NeverTouchExecutor)
            .await
            .expect("空订单键批量必须成功")
            .is_empty());
        assert!(db
            .mall_payment_sources()
            .list_by_orders(&[], &mut NeverTouchExecutor)
            .await
            .expect("空订单批量必须成功")
            .is_empty());
        assert!(MallConsumptionEntryRepository::new(&db)
            .list_by_payment_sources(&[], &mut NeverTouchExecutor)
            .await
            .expect("空来源批量必须成功")
            .is_empty());
        assert!(MallConsumptionCostAssessmentRepository::new(&db)
            .list_latest_by_entries(&[], &mut NeverTouchExecutor)
            .await
            .expect("空消费批量必须成功")
            .is_empty());
    }
}
