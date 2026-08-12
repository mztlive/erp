//! 域 D14 `sales_review` 仓储：sales_order_review、procurement_confirmation(+_line)、
//! sales_change_order、sales_change_submission(+_line)、sales_change_review
//! （页面：W05、W07）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类；审批/确认/复核记录与
//! 变更提交是历史与事实类对象（数据模型 §6.5），**不提供软删除方法**。
//! 本文件只补充域特有查询与跨集合多步骤写入入口；集合名常量统一取
//! `SalesReviewExt` 关联常量（单一权威来源，conventions §4.3）。

use entities::ids::{SalesOrderId, SalesOrderSubmissionId};
use entities::sales_review::{
    ProcurementConfirmation, ProcurementConfirmationId, ProcurementConfirmationLine,
    ProcurementConfirmationStatus, SalesChangeOrder, SalesChangeOrderId, SalesChangeOrderStatus,
    SalesChangeSubmission, SalesChangeSubmissionId, SalesChangeSubmissionLine, SalesOrderReview,
    SalesReviewStage, SalesReviewStatus,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::SalesReviewExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};
/// `sales_order_review` 集合名（单一来源：`SalesReviewExt` 关联常量）。
const PROCUREMENT_CONFIRMATIONS: &str = <mongodb::Database as SalesReviewExt>::PROCUREMENT_CONFIRMATIONS;
/// `procurement_confirmation_line` 集合名。
const PROCUREMENT_CONFIRMATION_LINES: &str =
    <mongodb::Database as SalesReviewExt>::PROCUREMENT_CONFIRMATION_LINES;
/// `sales_change_order` 集合名。
const SALES_CHANGE_ORDERS: &str = <mongodb::Database as SalesReviewExt>::SALES_CHANGE_ORDERS;
/// `sales_change_submission` 集合名。
const SALES_CHANGE_SUBMISSIONS: &str = <mongodb::Database as SalesReviewExt>::SALES_CHANGE_SUBMISSIONS;
/// `sales_change_submission_line` 集合名。
const SALES_CHANGE_SUBMISSION_LINES: &str =
    <mongodb::Database as SalesReviewExt>::SALES_CHANGE_SUBMISSION_LINES;

/// 审批记录列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderReviewRow {
    /// 实体主键。
    pub id: String,
    /// 销售单。
    pub sales_order_id: String,
    /// 被审批的提交快照。
    pub submission_id: String,
    /// 审批阶段。
    pub review_stage: SalesReviewStage,
    /// 审批状态。
    pub status: SalesReviewStatus,
    /// 审批人。
    pub reviewer_id: Option<String>,
    /// 审批时间。
    pub reviewed_at: Option<u64>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 审批记录列表筛选条件。
