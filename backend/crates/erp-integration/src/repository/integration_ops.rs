//! 域 D34 `integration_ops` 仓储：inbox_message、integration_error_task、reconciliation_difference(+_resolution)（页面：W29）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`persistence_core::Error::OptimisticLockingError`]）；本文件只补充域特有查询与跨集合
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
//! 筛选/行类型定义在 `filters` 子模块，经本模块再导出给 `IntegrationOpsExt`。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Repository, Result, mongo_ops};

use super::IntegrationOpsExt;
use crate::entity::integration_ops::{
    InboxMessage, IntegrationErrorTask, MessageType, ReconciliationDifference, ReconciliationDifferenceId,
    ReconciliationDifferenceResolution, SourceSystemId,
};
use crate::repository::owned::InboxMessageRepository;

mod difference_resolution_batch;
pub use difference_resolution_batch::ReconciliationDifferenceResolutionBatchExt;

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

mod filters;
/// 过滤构造器复用入口：`difference_resolution_batch` 经本模块路径复用
/// [`undeleted_base`](filters::undeleted_base)，时间区间/ID 列表/授权求交/关键词
/// 构造器（`insert_time_range`/`insert_id_in`/`and_scope`/`keyword_filter`）在
/// `filters` 内 `pub(crate)` 集中维护，三类 `to_doc` 同文件直接复用。
pub(crate) use filters::undeleted_base;
pub use filters::{
    InboxMessageFilter, InboxMessageRow, IntegrationErrorTaskFilter, IntegrationErrorTaskRow,
    ReconciliationDifferenceFilter, ReconciliationDifferenceRow, ResolutionHistoryRow,
};

