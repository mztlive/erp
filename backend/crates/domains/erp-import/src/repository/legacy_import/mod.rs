//! 域 D22 `legacy_import` 仓储：legacy_import_batch、legacy_import_row、legacy_import_confirmation。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`persistence_core::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `LegacyImportExt` 关联常量取。
//!
//! 保留期语义（数据模型 §4.5.7/§6.12）：批次元数据、汇总计数、成功结果行与
//! 映射审计**长期保留**；失败行诊断与 `failure_diagnostic_file_asset_id` 按
//! 30 天清理（TTL 索引见 `indexes::legacy_import`）。
//! `legacy_import_confirmation` 是正式确认事实：append-only、无业务软删除
//! （§4.5），本文件不提供任何软删除入口。
//!
//! 筛选/行类型定义在本文件，经 `LegacyImportExt` 的关联类型对外暴露。
//!
//! - `apply_scope`：按请求行 ID 且受批次约束的应用范围读取（INT-R29）。

mod apply_scope;
mod failed_retry;
mod projected;
mod supersede_batch;

pub use apply_scope::{LegacyImportApplyScope, LegacyImportRowApplyScopeExt};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::BusinessDate;
pub use failed_retry::LegacyImportRowFailedRetryExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter, mongo_ops,
};
use projected::{
    legacy_import_batch_projection, legacy_import_confirmation_projection, legacy_import_row_projection,
    search_projected_page, sort_doc,
};
use serde::{Deserialize, Serialize};
pub use supersede_batch::LegacyImportConfirmationSupersedeBatchExt;

use super::extensions::LegacyImportExt;
use crate::entity::legacy_import::{
    ConfirmationDecision, ConfirmationStatus, ImportStatus, LegacyImportBatch, LegacyImportBatchId,
    LegacyImportBatchStatus, LegacyImportConfirmation, LegacyImportRow, MappingStatus, ParseStatus,
};
/// `legacy_import_batch` 集合名（单一来源：`LegacyImportExt` 关联常量）。
const LEGACY_IMPORT_BATCHES: &str = <mongodb::Database as LegacyImportExt>::LEGACY_IMPORT_BATCHES;
/// `legacy_import_row` 集合名（单一来源：`LegacyImportExt` 关联常量）。
const LEGACY_IMPORT_ROWS: &str = <mongodb::Database as LegacyImportExt>::LEGACY_IMPORT_ROWS;
/// 导入批次列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyImportBatchRow {
    /// 实体主键。
    pub id: String,
    /// 导入批次号。
    pub batch_no: String,
    /// 来源系统。
    pub source_system_id: erp_core::ids::SourceSystemId,
    /// 本批来源对象集合。
    pub source_object_set: String,
    /// 期初业务基准日。
    pub baseline_date: BusinessDate,
    /// 导入规则版本。
    pub import_rule_version: String,
    /// 批次状态。
    pub status: LegacyImportBatchStatus,
    /// 处理统计：总行数。
    pub total_rows: u64,
    /// 处理统计：成功行数。
    pub success_rows: u64,
    /// 处理统计：失败行数。
    pub failed_rows: u64,
    /// 脱敏错误码摘要。
    pub failure_code_summary: Option<String>,
    /// 确认状态派生摘要。
    pub confirmation_status_summary: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 导入批次列表筛选条件。