#[derive(Debug, Clone)]
pub struct SalesOrderReviewFilter {
    /// 被审批的提交快照；`None` 表示不筛选。
    pub submission_id: Option<SalesOrderSubmissionId>,
    /// 销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 审批阶段；`None` 表示不筛选。
    pub review_stage: Option<SalesReviewStage>,
    /// 审批状态；`None` 表示不筛选。
    pub status: Option<SalesReviewStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 层白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SalesOrderReviewFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(submission_id) = &self.submission_id {
            filter.insert("submission_id", submission_id.to_string());
        }
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(review_stage) = self.review_stage {
            filter.insert("review_stage", review_stage.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SalesOrderReviewFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 采购确认列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcurementConfirmationRow {
    /// 实体主键。
    pub id: String,
    /// 被确认的销售单。
    pub sales_order_id: String,
    /// 被确认的销售提交。
    pub submission_id: String,
    /// 确认状态。
    pub status: ProcurementConfirmationStatus,
    /// 采购处理人。
    pub handled_by: Option<String>,
    /// 处理时间。
    pub handled_at: Option<u64>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购确认列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProcurementConfirmationFilter {
    /// 被确认的销售提交；`None` 表示不筛选。
    pub submission_id: Option<SalesOrderSubmissionId>,
    /// 确认状态；`None` 表示不筛选。
    pub status: Option<ProcurementConfirmationStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 层白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProcurementConfirmationFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(submission_id) = &self.submission_id {
            filter.insert("submission_id", submission_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProcurementConfirmationFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 销售变更单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesChangeOrderRow {
    /// 实体主键。
    pub id: String,
    /// 原销售单。
    pub sales_order_id: String,
    /// 发起时当前版本。
    pub base_revision_id: String,
    /// 变更类型。
    pub change_type: entities::sales_review::SalesChangeType,
    /// 变更状态。
    pub status: SalesChangeOrderStatus,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 销售变更单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SalesChangeOrderFilter {
    /// 原销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 变更状态；`None` 表示不筛选。
    pub status: Option<SalesChangeOrderStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 层白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SalesChangeOrderFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SalesChangeOrderFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SalesOrderReview> {
    /// 分页检索销售审批记录（投影查询）。
    ///
    /// 只返回 [`SalesOrderReviewRow`] 所需的列表字段，不加载整文档；排序字段由
    /// Service 层白名单校验后传入（api-contract §4）。
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
    pub async fn search_sales_order_reviews(
        &self,
        filter: &SalesOrderReviewFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesOrderReviewRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sales_order_review_projection())
            .build();
        let collection = self.collection().clone_with_type::<SalesOrderReviewRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「提交 + 审批阶段」查找审批记录。
    ///
    /// 唯一性由 `uk_sales_order_reviews_submission_stage` 唯一索引保证；销售修改
    /// 被驳回内容后旧审批改为失效、新提交从第一步开始（数据模型 §6.5）。
    ///
    /// # 参数
    /// * `submission_id` - 被审批的提交快照
    /// * `review_stage` - 审批阶段
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的审批记录；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_submission_and_stage(
        &self,
        submission_id: &SalesOrderSubmissionId,
        review_stage: SalesReviewStage,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderReview>> {
        self.find_one(
            doc! {
                "submission_id": submission_id.to_string(),
                "review_stage": review_stage.as_str(),
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, ProcurementConfirmation> {
    /// 分页检索采购二次确认（投影查询）。
    ///
    /// 只返回 [`ProcurementConfirmationRow`] 所需的列表字段，不加载整文档；
    /// 排序字段由 Service 层白名单校验后传入（api-contract §4）。
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
    pub async fn search_procurement_confirmations(
        &self,
        filter: &ProcurementConfirmationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProcurementConfirmationRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(procurement_confirmation_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProcurementConfirmationRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按提交快照查找待处理确认批次。
    ///
    /// 「同一销售提交仅一个有效确认批次」由部分唯一索引
    /// `uk_procurement_confirmations_pending_per_submission` 保证（数据模型 §6.5）。
    ///
    /// # 参数
    /// * `submission_id` - 被确认的销售提交
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回待处理确认批次；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_pending_by_submission(
        &self,
        submission_id: &SalesOrderSubmissionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProcurementConfirmation>> {
        self.find_one(
            doc! {
                "submission_id": submission_id.to_string(),
                "status": ProcurementConfirmationStatus::Pending.as_str(),
            },
            executor,
        )
        .await
    }

    /// 按销售单查找是否存在待处理采购确认。
    ///
    /// 销售单回草稿后，若仍有 `PENDING` 确认则说明已重新提交并进入新一轮采购确认，
    /// 此时不应再展示「采购驳回待销售处理」入口。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回待处理确认批次；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_pending_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProcurementConfirmation>> {
        self.find_one(
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "status": ProcurementConfirmationStatus::Pending.as_str(),
            },
            executor,
        )
        .await
    }

    /// 按销售单取最近一次驳回的采购确认（`handled_at` 降序，缺省时按 `created_at`）。
    ///
    /// 用于销售单详情在**不依赖采购队列 list 权限**的前提下，向销售暴露开放驳回
    /// 处理入口（改价重提 / 作废）。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最近驳回的确认实体；无驳回记录时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_latest_rejected_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProcurementConfirmation>> {
        let rows = self
            .find_many_sorted(
                doc! {
                    "sales_order_id": sales_order_id.to_string(),
                    "status": ProcurementConfirmationStatus::Rejected.as_str(),
                },
                doc! { "handled_at": -1, "created_at": -1 },
                executor,
            )
            .await?;
        // `handled_at`/`created_at` 降序：首条即最近驳回。
        Ok(rows.into_iter().next())
    }
}

impl<'a> Repository<'a, ProcurementConfirmationLine> {
    /// 列出确认批次的全部明细（按分行序号升序）。
    ///
    /// # 参数
    /// * `confirmation_id` - 所属确认批次
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按分行序号升序的确认明细列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_lines_by_confirmation(
        &self,
        confirmation_id: &ProcurementConfirmationId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementConfirmationLine>> {
        self.find_many_sorted(
            doc! { "procurement_confirmation_id": confirmation_id.to_string() },
            doc! { "line_no": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SalesChangeOrder> {
    /// 分页检索销售变更单（投影查询）。
    ///
    /// 只返回 [`SalesChangeOrderRow`] 所需的列表字段，不加载整文档；排序字段由
    /// Service 层白名单校验后传入（api-contract §4）。
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
    pub async fn search_sales_change_orders(
        &self,
        filter: &SalesChangeOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesChangeOrderRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sales_change_order_projection())
            .build();
        let collection = self.collection().clone_with_type::<SalesChangeOrderRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「销售单 + 基准版本」查找进行中变更单。
    ///
    /// 「同一销售单同一 `base_revision_id` 同时只能有一个进行中变更」由部分唯一
    /// 索引 `uk_sales_change_orders_active_per_order_base` 保证（数据模型 §6.5）。
    ///
    /// # 参数
    /// * `sales_order_id` - 原销售单
    /// * `base_revision_id` - 发起时当前版本
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回进行中变更单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_in_progress_by_order_and_base(
        &self,
        sales_order_id: &SalesOrderId,
        base_revision_id: &entities::sales_order::SalesOrderRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesChangeOrder>> {
        self.find_one(
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "base_revision_id": base_revision_id.to_string(),
                "status": {
                    "$in": [
                        SalesChangeOrderStatus::Draft.as_str(),
                        SalesChangeOrderStatus::PendingImpactConfirmation.as_str(),
                        SalesChangeOrderStatus::PendingFinanceReview.as_str(),
                        SalesChangeOrderStatus::Rejected.as_str(),
                    ]
                },
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SalesChangeSubmission> {
    /// 按「变更单 + 提交序号」查找变更提交。
    ///
    /// 唯一性由 `uk_sales_change_submissions_order_submission_no` 唯一索引保证；
    /// 提交头、行形成后不可更新（数据模型 §6.5）。
    ///
    /// # 参数
    /// * `sales_change_order_id` - 所属销售变更单
    /// * `submission_no` - 提交序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的变更提交；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_order_and_no(
        &self,
        sales_change_order_id: &SalesChangeOrderId,
        submission_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesChangeSubmission>> {
        self.find_one(
            doc! {
                "sales_change_order_id": sales_change_order_id.to_string(),
                "submission_no": submission_no as i32,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SalesChangeSubmissionLine> {
    /// 列出变更提交的全部明细（按行号升序）。
    ///
    /// # 参数
    /// * `submission_id` - 所属变更提交
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按行号升序的变更提交明细列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_lines_by_submission(
        &self,
        submission_id: &SalesChangeSubmissionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesChangeSubmissionLine>> {
        self.find_many_sorted(
            doc! { "sales_change_submission_id": submission_id.to_string() },
            doc! { "line_no": 1 },
            executor,
        )
        .await
    }
}

/// D14 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口（确认批次、变更提交），由 `SalesReviewExt::sales_review()`
/// 访问。
pub struct SalesReviewRepository<'a> {
    db: &'a Database,
}

impl<'a> SalesReviewRepository<'a> {
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

    /// 建立采购二次确认批次（确认头 + 分行）。
    ///
    /// 依次写入 `procurement_confirmation` 与 `procurement_confirmation_line`，
    /// 保证「确认批次 + 分行」原子可见（数据模型 §6.5）。**必须收到事务执行器**：
    /// 本方法不构成原子边界，传入 `NoTransaction` 时两笔写入各自自动提交，中途
    /// 失败会留下只有头没有分行的半成品；Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `confirmation` - 待写入的确认批次头
    /// * `lines` - 待写入的确认分行
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入失败
    /// 时返回错误。
    pub async fn create_procurement_confirmation_with_lines(
        &self,
        confirmation: &ProcurementConfirmation,
        lines: &[ProcurementConfirmationLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<ProcurementConfirmation>(PROCUREMENT_CONFIRMATIONS),
            confirmation,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<ProcurementConfirmationLine>(PROCUREMENT_CONFIRMATION_LINES),
            lines.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }

    /// 发起销售变更影响确认：写入不可变变更提交并把变更单推进到待影响确认。
    ///
    /// 依次写入 `sales_change_submission`、`sales_change_submission_line` 并 CAS
    /// 更新变更单（绑定 `current_submission_id`/`target_content_hash`，数据模型
    /// §6.5：所有复核必须引用同一个 `sales_change_submission_id`）。调用方须先
    /// 在 `SalesChangeOrder` 实体上完成 `submit_impact` 状态迁移（本层不做业务
    /// 判定）。**必须收到事务执行器**：本方法不构成原子边界，传入
    /// `NoTransaction` 时中途失败会留下没有头的明细或未推进的变更单；Service
    /// 必须通过 `with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `change_order` - 已迁移到待影响确认的变更单（成功后内存版本递增）
    /// * `submission` - 不可变变更提交头
    /// * `lines` - 不可变变更提交明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突、乐观锁冲突或 MongoDB 写入失败时返回错误。
    pub async fn submit_sales_change(
        &self,
        change_order: &mut SalesChangeOrder,
        submission: &SalesChangeSubmission,
        lines: &[SalesChangeSubmissionLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesChangeSubmission>(SALES_CHANGE_SUBMISSIONS),
            submission,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SalesChangeSubmissionLine>(SALES_CHANGE_SUBMISSION_LINES),
            lines.to_vec(),
            executor,
        )
        .await?;
        Repository::new(self.db, SALES_CHANGE_ORDERS)
            .update(change_order, executor)
            .await
    }
}

/// 构建排序文档。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { sort_by.unwrap_or("created_at"): direction }
}

/// 审批记录列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_order_review_projection() -> Document {
    doc! {
        "id": 1,
        "sales_order_id": 1,
        "submission_id": 1,
        "review_stage": 1,
        "status": 1,
        "reviewer_id": 1,
        "reviewed_at": 1,
        "created_at": 1,
    }
}

/// 采购确认列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn procurement_confirmation_projection() -> Document {
    doc! {
        "id": 1,
        "sales_order_id": 1,
        "submission_id": 1,
        "status": 1,
        "handled_by": 1,
        "handled_at": 1,
        "created_at": 1,
    }
}

/// 销售变更单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_change_order_projection() -> Document {
    doc! {
        "id": 1,
        "sales_order_id": 1,
        "base_revision_id": 1,
        "change_type": 1,
        "status": 1,
        "current_submission_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sales_order_review_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SalesOrderReviewFilter {
            submission_id: Some(entities::ids::SalesOrderSubmissionId::new("s-1")),
            sales_order_id: Some(SalesOrderId::new("o-1")),
            review_stage: Some(SalesReviewStage::SalesLeader),
            status: Some(SalesReviewStatus::Pending),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("submission_id").unwrap(), "s-1");
        assert_eq!(document.get_str("sales_order_id").unwrap(), "o-1");
        assert_eq!(document.get_str("review_stage").unwrap(), "SALES_LEADER");
        assert_eq!(document.get_str("status").unwrap(), "PENDING");
    }

    #[test]
    fn procurement_confirmation_filter_applies_submission_and_status() {
        let filter = ProcurementConfirmationFilter {
            submission_id: Some(entities::ids::SalesOrderSubmissionId::new("s-1")),
            status: Some(ProcurementConfirmationStatus::Pending),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("submission_id").unwrap(), "s-1");
        assert_eq!(document.get_str("status").unwrap(), "PENDING");
    }

    #[test]
    fn sales_change_order_filter_applies_order_and_status() {
        let filter = SalesChangeOrderFilter {
            sales_order_id: Some(SalesOrderId::new("o-1")),
            status: Some(SalesChangeOrderStatus::Draft),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("sales_order_id").unwrap(), "o-1");
        assert_eq!(document.get_str("status").unwrap(), "DRAFT");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_descending() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("status"), true), doc! { "status": 1 });
    }
}
