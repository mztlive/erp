//! 域 D34 `integration_ops` 仓储：inbox_message、integration_error_task、reconciliation_difference(+_resolution)（页面：W29）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与跨集合
//! 多步骤写入入口。集合名常量统一从 `extensions::integration_ops` 的
//! `IntegrationOpsExt` 关联常量导入（单一权威来源）。
//!
//! 数据模型 §5.4/§6.21：集成表是普通表组，不实现 outbox、消息中间件或投递状态机。
//! 本域四张集合均为事实类或不可变记录：`inbox_message` 是消息契约审计真相、
//! `reconciliation_difference` 是正式差异事实（§4.5.1 不设业务软删除）、
//! `reconciliation_difference_resolution` 是只追加处理记录（不可更新、不可删除），
//! `integration_error_task` 由 `status` 状态机承载投递状态。**本域不提供任何软删除
//! 方法**（base 的泛型 `soft_delete`/`restore` 不在本域调用面暴露）。
//!
//! 筛选/行类型定义在本文件，经 `IntegrationOpsExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::common::time::Instant;
use entities::integration_ops::{
    ErrorClass, ErrorTaskStatus, InboxMessage, InboxMessageId, InboxMessageStatus, IntegrationErrorTask,
    MessageType, ReconciliationDifference, ReconciliationDifferenceId, ReconciliationDifferenceResolution,
    ResolutionAction, ResolutionType, ResultingStatus, SourceSystemId,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::IntegrationOpsExt;
use super::{regex_filter::insert_literal_regex_filter, PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

mod difference_resolution_batch;

/// `inbox_message` 集合名（单一来源：`IntegrationOpsExt` 关联常量）。
const INBOX_MESSAGES: &str = <mongodb::Database as IntegrationOpsExt>::INBOX_MESSAGES;
/// `integration_error_task` 集合名（单一来源：`IntegrationOpsExt` 关联常量）。
const INTEGRATION_ERROR_TASKS: &str = <mongodb::Database as IntegrationOpsExt>::INTEGRATION_ERROR_TASKS;

/// `inbox_message` 列表排序白名单（P2 §2.3：禁止透传任意字段名）。
const INBOX_SORT_FIELDS: &[&str] = &["created_at", "received_at", "status"];
/// `integration_error_task` 列表排序白名单。
const ERROR_TASK_SORT_FIELDS: &[&str] = &["created_at", "last_attempt_at", "status"];
/// `reconciliation_difference` 列表排序白名单。
const DIFFERENCE_SORT_FIELDS: &[&str] = &["created_at"];

/// 入站消息列表投影行（列表接口只取必要字段，禁止返回整文档；
/// 内容引用 `payload_reference` 不进入列表投影）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxMessageRow {
    /// 实体主键。
    pub id: String,
    /// 来源系统 ID。
    pub source_system_id: SourceSystemId,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 消息类型。
    pub message_type: MessageType,
    /// 业务事实键（幂等键）。
    pub business_fact_key: String,
    /// 来源契约版本。
    pub payload_schema_version: String,
    /// 消息处理状态。
    pub status: InboxMessageStatus,
    /// 来源系统发送时间。
    pub source_sent_at: Option<Instant>,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 处理完成时间。
    pub processed_at: Option<Instant>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 入站消息列表筛选条件。
#[derive(Debug, Clone)]
pub struct InboxMessageFilter {
    /// 来源系统 ID；`None` 表示不筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 消息类型；`None` 表示不筛选。
    pub message_type: Option<MessageType>,
    /// 消息处理状态；`None` 表示不筛选。
    pub status: Option<InboxMessageStatus>,
    /// 来源事件 ID 模糊匹配（字面量、忽略大小写）；`None` 表示不筛选。
    pub source_event_id: Option<String>,
    /// 接收时间下界（Unix 秒，含）；`None` 表示不筛选。
    pub received_at_from: Option<i64>,
    /// 接收时间上界（Unix 秒，含）；`None` 表示不筛选。
    pub received_at_to: Option<i64>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for InboxMessageFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(source_system_id) = &self.source_system_id {
            filter.insert("source_system_id", source_system_id.to_string());
        }
        if let Some(message_type) = self.message_type {
            filter.insert("message_type", message_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        insert_literal_regex_filter(&mut filter, "source_event_id", self.source_event_id.as_deref());
        insert_time_range(
            &mut filter,
            "received_at",
            self.received_at_from,
            self.received_at_to,
        );
        filter
    }
}

impl Pagination for InboxMessageFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 集成错误任务列表投影行（列表接口只取必要字段；解决证据文本
/// `resolution` 不进入列表投影）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrationErrorTaskRow {
    /// 实体主键。
    pub id: String,
    /// 关联的消息。
    pub message_id: Option<InboxMessageId>,
    /// 关联的业务对象。
    pub business_object_id: Option<String>,
    /// 错误分类。
    pub error_class: ErrorClass,
    /// 任务状态。
    pub status: ErrorTaskStatus,
    /// 责任角色。
    pub owner_role: Option<String>,
    /// 责任人。
    pub owner_user_id: Option<String>,
    /// 重试次数。
    pub attempt_count: u32,
    /// 最近尝试时间。
    pub last_attempt_at: Option<Instant>,
    /// 最近尝试结果（脱敏）。
    pub last_attempt_summary: Option<String>,
    /// 解决方式。
    pub resolution_type: Option<ResolutionType>,
    /// 完成时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 集成错误任务列表筛选条件。
#[derive(Debug, Clone)]
pub struct IntegrationErrorTaskFilter {
    /// 关联的消息；`None` 表示不筛选。
    pub message_id: Option<InboxMessageId>,
    /// 关联的业务对象；`None` 表示不筛选。
    pub business_object_id: Option<String>,
    /// 错误分类；`None` 表示不筛选。
    pub error_class: Option<ErrorClass>,
    /// 任务状态；`None` 表示不筛选。
    pub status: Option<ErrorTaskStatus>,
    /// 责任角色；`None` 表示不筛选。
    pub owner_role: Option<String>,
    /// 责任人模糊匹配（字面量、忽略大小写）；`None` 表示不筛选。
    pub owner_user_id: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for IntegrationErrorTaskFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(message_id) = &self.message_id {
            filter.insert("message_id", message_id.to_string());
        }
        if let Some(business_object_id) = &self.business_object_id {
            filter.insert("business_object_id", business_object_id);
        }
        if let Some(error_class) = self.error_class {
            filter.insert("error_class", error_class.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(owner_role) = &self.owner_role {
            filter.insert("owner_role", owner_role);
        }
        insert_literal_regex_filter(&mut filter, "owner_user_id", self.owner_user_id.as_deref());
        filter
    }
}

impl Pagination for IntegrationErrorTaskFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 对账差异列表投影行（正式差异事实，只读）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconciliationDifferenceRow {
    /// 实体主键。
    pub id: String,
    /// 差异对象类型。
    pub business_object_type: String,
    /// 差异对象 ID。
    pub business_object_id: String,
    /// 差异分类。
    pub difference_type: String,
    /// 左侧不可变证据引用。
    pub left_fact_reference: Option<String>,
    /// 右侧不可变证据引用。
    pub right_fact_reference: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 差异发现时间（秒级时间戳）。
    pub created_at: u64,
}

/// 对账差异列表筛选条件。
#[derive(Debug, Clone)]
pub struct ReconciliationDifferenceFilter {
    /// 差异对象类型；`None` 表示不筛选。
    pub business_object_type: Option<String>,
    /// 差异对象 ID；`None` 表示不筛选。
    pub business_object_id: Option<String>,
    /// 差异分类；`None` 表示不筛选。
    pub difference_type: Option<String>,
    /// 发现时间下界（Unix 秒，含）；`None` 表示不筛选。
    pub created_at_from: Option<i64>,
    /// 发现时间上界（Unix 秒，含）；`None` 表示不筛选。
    pub created_at_to: Option<i64>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ReconciliationDifferenceFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(business_object_type) = &self.business_object_type {
            filter.insert("business_object_type", business_object_type);
        }
        if let Some(business_object_id) = &self.business_object_id {
            filter.insert("business_object_id", business_object_id);
        }
        if let Some(difference_type) = &self.difference_type {
            filter.insert("difference_type", difference_type);
        }
        insert_time_range(
            &mut filter,
            "created_at",
            self.created_at_from,
            self.created_at_to,
        );
        filter
    }
}

impl Pagination for ReconciliationDifferenceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 差异解决记录历史投影行（不可变追加记录，只读）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionHistoryRow {
    /// 实体主键。
    pub id: String,
    /// 递增处理序号。
    pub resolution_no: u32,
    /// 解决动作。
    pub resolution_action: ResolutionAction,
    /// 动作后的派生状态。
    pub resulting_status: ResultingStatus,
    /// 终态证据引用。
    pub evidence_reference: Option<String>,
    /// 处理人。
    pub handled_by: String,
    /// 处理时间。
    pub handled_at: Instant,
}

impl<'a> Repository<'a, InboxMessage> {
    /// 按「来源系统 + 来源事件 ID」查找已接收消息（消息层去重判定）。
    ///
    /// 消息层唯一性由 `uk_inbox_messages_identity` 唯一索引保证；本方法用于
    /// 去重判定与幂等读取，服务层不得做「先查后插」的重复性判断（§8.4 第 3 条）。
    ///
    /// # 参数
    /// * `source_system_id` - 来源系统 ID
    /// * `source_event_id` - 来源事件 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的已接收消息；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_identity(
        &self,
        source_system_id: &SourceSystemId,
        source_event_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<InboxMessage>> {
        self.find_one(
            doc! {
                "source_system_id": source_system_id.to_string(),
                "source_event_id": source_event_id,
            },
            executor,
        )
        .await
    }

    /// 按「消息类型 + 业务事实键」查找已接收消息（业务事实去重判定）。
    ///
    /// 业务事实键幂等由 `uk_inbox_messages_business_fact` 唯一索引保证：同一事实
    /// 来自实时与回填时只形成一份正式记录（§6.21）；`business_fact_key` 实体层
    /// 强制非空，唯一索引可直接建在字段上。
    ///
    /// # 参数
    /// * `message_type` - 消息类型（事实类型）
    /// * `business_fact_key` - 业务事实键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的已接收消息；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_business_fact_key(
        &self,
        message_type: MessageType,
        business_fact_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<InboxMessage>> {
        self.find_one(
            doc! {
                "message_type": message_type.as_str(),
                "business_fact_key": business_fact_key,
            },
            executor,
        )
        .await
    }

    /// 分页检索已接收消息列表（投影查询）。
    ///
    /// 只返回 [`InboxMessageRow`] 所需的列表字段，不加载整文档
    /// （内容引用 `payload_reference` 不进入列表投影）；排序字段走白名单
    /// （P2 §2.3，白名单外字段回退 `created_at` 降序）。
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
    pub async fn search_inbox_messages(
        &self,
        filter: &InboxMessageFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<InboxMessageRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                INBOX_SORT_FIELDS,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(inbox_message_projection())
            .build();
        let collection = self.collection().clone_with_type::<InboxMessageRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, IntegrationErrorTask> {
    /// 按稳定 ID 读取 W29 集成异常对象。
    ///
    /// 工作项入口的历史名称；纯主键读取，直接委托基类单条查询。
    ///
    /// # 参数
    /// * `id` - 集成异常任务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未删除集成异常对象；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的集成错误任务集合，不访问入站消息集合。
    pub async fn find_work_item_integration_error_task(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<IntegrationErrorTask>> {
        self.find_by_id(id, executor).await
    }

    /// 分页检索错误任务列表（投影查询）。
    ///
    /// 只返回 [`IntegrationErrorTaskRow`] 所需的列表字段，不加载整文档
    /// （解决证据文本 `resolution` 不进入列表投影）；排序字段走白名单。
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
    pub async fn search_error_tasks(
        &self,
        filter: &IntegrationErrorTaskFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<IntegrationErrorTaskRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                ERROR_TASK_SORT_FIELDS,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(integration_error_task_projection())
            .build();
        let collection = self.collection().clone_with_type::<IntegrationErrorTaskRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, ReconciliationDifference> {
    /// 按稳定 ID 读取 W29 对账差异对象。
    ///
    /// 工作项入口的历史名称；纯主键读取，直接委托基类单条查询。
    ///
    /// # 参数
    /// * `id` - 对账差异 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未删除对账差异；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的对账差异集合，不访问解决记录集合。
    pub async fn find_work_item_reconciliation_difference(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReconciliationDifference>> {
        self.find_by_id(id, executor).await
    }

    /// 分页检索对账差异列表（投影查询）。
    ///
    /// 只返回 [`ReconciliationDifferenceRow`] 所需的列表字段，不加载整文档；
    /// 排序字段走白名单（仅 `created_at`，差异发现时间）。
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
    pub async fn search_differences(
        &self,
        filter: &ReconciliationDifferenceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ReconciliationDifferenceRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                DIFFERENCE_SORT_FIELDS,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(reconciliation_difference_projection())
            .build();
        let collection = self.collection().clone_with_type::<ReconciliationDifferenceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, ReconciliationDifferenceResolution> {
    /// 按差异 ID 读取全部解决记录（不可变追加历史，按处理序号升序）。
    ///
    /// 处理记录不可更新或删除（§6.21），只提供追加与只读查询；
    /// 查询走 `(reconciliation_difference_id, resolution_no)` 唯一索引。
    ///
    /// # 参数
    /// * `difference_id` - 所属对账差异 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该差异的全部解决记录投影行，按 `resolution_no` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn search_resolutions(
        &self,
        difference_id: &ReconciliationDifferenceId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ResolutionHistoryRow>> {
        let options = FindOptions::builder()
            .sort(doc! { "resolution_no": 1 })
            .projection(resolution_history_projection())
            .build();
        let filter = doc! {
            "reconciliation_difference_id": difference_id.to_string(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        let collection = self.collection().clone_with_type::<ResolutionHistoryRow>();
        mongo_ops::find_many(&collection, filter, options, executor).await
    }

    /// 读取差异的最新一条解决记录（派生当前处理状态）。
    ///
    /// 按处理序号降序取首条，当前处理状态由最后一条处理动作派生（§6.21）。
    ///
    /// # 参数
    /// * `difference_id` - 所属对账差异 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新解决记录；尚无处理记录时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_latest_by_difference(
        &self,
        difference_id: &ReconciliationDifferenceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReconciliationDifferenceResolution>> {
        let options = FindOptions::builder()
            .sort(doc! { "resolution_no": -1 })
            .limit(1)
            .build();
        let filter = doc! {
            "reconciliation_difference_id": difference_id.to_string(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        let mut found = mongo_ops::find_many(&self.collection(), filter, options, executor).await?;
        Ok(found.pop())
    }
}

/// D34 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的跨集合
/// 原子写入入口，由 `IntegrationOpsExt::integration_ops()` 访问。
pub struct IntegrationOpsRepository<'a> {
    db: &'a Database,
}

impl<'a> IntegrationOpsRepository<'a> {
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

    /// 为消息登记错误任务并把消息置为失败（跨集合多步骤写入）。
    ///
    /// 依次写入 `integration_error_tasks` 并更新 `inbox_messages`（CAS 乐观锁），
    /// 保证「错误任务 + 消息失败标记」原子可见（§6.21）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，消息更新失败（如版本冲突）会留下只有任务没有
    /// 失败标记的半成品；Service 必须通过 `database::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `task` - 待写入的错误任务（消息类失败必填 `message_id`）
    /// * `message` - 待置为失败的消息实体（调用方须先经 `InboxMessage::update`
    ///   把状态改为 `InboxMessageStatus::Failed`）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）、消息版本冲突
    /// （透出 [`crate::Error::OptimisticLockingError`]）或 MongoDB 写入失败时返回错误。
    pub async fn create_error_task_with_message_failure(
        &self,
        task: &IntegrationErrorTask,
        message: &mut InboxMessage,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<IntegrationErrorTask>(INTEGRATION_ERROR_TASKS),
            task,
            executor,
        )
        .await?;
        Repository::new(self.db, INBOX_MESSAGES)
            .update(message, executor)
            .await?;
        Ok(())
    }
}

/// 构建排序文档（P2 §2.3：排序字段白名单化）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时回退 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 本列表查询允许的排序字段白名单
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|field| allowed.contains(field))
        .unwrap_or("created_at");
    doc! { field: direction }
}

/// 向查询条件追加秒级时间戳闭区间范围（BSON Int64 形态，与 `Instant`/`created_at`
/// 持久化形态一致；区间两端可选，任一端缺失表示不设界）。
///
/// # 参数
/// * `filter` - 待追加的查询条件
/// * `field` - 时间字段名
/// * `from` - 下界（含）；`None` 表示不设下界
/// * `to` - 上界（含）；`None` 表示不设上界
fn insert_time_range(filter: &mut Document, field: &str, from: Option<i64>, to: Option<i64>) {
    let mut range = Document::new();
    if let Some(from) = from {
        range.insert("$gte", from);
    }
    if let Some(to) = to {
        range.insert("$lte", to);
    }
    if !range.is_empty() {
        filter.insert(field, range);
    }
}

/// 入站消息列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn inbox_message_projection() -> Document {
    doc! {
        "id": 1,
        "source_system_id": 1,
        "source_event_id": 1,
        "message_type": 1,
        "business_fact_key": 1,
        "payload_schema_version": 1,
        "status": 1,
        "source_sent_at": 1,
        "received_at": 1,
        "processed_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 错误任务列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn integration_error_task_projection() -> Document {
    doc! {
        "id": 1,
        "message_id": 1,
        "business_object_id": 1,
        "error_class": 1,
        "status": 1,
        "owner_role": 1,
        "owner_user_id": 1,
        "attempt_count": 1,
        "last_attempt_at": 1,
        "last_attempt_summary": 1,
        "resolution_type": 1,
        "resolved_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 对账差异列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn reconciliation_difference_projection() -> Document {
    doc! {
        "id": 1,
        "business_object_type": 1,
        "business_object_id": 1,
        "difference_type": 1,
        "left_fact_reference": 1,
        "right_fact_reference": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 解决记录历史投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn resolution_history_projection() -> Document {
    doc! {
        "id": 1,
        "resolution_no": 1,
        "resolution_action": 1,
        "resulting_status": 1,
        "evidence_reference": 1,
        "handled_by": 1,
        "handled_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        sort_doc, InboxMessageFilter, IntegrationErrorTaskFilter, QueryFilter,
        ReconciliationDifferenceFilter, DIFFERENCE_SORT_FIELDS, ERROR_TASK_SORT_FIELDS, INBOX_SORT_FIELDS,
    };
    use entities::integration_ops::{
        ErrorClass, ErrorTaskStatus, InboxMessageStatus, MessageType, SourceSystemId,
    };

    #[test]
    fn inbox_filter_applies_optional_fields_and_time_range() {
        let filter = InboxMessageFilter {
            source_system_id: Some(SourceSystemId::new("sys-mall-1")),
            message_type: Some(MessageType::PaymentSucceeded),
            status: Some(InboxMessageStatus::Received),
            source_event_id: Some("SO-1.".to_string()),
            received_at_from: Some(1_700_000_000),
            received_at_to: Some(1_700_000_100),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("source_system_id").unwrap(), "sys-mall-1");
        assert_eq!(document.get_str("message_type").unwrap(), "PAYMENT_SUCCEEDED");
        assert_eq!(document.get_str("status").unwrap(), "received");
        assert_eq!(
            document
                .get_document("source_event_id")
                .unwrap()
                .get_str("$regex")
                .unwrap(),
            r"SO\-1\."
        );
        let range = document.get_document("received_at").unwrap();
        assert_eq!(range.get_i64("$gte").unwrap(), 1_700_000_000);
        assert_eq!(range.get_i64("$lte").unwrap(), 1_700_000_100);
    }

    #[test]
    fn error_task_filter_maps_enums_to_stable_codes() {
        let filter = IntegrationErrorTaskFilter {
            message_id: None,
            business_object_id: Some("so-1".to_string()),
            error_class: Some(ErrorClass::TransientFailure),
            status: Some(ErrorTaskStatus::AutoRetrying),
            owner_role: Some("ops".to_string()),
            owner_user_id: Some("u-1".to_string()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("error_class").unwrap(), "transient_failure");
        assert_eq!(document.get_str("status").unwrap(), "auto_retrying");
        assert_eq!(document.get_str("owner_role").unwrap(), "ops");
        assert_eq!(
            document
                .get_document("owner_user_id")
                .unwrap()
                .get_str("$regex")
                .unwrap(),
            r"u\-1"
        );
    }

    #[test]
    fn difference_filter_applies_object_key_and_time_range() {
        let filter = ReconciliationDifferenceFilter {
            business_object_type: Some("mall_order".to_string()),
            business_object_id: Some("MO-1".to_string()),
            difference_type: Some("amount_mismatch".to_string()),
            created_at_from: Some(1_700_000_000),
            created_at_to: Some(1_700_000_100),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("business_object_type").unwrap(), "mall_order");
        assert_eq!(document.get_str("business_object_id").unwrap(), "MO-1");
        assert_eq!(document.get_str("difference_type").unwrap(), "amount_mismatch");
        let range = document.get_document("created_at").unwrap();
        assert_eq!(range.get_i64("$gte").unwrap(), 1_700_000_000);
        assert_eq!(range.get_i64("$lte").unwrap(), 1_700_000_100);
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_rejects_non_whitelisted_fields() {
        assert_eq!(
            sort_doc(None, false, INBOX_SORT_FIELDS),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("received_at"), true, INBOX_SORT_FIELDS),
            doc! { "received_at": 1 }
        );
        assert_eq!(
            sort_doc(Some("payload_schema_version"), true, INBOX_SORT_FIELDS),
            doc! { "created_at": 1 },
            "白名单外字段必须回退 created_at，禁止透传"
        );
        assert_eq!(
            sort_doc(Some("last_attempt_at"), false, ERROR_TASK_SORT_FIELDS),
            doc! { "last_attempt_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("business_object_id"), false, DIFFERENCE_SORT_FIELDS),
            doc! { "created_at": -1 }
        );
    }
}