/// 入站消息集合的域特异查询。
#[allow(async_fn_in_trait)]
pub trait InboxMessageRepositoryExt {
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
    async fn find_by_identity(
        &self,
        source_system_id: &SourceSystemId,
        source_event_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<InboxMessage>>;

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
    async fn find_by_business_fact_key(
        &self,
        message_type: MessageType,
        business_fact_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<InboxMessage>>;

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
    async fn search_inbox_messages(
        &self,
        filter: &InboxMessageFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<InboxMessageRow>>;
}

impl InboxMessageRepositoryExt for Repository<'_, InboxMessage> {
    async fn find_by_identity(
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

    async fn find_by_business_fact_key(
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

    async fn search_inbox_messages(
        &self,
        filter: &InboxMessageFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<InboxMessageRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending, INBOX_SORT_FIELDS))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(inbox_message_projection())
            .build();
        let collection = self.collection().clone_with_type::<InboxMessageRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }
}

/// 集成错误任务集合的域特异查询。
#[allow(async_fn_in_trait)]
pub trait IntegrationErrorTaskRepositoryExt {
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
    async fn find_work_item_integration_error_task(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<IntegrationErrorTask>>;

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
    async fn search_error_tasks(
        &self,
        filter: &IntegrationErrorTaskFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<IntegrationErrorTaskRow>>;
}

impl IntegrationErrorTaskRepositoryExt for Repository<'_, IntegrationErrorTask> {
    async fn find_work_item_integration_error_task(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<IntegrationErrorTask>> {
        self.find_by_id(id, executor).await
    }

    async fn search_error_tasks(
        &self,
        filter: &IntegrationErrorTaskFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<IntegrationErrorTaskRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending, ERROR_TASK_SORT_FIELDS))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(integration_error_task_projection())
            .build();
        let collection = self.collection().clone_with_type::<IntegrationErrorTaskRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }
}

/// 对账差异集合的域特异查询。
#[allow(async_fn_in_trait)]
pub trait ReconciliationDifferenceRepositoryExt {
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
    async fn find_work_item_reconciliation_difference(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReconciliationDifference>>;

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
    async fn search_differences(
        &self,
        filter: &ReconciliationDifferenceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ReconciliationDifferenceRow>>;
}

impl ReconciliationDifferenceRepositoryExt for Repository<'_, ReconciliationDifference> {
    async fn find_work_item_reconciliation_difference(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReconciliationDifference>> {
        self.find_by_id(id, executor).await
    }

    async fn search_differences(
        &self,
        filter: &ReconciliationDifferenceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ReconciliationDifferenceRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending, DIFFERENCE_SORT_FIELDS))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(reconciliation_difference_projection())
            .build();
        let collection = self.collection().clone_with_type::<ReconciliationDifferenceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }
}

/// 对账差异解决记录集合的域特异查询。
#[allow(async_fn_in_trait)]
pub trait ReconciliationDifferenceResolutionRepositoryExt {
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
    async fn search_resolutions(
        &self,
        difference_id: &ReconciliationDifferenceId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ResolutionHistoryRow>>;

    /// 按历史处理人读取其参与过的差异 ID；不删除处理留痕。
    ///
    /// # 参数
    /// * `handled_by` - 历史处理人稳定 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回去重后的差异 ID。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_difference_ids_handled_by(
        &self,
        handled_by: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

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
    async fn find_latest_by_difference(
        &self,
        difference_id: &ReconciliationDifferenceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReconciliationDifferenceResolution>>;
}

impl ReconciliationDifferenceResolutionRepositoryExt for Repository<'_, ReconciliationDifferenceResolution> {
    async fn search_resolutions(
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

    async fn find_difference_ids_handled_by(
        &self,
        handled_by: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        if handled_by.is_empty() {
            return Ok(Vec::new());
        }
        let filter = doc! {
            "handled_by": { "$in": handled_by },
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        let options =
            FindOptions::builder().projection(doc! { "reconciliation_difference_id": 1, "id": 1 }).build();
        let collection = self.collection().clone_with_type::<Document>();
        let rows = mongo_ops::find_many(&collection, filter, options, executor).await?;
        let mut ids = rows
            .into_iter()
            .filter_map(|row| row.get_str("reconciliation_difference_id").ok().map(str::to_string))
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    async fn find_latest_by_difference(
        &self,
        difference_id: &ReconciliationDifferenceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReconciliationDifferenceResolution>> {
        let options = FindOptions::builder().sort(doc! { "resolution_no": -1 }).limit(1).build();
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
    /// 失败标记的半成品；Service 必须通过 `persistence_core::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `task` - 待写入的错误任务（消息类失败必填 `message_id`）
    /// * `message` - 待置为失败的消息实体（调用方须先经 `InboxMessage::update`
    ///   把状态改为 `InboxMessageStatus::Failed`）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]）、消息版本冲突
    /// （透出 [`persistence_core::Error::OptimisticLockingError`]）或 MongoDB 写入失败时返回错误。
    pub async fn create_error_task_with_message_failure(
        &self,
        task: &IntegrationErrorTask,
        message: &mut InboxMessage,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<IntegrationErrorTask>(INTEGRATION_ERROR_TASKS),
            task,
            executor,
        )
        .await?;
        InboxMessageRepository::new(self.db, INBOX_MESSAGES).update(message, executor).await?;
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
    let field = sort_by.filter(|field| allowed.contains(field)).unwrap_or("created_at");
    doc! { field: direction, "id": direction }
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
        "owner_org_unit_id": 1,
        "completed_by": 1,
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
        "owner_user_id": 1,
        "owner_org_unit_id": 1,
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

    use super::{DIFFERENCE_SORT_FIELDS, ERROR_TASK_SORT_FIELDS, INBOX_SORT_FIELDS, sort_doc};

    #[test]
    fn sort_doc_defaults_to_created_at_and_rejects_non_whitelisted_fields() {
        assert_eq!(sort_doc(None, false, INBOX_SORT_FIELDS), doc! { "created_at": -1, "id": -1 });
        assert_eq!(
            sort_doc(Some("received_at"), true, INBOX_SORT_FIELDS),
            doc! { "received_at": 1, "id": 1 }
        );
        assert_eq!(
            sort_doc(Some("payload_schema_version"), true, INBOX_SORT_FIELDS),
            doc! { "created_at": 1, "id": 1 },
            "白名单外字段必须回退 created_at，禁止透传"
        );
        assert_eq!(
            sort_doc(Some("last_attempt_at"), false, ERROR_TASK_SORT_FIELDS),
            doc! { "last_attempt_at": -1, "id": -1 }
        );
        assert_eq!(
            sort_doc(Some("business_object_id"), false, DIFFERENCE_SORT_FIELDS),
            doc! { "created_at": -1, "id": -1 }
        );
    }
}
