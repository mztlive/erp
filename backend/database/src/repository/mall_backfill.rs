//! 域 D31 `mall_backfill` 仓储：mall_consumption_backfill_job、
//! mall_consumption_backfill_item。
//!
//! 回填作业走 [`Repository`] 基类（状态推进与进度统计使用乐观锁 CAS）；
//! 回填明细是不可变执行结果（§4.5 不设业务软删除），**不提供软删除方法**：
//! 只暴露只读追加仓储。集合名常量统一从 `MallBackfillExt` 关联常量导入。
//!
//! ★ `(job_id, business_fact_key)` 去重只靠唯一索引（P2 计划 §5），本层不提供
//! 「先查后插」的查重入口；重复写入由 `uk_mall_consumption_backfill_items_key`
//! 唯一索引拒绝并透出 [`crate::Error::DuplicateKey`]。
//!
//! 筛选/行类型定义在本文件，经 `MallBackfillExt` 的关联类型对外暴露。

use entities::common::time::Instant;
use entities::mall_backfill::{
    BackfillCostBasis, BackfillItemResult, BackfillJobStatus, MallConsumptionBackfillItem,
    MallConsumptionBackfillJob,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::MallBackfillExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `mall_consumption_backfill_job` 集合名（单一来源：`MallBackfillExt` 关联常量）。
const MALL_CONSUMPTION_BACKFILL_JOBS: &str =
    <mongodb::Database as MallBackfillExt>::MALL_CONSUMPTION_BACKFILL_JOBS;
/// `mall_consumption_backfill_item` 集合名（单一来源：`MallBackfillExt` 关联常量）。
const MALL_CONSUMPTION_BACKFILL_ITEMS: &str =
    <mongodb::Database as MallBackfillExt>::MALL_CONSUMPTION_BACKFILL_ITEMS;

/// 回填作业列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallConsumptionBackfillJobRow {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub mall_id: String,
    /// 对应唯一 `T`。
    pub cutover_id: entities::ids::MallConsumptionCutoverId,
    /// 半开回填范围起点。
    pub range_start: Instant,
    /// 半开回填范围终点。
    pub range_end: Instant,
    /// 作业状态。
    pub status: BackfillJobStatus,
    /// 来源统计总笔数。
    pub total_count: u64,
    /// 来源统计总金额（Decimal128 持久化）。
    pub total_amount: entities::money::Amount,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 回填作业列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallConsumptionBackfillJobFilter {
    /// 来源商城（字面量忽略大小写模糊匹配）；`None` 表示不筛选。
    pub mall_id: Option<String>,
    /// 作业状态；`None` 表示不筛选。
    pub status: Option<BackfillJobStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`range_start`/`created_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallConsumptionBackfillJobFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "mall_id", self.mall_id.as_deref());
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for MallConsumptionBackfillJobFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 回填明细列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallConsumptionBackfillItemRow {
    /// 实体主键。
    pub id: String,
    /// 回填批次。
    pub job_id: entities::ids::MallConsumptionBackfillJobId,
    /// 事实身份。
    pub business_fact_key: String,
    /// 来源回填记录。
    pub source_event_reference: String,
    /// 形成的正式事实。
    pub mall_order_fact_id: Option<entities::ids::MallOrderFactId>,
    /// 结果类型。
    pub result: BackfillItemResult,
    /// 成本口径。
    pub cost_basis: BackfillCostBasis,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 回填明细列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallConsumptionBackfillItemFilter {
    /// 回填批次；`None` 表示不筛选。
    pub job_id: Option<entities::ids::MallConsumptionBackfillJobId>,
    /// 结果类型；`None` 表示不筛选。
    pub result: Option<BackfillItemResult>,
    /// 成本口径；`None` 表示不筛选。
    pub cost_basis: Option<BackfillCostBasis>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`business_fact_key`/`created_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallConsumptionBackfillItemFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(job_id) = &self.job_id {
            filter.insert("job_id", job_id.to_string());
        }
        if let Some(result) = self.result {
            filter.insert("result", result.as_str());
        }
        if let Some(cost_basis) = self.cost_basis {
            filter.insert("cost_basis", cost_basis.as_str());
        }
        filter
    }
}

impl Pagination for MallConsumptionBackfillItemFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, MallConsumptionBackfillJob> {
    /// 分页检索回填作业列表（投影查询）。
    ///
    /// 只返回 [`MallConsumptionBackfillJobRow`] 所需的列表字段，不加载整文档
    /// （进度统计与报告文件引用不进列表投影）；排序字段按白名单映射
    /// （非法字段回落到 `created_at`），禁止透传任意字段名。
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
    pub async fn search_backfill_jobs(
        &self,
        filter: &MallConsumptionBackfillJobFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallConsumptionBackfillJobRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                &["range_start", "created_at"],
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(backfill_job_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<MallConsumptionBackfillJobRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 查询同一商城与指定半开范围重叠的回填作业。
    ///
    /// 仅封装范围相交的持久化查询；哪些作业状态会阻断新批次由
    /// [`MallConsumptionBackfillJob::blocks_overlapping_batch`] 决定。
    ///
    /// # 参数
    /// * `mall_id` - 来源商城
    /// * `range_start` - 新范围起点（含）
    /// * `range_end` - 新范围终点（不含）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回范围相交的未删除作业。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_overlapping_for_mall(
        &self,
        mall_id: &str,
        range_start: Instant,
        range_end: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallConsumptionBackfillJob>> {
        self.find_many(
            doc! {
                "mall_id": mall_id,
                "range_start": { "$lt": range_end.unix_secs() },
                "range_end": { "$gt": range_start.unix_secs() },
            },
            executor,
        )
        .await
    }
}

/// `mall_consumption_backfill_item` 只读追加仓储（回填明细是不可变执行结果，§4.5 不设软删除）。
pub struct MallConsumptionBackfillItemRepository<'a> {
    db: &'a Database,
}

impl<'a> MallConsumptionBackfillItemRepository<'a> {
    /// 创建仓储实例。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 追加回填明细。
    ///
    /// 明细不可变（只提供 `new()`）；`(job_id, business_fact_key)` 唯一由
    /// `uk_mall_consumption_backfill_items_key` 唯一索引保证（§6.17），
    /// 与实时或其他批次重叠的重复写入透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `item` - 待追加的回填明细
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(
        &self,
        item: &MallConsumptionBackfillItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), item, executor).await
    }

    /// 按 ID 查找回填明细。
    ///
    /// # 参数
    /// * `id` - 明细主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的明细；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallConsumptionBackfillItem>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按（批次, 事实身份）查找回填明细。
    ///
    /// 唯一性由 `uk_mall_consumption_backfill_items_key` 唯一索引保证（§6.17）；
    /// 供续跑与重跑时的明细归属判定使用，不得用于先查后插去重。
    ///
    /// # 参数
    /// * `job_id` - 回填批次
    /// * `business_fact_key` - 事实身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的明细；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_job_and_key(
        &self,
        job_id: &entities::ids::MallConsumptionBackfillJobId,
        business_fact_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallConsumptionBackfillItem>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "job_id": job_id.to_string(),
                "business_fact_key": business_fact_key,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 分页检索回填明细列表（投影查询）。
    ///
    /// 只返回 [`MallConsumptionBackfillItemRow`] 所需的列表字段，不加载整文档
    /// （失败原因错误码/详情不进列表投影）；排序字段按白名单映射
    /// （非法字段回落到 `created_at`）。
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
    pub async fn search_backfill_items(
        &self,
        filter: &MallConsumptionBackfillItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallConsumptionBackfillItemRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                &["business_fact_key", "created_at"],
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(backfill_item_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<MallConsumptionBackfillItemRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallConsumptionBackfillItem> {
        self.db
            .collection::<MallConsumptionBackfillItem>(MALL_CONSUMPTION_BACKFILL_ITEMS)
    }
}

/// D31 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类与只读追加仓储；本类型只承载依赖
/// 事务的跨集合原子写入入口，由 `MallBackfillExt::mall_backfill()` 访问。
pub struct MallBackfillRepository<'a> {
    db: &'a Database,
}

impl<'a> MallBackfillRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 建立回填作业及其首发明细（跨集合多步骤写入）。
    ///
    /// 回填作业创建时随批次携带首发明细，作业与明细必须原子可见（§6.17）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时作业与明细各自自动提交，中途失败会留下只有作业没有明细的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `job` - 待写入的回填作业
    /// * `items` - 待写入的回填明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为去重语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_job_with_items(
        &self,
        job: &MallConsumptionBackfillJob,
        items: Vec<MallConsumptionBackfillItem>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<MallConsumptionBackfillJob>(MALL_CONSUMPTION_BACKFILL_JOBS),
            job,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<MallConsumptionBackfillItem>(MALL_CONSUMPTION_BACKFILL_ITEMS),
            items,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档（白名单映射）。
///
/// # 参数
/// * `sort_by` - 排序字段；不在白名单或为 `None` 时默认 `created_at`
/// * `allowed` - 允许的排序字段白名单
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, allowed: &[&str], sort_ascending: bool) -> Document {
    let field = sort_by
        .filter(|field| allowed.contains(field))
        .unwrap_or("created_at");
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction }
}

/// 回填作业列表投影字段（不含进度统计与报告引用）。
///
/// # 返回
/// 返回投影条件文档。
fn backfill_job_projection() -> Document {
    doc! {
        "id": 1,
        "mall_id": 1,
        "cutover_id": 1,
        "range_start": 1,
        "range_end": 1,
        "status": 1,
        "total_count": 1,
        "total_amount": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 回填明细列表投影字段（不含失败原因）。
///
/// # 返回
/// 返回投影条件文档。
fn backfill_item_projection() -> Document {
    doc! {
        "id": 1,
        "job_id": 1,
        "business_fact_key": 1,
        "source_event_reference": 1,
        "mall_order_fact_id": 1,
        "result": 1,
        "cost_basis": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use entities::mall_backfill::{BackfillCostBasis, BackfillItemResult, BackfillJobStatus};
    use mongodb::bson::doc;

    use super::{sort_doc, MallConsumptionBackfillItemFilter, MallConsumptionBackfillJobFilter, QueryFilter};

    #[test]
    fn backfill_job_filter_applies_optional_fields_and_deleted_filter() {
        let filter = MallConsumptionBackfillJobFilter {
            mall_id: Some("mall-a".to_string()),
            status: Some(BackfillJobStatus::Running),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("status").unwrap(), "running");
    }

    #[test]
    fn backfill_item_filter_maps_result_and_cost_basis() {
        let filter = MallConsumptionBackfillItemFilter {
            job_id: None,
            result: Some(BackfillItemResult::New),
            cost_basis: Some(BackfillCostBasis::Actual),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("result").unwrap(), "new");
        assert_eq!(document.get_str("cost_basis").unwrap(), "ACTUAL");
    }

    #[test]
    fn sort_doc_maps_only_whitelisted_fields_and_defaults_to_created_at() {
        assert_eq!(
            sort_doc(None, &["range_start", "created_at"], false),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("range_start"), &["range_start", "created_at"], true),
            doc! { "range_start": 1 }
        );
        assert_eq!(
            sort_doc(Some("malicious_field"), &["range_start", "created_at"], false),
            doc! { "created_at": -1 },
            "白名单外字段必须回落到默认排序"
        );
    }
}