#[derive(Debug, Clone)]
pub struct LegacyImportBatchFilter {
    /// 批次号模糊匹配（字面量、忽略大小写）；`None` 表示不筛选。
    pub batch_no: Option<String>,
    /// 来源系统；`None` 表示不筛选。
    pub source_system_id: Option<erp_core::ids::SourceSystemId>,
    /// 批次状态；`None` 表示不筛选。
    pub status: Option<LegacyImportBatchStatus>,
    /// 期初基准日起（含）；`None` 表示不限。
    pub baseline_date_from: Option<BusinessDate>,
    /// 期初基准日止（含）；`None` 表示不限。
    pub baseline_date_to: Option<BusinessDate>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`batch_no`、`baseline_date`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for LegacyImportBatchFilter {
    // 为未指定过滤的批次列表提供第 `1` 页、每页 `20` 条的起始条件。
    fn default() -> Self {
        Self {
            batch_no: None,
            source_system_id: None,
            status: None,
            baseline_date_from: None,
            baseline_date_to: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for LegacyImportBatchFilter {
    // 构造未删除批次的查询谓词：批次号使用忽略大小写的字面量匹配，来源系统和状态精确匹配。基准日期按 `YYYY-MM-DD` 使用含端点的 `$gte`/`$lte`，只给一端则只写对应边界；缺省字段及分页、排序不进入谓词。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "batch_no", self.batch_no.as_deref());
        if let Some(source_system_id) = &self.source_system_id {
            filter.insert("source_system_id", source_system_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }

        match (self.baseline_date_from, self.baseline_date_to) {
            (Some(from), Some(to)) => {
                filter.insert("baseline_date", doc! { "$gte": from.to_string(), "$lte": to.to_string() });
            },
            (Some(from), None) => {
                filter.insert("baseline_date", doc! { "$gte": from.to_string() });
            },
            (None, Some(to)) => {
                filter.insert("baseline_date", doc! { "$lte": to.to_string() });
            },
            (None, None) => {},
        }
        filter
    }
}

impl Pagination for LegacyImportBatchFilter {
    // 将仓储列表的页码原样交给分页层，条数扩为 `u64`，不把 `0` 归一为第一页。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 导入批次集合仓储扩展：投影列表与按批次号定位。
#[allow(async_fn_in_trait)]
pub trait LegacyImportBatchRepositoryExt {
    /// 用调用方执行器按相同条件取批次列表投影及总数，不加载资产引用等内部字段。排序字段按白名单映射，默认 `created_at`；查询、游标或计数失败返回仓储错误。
    async fn search_legacy_import_batches(
        &self,
        filter: &LegacyImportBatchFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<LegacyImportBatchRow>>;

    /// 用全局唯一批次号精确查找未删除批次，供重跑定位和幂等判断；无匹配返回 `None`，MongoDB 查询失败时返回对应错误。
    async fn find_by_batch_no(
        &self,
        batch_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<LegacyImportBatch>>;
}

impl LegacyImportBatchRepositoryExt for persistence_core::Repository<'_, LegacyImportBatch> {
    async fn search_legacy_import_batches(
        &self,
        filter: &LegacyImportBatchFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<LegacyImportBatchRow>> {
        search_projected_page(
            &self.collection(),
            filter.to_doc(),
            sort_doc(filter.sort_by.as_deref(), filter.sort_ascending),
            filter.skip(),
            filter.limit(),
            legacy_import_batch_projection(),
            executor,
        )
        .await
    }

    async fn find_by_batch_no(
        &self,
        batch_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<LegacyImportBatch>> {
        self.find_one(
            doc! {
                "batch_no": batch_no,
            },
            executor,
        )
        .await
    }
}

/// 导入行列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyImportRowRow {
    /// 实体主键。
    pub id: String,
    /// 所属导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 来源对象类型。
    pub source_object_type: String,
    /// 批次内来源行身份。
    pub source_row_key: String,
    /// 解析状态。
    pub parse_status: ParseStatus,
    /// 映射状态。
    pub mapping_status: MappingStatus,
    /// 导入状态。
    pub import_status: ImportStatus,
    /// 来源稳定身份。
    pub external_identity_map_id: Option<erp_core::ids::ExternalIdentityMapId>,
    /// 失败原因错误码。
    pub error_code: Option<String>,
    /// 成功结果目标单据 ID。
    pub target_document_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 导入行列表筛选条件。
#[derive(Debug, Clone)]
pub struct LegacyImportRowFilter {
    /// 所属导入批次；`None` 表示不筛选。
    pub batch_id: Option<LegacyImportBatchId>,
    /// 解析状态；`None` 表示不筛选。
    pub parse_status: Option<ParseStatus>,
    /// 映射状态；`None` 表示不筛选。
    pub mapping_status: Option<MappingStatus>,
    /// 导入状态；`None` 表示不筛选。
    pub import_status: Option<ImportStatus>,
    /// 来源行键模糊匹配（字面量、忽略大小写）；`None` 表示不筛选。
    pub source_row_key: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`source_row_key`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for LegacyImportRowFilter {
    // 构造未删除行的 MongoDB 条件：批次和解析、映射、导入状态精确匹配，来源行键使用忽略大小写的字面量匹配；缺省、分页和排序项不进入谓词。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(batch_id) = &self.batch_id {
            filter.insert("batch_id", batch_id.to_string());
        }
        if let Some(parse_status) = self.parse_status {
            filter.insert("parse_status", parse_status.as_str());
        }
        if let Some(mapping_status) = self.mapping_status {
            filter.insert("mapping_status", mapping_status.as_str());
        }
        if let Some(import_status) = self.import_status {
            filter.insert("import_status", import_status.as_str());
        }
        insert_literal_regex_filter(&mut filter, "source_row_key", self.source_row_key.as_deref());
        filter
    }
}

impl Pagination for LegacyImportRowFilter {
    // 为导入行列表交出保存的页码和转为 `u64` 的条数；不改写 `0` 页码。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 导入行集合仓储扩展：投影列表与按批次批量读取。
#[allow(async_fn_in_trait)]
pub trait LegacyImportRowRepositoryExt {
    /// 返回导入行分页投影及同条件总数，不加载最大 64 KB 的 `normalized_payload_reference`。查询、游标或计数失败时返回仓储错误。
    async fn search_legacy_import_rows(
        &self,
        filter: &LegacyImportRowFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<LegacyImportRowRow>>;

    /// 用一次 `$in` 查询取得多个批次中未删除的导入行，避免逐批加载；查询或游标读取失败直接返回。
    async fn find_rows_by_batch_ids(
        &self,
        batch_ids: &[LegacyImportBatchId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<LegacyImportRow>>;
}

impl LegacyImportRowRepositoryExt for persistence_core::Repository<'_, LegacyImportRow> {
    async fn search_legacy_import_rows(
        &self,
        filter: &LegacyImportRowFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<LegacyImportRowRow>> {
        search_projected_page(
            &self.collection(),
            filter.to_doc(),
            sort_doc(filter.sort_by.as_deref(), filter.sort_ascending),
            filter.skip(),
            filter.limit(),
            legacy_import_row_projection(),
            executor,
        )
        .await
    }

    async fn find_rows_by_batch_ids(
        &self,
        batch_ids: &[LegacyImportBatchId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<LegacyImportRow>> {
        let keys: Vec<mongodb::bson::Bson> = batch_ids.iter().map(|id| id.to_string().into()).collect();
        self.find_many_sorted(
            doc! {
                "batch_id": { "$in": keys },
            },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }
}

/// 导入确认列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyImportConfirmationRow {
    /// 实体主键。
    pub id: String,
    /// 所属导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 责任范围。
    pub confirmation_scope: String,
    /// 责任角色。
    pub owner_role: String,
    /// 本次确认针对的批次版本。
    pub batch_version: u32,
    /// 本次确认针对的试算版本。
    pub trial_version: u32,
    /// 确认状态。
    pub status: ConfirmationStatus,
    /// 确认决策。
    pub decision: Option<ConfirmationDecision>,
    /// 退回原因代码。
    pub reason_code: Option<String>,
    /// 对应 `IMPORT_BUSINESS_CONFIRMATION` 正式任务。
    pub work_item_id: erp_core::ids::WorkItemId,
    /// 实际确认或退回人。
    pub decided_by: Option<String>,
    /// 实际确认或退回时间。
    pub decided_at: Option<u64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 导入确认列表筛选条件。
#[derive(Debug, Clone)]
pub struct LegacyImportConfirmationFilter {
    /// 所属导入批次；`None` 表示不筛选。
    pub batch_id: Option<LegacyImportBatchId>,
    /// 责任范围；`None` 表示不筛选。
    pub confirmation_scope: Option<String>,
    /// 确认状态；`None` 表示不筛选。
    pub status: Option<ConfirmationStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`trial_version`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for LegacyImportConfirmationFilter {
    // 构造确认列表的 MongoDB 条件，只读未删除记录；批次、责任范围和状态按传入值精确匹配，缺省值及分页、排序字段不参与匹配。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(batch_id) = &self.batch_id {
            filter.insert("batch_id", batch_id.to_string());
        }
        if let Some(confirmation_scope) = &self.confirmation_scope {
            filter.insert("confirmation_scope", confirmation_scope);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for LegacyImportConfirmationFilter {
    // 向确认列表分页交出原页码和扩为 `u64` 的条数，不把 `0` 改写为第一页。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 导入确认集合仓储扩展：投影列表与确认事实定位。
#[allow(async_fn_in_trait)]
pub trait LegacyImportConfirmationRepositoryExt {
    /// 只读确认列表所需的投影字段和同条件总数，不改变追加式确认事实或增加删除入口；查询、游标或计数失败时返回仓储错误。
    async fn search_legacy_import_confirmations(
        &self,
        filter: &LegacyImportConfirmationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<LegacyImportConfirmationRow>>;

    /// 用唯一 `work_item_id` 定位 `IMPORT_BUSINESS_CONFIRMATION` 正式任务对应的确认事实；没有匹配返回 `None`，读失败则返回仓储错误。
    async fn find_by_work_item(
        &self,
        work_item_id: &erp_core::ids::WorkItemId,
        executor: &mut dyn Executor,
    ) -> Result<Option<LegacyImportConfirmation>>;

    /// 凭 `(batch_id, confirmation_scope, trial_version)` 的唯一组合定位确认，供矩阵幂等检查及退回后创建新试算确认时查重。无记录返回 `None`，查询失败直接返回。
    async fn find_by_batch_scope_trial(
        &self,
        batch_id: &LegacyImportBatchId,
        confirmation_scope: &str,
        trial_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<LegacyImportConfirmation>>;

    /// 按创建顺序稳定读取批次的全部未删除确认，以便组装矩阵；查询或游标读取失败时返回仓储错误。
    async fn list_by_batch(
        &self,
        batch_id: &LegacyImportBatchId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<LegacyImportConfirmation>>;
}

impl LegacyImportConfirmationRepositoryExt for persistence_core::Repository<'_, LegacyImportConfirmation> {
    async fn search_legacy_import_confirmations(
        &self,
        filter: &LegacyImportConfirmationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<LegacyImportConfirmationRow>> {
        search_projected_page(
            &self.collection(),
            filter.to_doc(),
            sort_doc(filter.sort_by.as_deref(), filter.sort_ascending),
            filter.skip(),
            filter.limit(),
            legacy_import_confirmation_projection(),
            executor,
        )
        .await
    }

    async fn find_by_work_item(
        &self,
        work_item_id: &erp_core::ids::WorkItemId,
        executor: &mut dyn Executor,
    ) -> Result<Option<LegacyImportConfirmation>> {
        self.find_one(
            doc! {
                "work_item_id": work_item_id.to_string(),
            },
            executor,
        )
        .await
    }

    async fn find_by_batch_scope_trial(
        &self,
        batch_id: &LegacyImportBatchId,
        confirmation_scope: &str,
        trial_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<LegacyImportConfirmation>> {
        self.find_one(
            doc! {
                "batch_id": batch_id.to_string(),
                "confirmation_scope": confirmation_scope,
                "trial_version": trial_version,
            },
            executor,
        )
        .await
    }

    async fn list_by_batch(
        &self,
        batch_id: &LegacyImportBatchId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<LegacyImportConfirmation>> {
        self.find_many_sorted(doc! { "batch_id": batch_id.to_string() }, doc! { "created_at": 1 }, executor)
            .await
    }
}

/// D22 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `LegacyImportExt::legacy_import()` 访问。
pub struct LegacyImportRepository<'a> {
    db: &'a Database,
}

impl<'a> LegacyImportRepository<'a> {
    /// 绑定导入领域仓储所用数据库，供批次与行的跨集合写入共用。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 先写批次再批量写导入行。调用方须保证行属于该批次，并通过 `Transactional::with_transaction` 传入原事务执行器；使用 `NoTransaction` 时，行写入失败可能留下只有批次的半成品。批次或行写入失败返回对应仓储错误，包括 [`persistence_core::Error::DuplicateKey`]。
    pub async fn create_batch_with_rows(
        &self,
        batch: &LegacyImportBatch,
        rows: &[LegacyImportRow],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<LegacyImportBatch>(LEGACY_IMPORT_BATCHES),
            batch,
            executor,
        )
        .await?;

        mongo_ops::insert_many(&self.db.collection::<LegacyImportRow>(LEGACY_IMPORT_ROWS), rows, executor)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use mongodb::bson::doc;
    use persistence_core::QueryFilter;

    use super::{LegacyImportBatchFilter, sort_doc};

    #[test]
    fn batch_filter_applies_optional_fields_and_deleted_filter() {
        let filter = LegacyImportBatchFilter {
            batch_no: Some("IMP-2026-001".to_string()),
            source_system_id: Some(erp_core::ids::SourceSystemId::new("sys-mall")),
            status: Some(crate::entity::legacy_import::LegacyImportBatchStatus::Completed),
            baseline_date_from: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            baseline_date_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            ..Default::default()
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("source_system_id").unwrap(), "sys-mall");
        assert_eq!(document.get_str("status").unwrap(), "completed");

        let range = document.get_document("baseline_date").unwrap();
        assert_eq!(range.get_str("$gte").unwrap(), "2026-01-01");
        assert_eq!(range.get_str("$lte").unwrap(), "2026-12-31");
    }

    #[test]
    fn sort_doc_whitelists_known_fields_and_defaults_otherwise() {
        assert_eq!(sort_doc(Some("baseline_date"), true), doc! { "baseline_date": 1 });
        assert_eq!(sort_doc(Some("trial_version"), false), doc! { "trial_version": -1 });
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        // 非法字段在测试构型触发 `debug_assert`（误传即暴露），发布构型保留回退；
        // 此处不再以非法字段断言回退行为。
    }
}
