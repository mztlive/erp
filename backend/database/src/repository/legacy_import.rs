//! 域 D22 `legacy_import` 仓储：legacy_import_batch、legacy_import_row、legacy_import_confirmation。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `LegacyImportExt` 关联常量取。
//!
//! 保留期语义（数据模型 §4.5.7/§6.12）：批次元数据、汇总计数、成功结果行与
//! 映射审计**长期保留**；失败行诊断与 `failure_diagnostic_file_asset_id` 按
//! 30 天清理（TTL 索引见 `indexes::legacy_import`）。
//! `legacy_import_confirmation` 是正式确认事实：append-only、无业务软删除
//! （§4.5），本文件不提供任何软删除入口。
//!
//! 筛选/行类型定义在本文件，经 `LegacyImportExt` 的关联类型对外暴露。

use entities::common::time::BusinessDate;
use entities::legacy_import::{
    ConfirmationDecision, ConfirmationStatus, ImportStatus, LegacyImportBatch, LegacyImportBatchId,
    LegacyImportBatchStatus, LegacyImportConfirmation, LegacyImportRow, MappingStatus, ParseStatus,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::LegacyImportExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};
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
    pub source_system_id: entities::ids::SourceSystemId,
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
    pub source_system_id: Option<entities::ids::SourceSystemId>,
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

impl QueryFilter for LegacyImportBatchFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
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
                filter.insert(
                    "baseline_date",
                    doc! { "$gte": from.to_string(), "$lte": to.to_string() },
                );
            }
            (Some(from), None) => {
                filter.insert("baseline_date", doc! { "$gte": from.to_string() });
            }
            (None, Some(to)) => {
                filter.insert("baseline_date", doc! { "$lte": to.to_string() });
            }
            (None, None) => {}
        }
        filter
    }
}

impl Pagination for LegacyImportBatchFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, LegacyImportBatch> {
    /// 分页检索导入批次列表（投影查询）。
    ///
    /// 只返回 [`LegacyImportBatchRow`] 所需的列表字段，不加载整文档
    /// （资产引用等内部字段不进入列表投影）；排序字段经白名单映射，
    /// 非法字段回退默认 `created_at`。
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
    pub async fn search_legacy_import_batches(
        &self,
        filter: &LegacyImportBatchFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<LegacyImportBatchRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(legacy_import_batch_projection())
            .build();
        let collection = self.collection().clone_with_type::<LegacyImportBatchRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按批次号精确查找导入批次。
    ///
    /// 唯一性由 `uk_legacy_import_batches_batch_no` 唯一索引保证
    /// （§6.12：`batch_no` 唯一），本方法用于重跑定位与幂等判定。
    ///
    /// # 参数
    /// * `batch_no` - 导入批次号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除批次；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_batch_no(
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

    /// 按「HMAC + 对象集合 + 基准日」查找重复导入预警候选批次。
    ///
    /// 数据模型 §6.12：`source_file_hmac + source_object_set + baseline_date`
    /// 用于重复导入预警；本方法按创建时间倒序返回全部历史候选，
    /// 预警判定由 Service 完成。
    ///
    /// # 参数
    /// * `source_object_set` - 本批来源对象集合
    /// * `baseline_date` - 期初业务基准日
    /// * `source_file_hmac` - 受控临时区计算的 keyed HMAC
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除批次（按创建时间倒序）。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_reimport_warning_candidates(
        &self,
        source_object_set: &str,
        baseline_date: BusinessDate,
        source_file_hmac: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<LegacyImportBatch>> {
        let filter = doc! {
            "source_object_set": source_object_set,
            "baseline_date": baseline_date.to_string(),
            "source_file_hmac": source_file_hmac,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        self.find_many_sorted(filter, doc! { "created_at": -1 }, executor)
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
    pub external_identity_map_id: Option<entities::ids::ExternalIdentityMapId>,
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
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
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
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, LegacyImportRow> {
    /// 分页检索导入行列表（投影查询）。
    ///
    /// 只返回 [`LegacyImportRowRow`] 所需的列表字段；规范化载荷
    /// （`normalized_payload_reference`，最大 64KB）不进入列表投影。
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
    pub async fn search_legacy_import_rows(
        &self,
        filter: &LegacyImportRowFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<LegacyImportRowRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(legacy_import_row_projection())
            .build();
        let collection = self.collection().clone_with_type::<LegacyImportRowRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「批次 + 对象类型 + 来源行键」精确查找导入行。
    ///
    /// 唯一性由 `uk_legacy_import_rows_batch_identity` 唯一索引保证
    /// （§6.12：`(batch_id, source_object_type, source_row_key)` 唯一），
    /// 用于重跑幂等判定，服务层不得做「先查后插」的重复性判断。
    ///
    /// # 参数
    /// * `batch_id` - 所属导入批次
    /// * `source_object_type` - 来源对象类型
    /// * `source_row_key` - 批次内来源行身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除导入行；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_row_by_identity(
        &self,
        batch_id: &LegacyImportBatchId,
        source_object_type: &str,
        source_row_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<LegacyImportRow>> {
        self.find_one(
            doc! {
                "batch_id": batch_id.to_string(),
                "source_object_type": source_object_type,
                "source_row_key": source_row_key,
            },
            executor,
        )
        .await
    }

    /// 按批次 ID 批量取回导入行（`$in` 一次取回，避免 N+1）。
    ///
    /// # 参数
    /// * `batch_ids` - 目标批次 ID 列表
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的未删除导入行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_rows_by_batch_ids(
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

    /// 统计指定批次内的未删除导入行数。
    ///
    /// # 参数
    /// * `batch_id` - 目标批次
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配行数。
    ///
    /// # 错误
    /// 当 MongoDB 统计失败时返回错误。
    pub async fn count_rows_by_batch(
        &self,
        batch_id: &LegacyImportBatchId,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        mongo_ops::count_documents(
            &self.collection(),
            doc! {
                "batch_id": batch_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
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
    pub work_item_id: entities::ids::WorkItemId,
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
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
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
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, LegacyImportConfirmation> {
    /// 分页检索导入确认事实列表（投影查询）。
    ///
    /// 只返回 [`LegacyImportConfirmationRow`] 所需的列表字段；确认事实
    /// append-only（§4.5/§6.12），本查询不设业务软删除过滤语义之外的
    /// 任何删除入口。
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
    pub async fn search_legacy_import_confirmations(
        &self,
        filter: &LegacyImportConfirmationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<LegacyImportConfirmationRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(legacy_import_confirmation_projection())
            .build();
        let collection = self.collection().clone_with_type::<LegacyImportConfirmationRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按正式任务查找确认事实。
    ///
    /// 唯一性由 `uk_legacy_import_confirmations_work_item` 唯一索引保证
    /// （§6.12：`work_item_id` 唯一）。
    ///
    /// # 参数
    /// * `work_item_id` - 对应 `IMPORT_BUSINESS_CONFIRMATION` 正式任务
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的确认事实；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_work_item(
        &self,
        work_item_id: &entities::ids::WorkItemId,
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

    /// 按「批次 + 责任范围 + 试算版本」精确查找确认事实。
    ///
    /// 唯一性由 `uk_legacy_import_confirmations_scope_trial` 唯一索引保证
    /// （§6.12：`(batch_id, confirmation_scope, trial_version)` 唯一），
    /// 用于确认矩阵幂等判定与「RETURN_FOR_FIX 后产生新 trial_version」
    /// 的新事实创建前查重。
    ///
    /// # 参数
    /// * `batch_id` - 所属导入批次
    /// * `confirmation_scope` - 责任范围
    /// * `trial_version` - 试算版本
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的确认事实；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_batch_scope_trial(
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
}

/// D22 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `LegacyImportExt::legacy_import()` 访问。
pub struct LegacyImportRepository<'a> {
    db: &'a Database,
}

impl<'a> LegacyImportRepository<'a> {
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

    /// 创建导入批次并写入全部导入行（跨集合多步骤写入）。
    ///
    /// 依次写入 `legacy_import_batch` 与 `legacy_import_rows`，保证
    /// 「批次 + 来源行」原子可见（数据模型 §6.12：重跑使用原批次或明确
    /// 的修复批次并保持来源行幂等）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，行唯一索引冲突会留下只有批次没有行的
    /// 半成品；Service 必须通过 `database::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `batch` - 待写入的导入批次
    /// * `rows` - 待写入的导入行（必须属于 `batch`）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当行唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service
    /// 映射为幂等/冲突语义）或 MongoDB 写入失败时返回错误。
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
        mongo_ops::insert_many(
            &self.db.collection::<LegacyImportRow>(LEGACY_IMPORT_ROWS),
            rows.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档（排序字段白名单映射，非法字段回退 `created_at`）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("batch_no") => "batch_no",
        Some("baseline_date") => "baseline_date",
        Some("trial_version") => "trial_version",
        Some("source_row_key") => "source_row_key",
        _ => "created_at",
    };
    doc! { field: direction }
}

/// 导入批次列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn legacy_import_batch_projection() -> Document {
    doc! {
        "id": 1,
        "batch_no": 1,
        "source_system_id": 1,
        "source_object_set": 1,
        "baseline_date": 1,
        "import_rule_version": 1,
        "status": 1,
        "total_rows": 1,
        "success_rows": 1,
        "failed_rows": 1,
        "failure_code_summary": 1,
        "confirmation_status_summary": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 导入行列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn legacy_import_row_projection() -> Document {
    doc! {
        "id": 1,
        "batch_id": 1,
        "source_object_type": 1,
        "source_row_key": 1,
        "parse_status": 1,
        "mapping_status": 1,
        "import_status": 1,
        "external_identity_map_id": 1,
        "error_code": 1,
        "target_document_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 导入确认列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn legacy_import_confirmation_projection() -> Document {
    doc! {
        "id": 1,
        "batch_id": 1,
        "confirmation_scope": 1,
        "owner_role": 1,
        "batch_version": 1,
        "trial_version": 1,
        "status": 1,
        "decision": 1,
        "reason_code": 1,
        "work_item_id": 1,
        "decided_by": 1,
        "decided_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, LegacyImportBatchFilter, QueryFilter};
    use entities::common::time::BusinessDate;
    use mongodb::bson::doc;

    #[test]
    fn batch_filter_applies_optional_fields_and_deleted_filter() {
        let filter = LegacyImportBatchFilter {
            batch_no: Some("IMP-2026-001".to_string()),
            source_system_id: Some(entities::ids::SourceSystemId::new("sys-mall")),
            status: Some(entities::legacy_import::LegacyImportBatchStatus::Completed),
            baseline_date_from: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            baseline_date_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
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
        assert_eq!(
            sort_doc(Some("trial_version"), false),
            doc! { "trial_version": -1 }
        );
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("id"), false), doc! { "created_at": -1 });
    }
}
