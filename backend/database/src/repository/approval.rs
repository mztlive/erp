//! 域 D03 审批定义与运行实例仓储。
//!
//! 单集合 CRUD 和 `BaseModel.version` 乐观锁复用 [`Repository`]；本文件补充
//! 发布定义解析、冻结步骤顺序、业务对象审批历史和当前步骤查询。跨集合推进、
//! 待办和正式业务事实必须由 Service 使用同一事务编排，本仓储不提供通用完成动作。

use entities::approval::{
    ApprovalDefinition, ApprovalDefinitionStatus, ApprovalInstance, ApprovalInstanceId,
    ApprovalInstanceStatus, ApprovalStepDefinition, ApprovalStepInstance,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::Database;

use super::extensions::ApprovalExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

const APPROVAL_DEFINITIONS: &str = <mongodb::Database as ApprovalExt>::APPROVAL_DEFINITIONS;
const APPROVAL_STEP_DEFINITIONS: &str = <mongodb::Database as ApprovalExt>::APPROVAL_STEP_DEFINITIONS;

/// 审批实例列表筛选条件。
#[derive(Debug, Clone, Default)]
pub struct ApprovalInstanceFilter {
    /// 实例状态；`None` 表示不筛选。
    pub status: Option<ApprovalInstanceStatus>,
    /// 稳定定义编码；`None` 表示不筛选。
    pub definition_key: Option<String>,
    /// 业务对象类型；`None` 表示不筛选。
    pub business_object_type: Option<String>,
    /// 业务对象 ID；`None` 表示不筛选。
    pub business_object_id: Option<String>,
    /// 冻结责任组织；`None` 表示不筛选。
    pub owner_organization_id: Option<String>,
    /// 被审批的提交或业务版本；`None` 表示不筛选。
    pub subject_version: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
}

impl ApprovalInstanceFilter {
    /// 构造阻塞审批实例队列筛选条件。
    ///
    /// # 参数
    /// * `page` - 页码（1 起，零按第一页处理）
    /// * `page_size` - 单页条数
    ///
    /// # 返回
    /// 返回只匹配 `BLOCKED` 实例的筛选条件。
    pub fn blocked(page: u64, page_size: u32) -> Self {
        Self {
            status: Some(ApprovalInstanceStatus::Blocked),
            page,
            page_size,
            ..Self::default()
        }
    }
}

impl QueryFilter for ApprovalInstanceFilter {
    /// 转换为 MongoDB 查询条件。
    ///
    /// # 返回
    /// 返回包含未删除约束和所有已提供精确条件的查询文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(definition_key) = &self.definition_key {
            filter.insert("definition_key", definition_key);
        }
        if let Some(business_object_type) = &self.business_object_type {
            filter.insert("business_object_type", business_object_type);
        }
        if let Some(business_object_id) = &self.business_object_id {
            filter.insert("business_object_id", business_object_id);
        }
        if let Some(owner_organization_id) = &self.owner_organization_id {
            filter.insert("owner_organization_id", owner_organization_id);
        }
        if let Some(subject_version) = &self.subject_version {
            filter.insert("subject_version", subject_version);
        }
        filter
    }
}

impl Pagination for ApprovalInstanceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ApprovalDefinition> {
    /// 按稳定编码和业务定义版本精确查询定义。
    ///
    /// 查询覆盖唯一索引 `(definition_key, definition_version)`。物理字段
    /// `definition_version` 是业务版本，`version` 保留给 `BaseModel` 乐观锁。
    ///
    /// # 参数
    /// * `definition_key` - 稳定定义编码
    /// * `definition_version` - 业务定义版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回匹配定义；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_by_key_version(
        &self,
        definition_key: &str,
        definition_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalDefinition>> {
        self.find_one(
            definition_key_version_filter(definition_key, definition_version),
            executor,
        )
        .await
    }

    /// 按稳定编码查询当前唯一已发布定义。
    ///
    /// 同一 `definition_key` 同时最多一个 `PUBLISHED` 版本由部分唯一索引保证；
    /// `start_approval` 只能使用本查询的结果。
    ///
    /// # 参数
    /// * `definition_key` - 稳定定义编码
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前已发布定义；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_published_by_key(
        &self,
        definition_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalDefinition>> {
        self.find_one(published_definition_filter(definition_key), executor)
            .await
    }
}

impl<'a> Repository<'a, ApprovalStepDefinition> {
    /// 列出审批定义版本的全部冻结步骤。
    ///
    /// 返回结果按 `sequence_no` 升序，供发布校验和实例启动一次加载，避免逐步骤 N+1 查询。
    ///
    /// # 参数
    /// * `approval_definition_id` - 审批定义版本记录 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回严格按顺序号排列的步骤定义。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_definition(
        &self,
        approval_definition_id: &entities::approval::ApprovalDefinitionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalStepDefinition>> {
        self.find_many_sorted(
            doc! { "approval_definition_id": approval_definition_id.to_string() },
            doc! { "sequence_no": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, ApprovalInstance> {
    /// 按审批定义和启动业务幂等键查询原实例。
    ///
    /// 查询包含终态历史；唯一索引 `(definition_key, start_idempotency_key)` 保证
    /// 同一业务请求永久对应同一审批实例。Service 必须再严格核对对象身份、
    /// `subject_version` 和冻结启动上下文，同键不同请求返回冲突。
    ///
    /// # 参数
    /// * `definition_key` - 稳定审批定义编码
    /// * `start_idempotency_key` - 启动业务幂等键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回原审批实例；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_by_start_idempotency_key(
        &self,
        definition_key: &str,
        start_idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalInstance>> {
        self.find_one(
            start_idempotency_filter(definition_key, start_idempotency_key),
            executor,
        )
        .await
    }

    /// 查询同一业务对象、提交版本和审批定义的当前非终态实例。
    ///
    /// 查询条件与部分唯一索引完全一致：`RUNNING` 或 `BLOCKED` 同时最多一条。
    ///
    /// # 参数
    /// * `definition_key` - 稳定审批定义编码
    /// * `business_object_type` - 业务对象类型
    /// * `business_object_id` - 业务对象 ID
    /// * `subject_version` - 不可变提交或业务版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前非终态实例；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_non_terminal_by_subject(
        &self,
        definition_key: &str,
        business_object_type: &str,
        business_object_id: &str,
        subject_version: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalInstance>> {
        self.find_one(
            non_terminal_subject_filter(
                definition_key,
                business_object_type,
                business_object_id,
                subject_version,
            ),
            executor,
        )
        .await
    }

    /// 查询同一业务对象、提交版本和审批定义的最近审批实例。
    ///
    /// 本查询包含终态历史，供服务层结合业务幂等键识别重复启动；它不替代明确的
    /// 幂等键约束。若同一组合存在历史记录，按 `started_at` 倒序返回最近一条。
    ///
    /// # 参数
    /// * `definition_key` - 稳定审批定义编码
    /// * `business_object_type` - 业务对象类型
    /// * `business_object_id` - 业务对象 ID
    /// * `subject_version` - 不可变提交或业务版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回最近审批实例；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_subject_and_definition(
        &self,
        definition_key: &str,
        business_object_type: &str,
        business_object_id: &str,
        subject_version: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalInstance>> {
        let mut filter = subject_definition_filter(
            definition_key,
            business_object_type,
            business_object_id,
            subject_version,
        );
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
        let instances = mongo_ops::find_many(
            &self.collection(),
            filter,
            mongodb::options::FindOptions::builder()
                .sort(doc! { "started_at": -1 })
                .limit(1)
                .build(),
            executor,
        )
        .await?;
        Ok(instances.into_iter().next())
    }

    /// 分页检索审批实例。
    ///
    /// 阻塞管理队列使用 [`ApprovalInstanceFilter::blocked`]；结果包含完整实体和
    /// `BaseModel.version`，API 必须把该版本映射为 `instance_version`。
    ///
    /// # 参数
    /// * `filter` - 状态、定义、对象与分页筛选条件
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按创建时间倒序的审批实例分页结果。
    ///
    /// # 错误
    /// MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_instances(
        &self,
        filter: &ApprovalInstanceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ApprovalInstance>> {
        self.search(filter, executor).await
    }

    /// 按授权组织范围分页查询阻塞审批实例。
    ///
    /// `None` 表示调用方已获公司级权限，不附加组织过滤；`Some(ids)` 将组织范围
    /// 直接写入 MongoDB 条件，禁止全量读取后隐藏；`Some(empty)` 不访问数据库并
    /// 返回空页。结果包含 `BaseModel.version`，API 映射为 `instance_version`。
    ///
    /// # 参数
    /// * `owner_organization_ids` - 可访问的冻结责任组织集合；`None` 为公司级范围
    /// * `page` - 页码（1 起，零按第一页处理）
    /// * `page_size` - 单页条数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回阻塞审批实例分页结果。
    ///
    /// # 错误
    /// MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn list_blocked(
        &self,
        owner_organization_ids: Option<&[String]>,
        page: u64,
        page_size: u32,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ApprovalInstance>> {
        if owner_organization_ids.is_some_and(<[String]>::is_empty) {
            return Ok(PageResult {
                items: Vec::new(),
                total: 0,
            });
        }

        let filter = blocked_instances_filter(owner_organization_ids);
        let pagination = ApprovalInstanceFilter::blocked(page, page_size);
        let options = mongodb::options::FindOptions::builder()
            .sort(doc! { "blocked_at": 1, "created_at": 1 })
            .skip(pagination.skip())
            .limit(pagination.limit())
            .build();
        let items =
            crate::mongo_ops::find_many(&self.collection(), filter.clone(), options, executor).await?;
        let total = crate::mongo_ops::count_documents(&self.collection(), filter, executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, ApprovalStepInstance> {
    /// 列出审批实例的全部步骤实例。
    ///
    /// 返回结果按冻结 `sequence_no` 升序，供实例详情和串行推进一次加载。
    ///
    /// # 参数
    /// * `approval_instance_id` - 审批实例 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按顺序号排列的步骤实例。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_instance(
        &self,
        approval_instance_id: &ApprovalInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalStepInstance>> {
        self.find_many_sorted(
            doc! { "approval_instance_id": approval_instance_id.to_string() },
            doc! { "sequence_no": 1 },
            executor,
        )
        .await
    }

    /// 查询审批实例当前唯一活动或阻塞步骤。
    ///
    /// 同一实例同时最多一个 `ACTIVE` 或 `BLOCKED` 步骤由部分唯一索引保证。
    ///
    /// # 参数
    /// * `approval_instance_id` - 审批实例 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前步骤；终态实例或无当前步骤时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_current_by_instance(
        &self,
        approval_instance_id: &ApprovalInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalStepInstance>> {
        self.find_one(current_step_filter(approval_instance_id), executor)
            .await
    }

    /// 按实例与冻结步骤编码精确查询步骤实例。
    ///
    /// 查询覆盖唯一索引 `(approval_instance_id, step_key)`，供外部相关性与幂等推进使用。
    ///
    /// # 参数
    /// * `approval_instance_id` - 审批实例 ID
    /// * `step_key` - 冻结步骤编码
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回匹配步骤实例；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_by_instance_and_key(
        &self,
        approval_instance_id: &ApprovalInstanceId,
        step_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalStepInstance>> {
        self.find_one(
            doc! {
                "approval_instance_id": approval_instance_id.to_string(),
                "step_key": step_key,
            },
            executor,
        )
        .await
    }
}

/// 审批定义跨集合写入仓储。
///
/// 单集合查询和更新继续使用 [`Repository`]；本类型只提供必须以父定义草稿为前置的
/// 「定义草稿 + 全部步骤」原子写入，为随后同事务发布定义建立可证明的物理顺序。
pub struct ApprovalRepository<'a> {
    db: &'a Database,
}

impl<'a> ApprovalRepository<'a> {
    /// 创建审批域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回审批域专用仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 写入草稿定义及其全部步骤定义。
    ///
    /// 本方法先插入 `DRAFT` 父定义，再批量插入步骤，确保步骤写入时父定义物理状态
    /// 仍为草稿。Service 必须在同一事务内随后调用 `ApprovalDefinition::publish` 与
    /// `approval_definitions().update`；传入非事务执行器会失去整体原子性，禁止用于发布。
    ///
    /// # 参数
    /// * `definition` - 状态必须为 `DRAFT` 的审批定义
    /// * `steps` - 已完整校验且引用该定义 ID 的全部串行步骤
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 返回
    /// 两个集合全部写入后返回 `Ok(())`。
    ///
    /// # 错误
    /// 父定义不是草稿、步骤引用其它定义，或 MongoDB 写入失败时返回错误。
    pub async fn create_draft_with_steps(
        &self,
        definition: &ApprovalDefinition,
        steps: &[ApprovalStepDefinition],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !draft_with_steps_is_valid(definition, steps) {
            return Err(Error::OptimisticLockingError);
        }
        mongo_ops::insert_one(
            &self.db.collection::<ApprovalDefinition>(APPROVAL_DEFINITIONS),
            definition,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<ApprovalStepDefinition>(APPROVAL_STEP_DEFINITIONS),
            steps.to_vec(),
            executor,
        )
        .await
    }
}

fn draft_with_steps_is_valid(definition: &ApprovalDefinition, steps: &[ApprovalStepDefinition]) -> bool {
    definition.status == ApprovalDefinitionStatus::Draft
        && !steps.is_empty()
        && steps
            .iter()
            .all(|step| step.approval_definition_id.as_ref() == definition.base.id.as_str())
}

fn definition_key_version_filter(definition_key: &str, definition_version: u32) -> Document {
    doc! {
        "definition_key": definition_key,
        "definition_version": i64::from(definition_version),
    }
}

fn published_definition_filter(definition_key: &str) -> Document {
    doc! {
        "definition_key": definition_key,
        "status": ApprovalDefinitionStatus::Published.as_str(),
    }
}

fn subject_definition_filter(
    definition_key: &str,
    business_object_type: &str,
    business_object_id: &str,
    subject_version: &str,
) -> Document {
    doc! {
        "definition_key": definition_key,
        "business_object_type": business_object_type,
        "business_object_id": business_object_id,
        "subject_version": subject_version,
    }
}

fn start_idempotency_filter(definition_key: &str, start_idempotency_key: &str) -> Document {
    doc! {
        "definition_key": definition_key,
        "start_idempotency_key": start_idempotency_key,
    }
}

fn non_terminal_subject_filter(
    definition_key: &str,
    business_object_type: &str,
    business_object_id: &str,
    subject_version: &str,
) -> Document {
    let mut filter = subject_definition_filter(
        definition_key,
        business_object_type,
        business_object_id,
        subject_version,
    );
    filter.insert(
        "status",
        doc! {
            "$in": [
                ApprovalInstanceStatus::Running.as_str(),
                ApprovalInstanceStatus::Blocked.as_str(),
            ]
        },
    );
    filter
}

fn current_step_filter(approval_instance_id: &ApprovalInstanceId) -> Document {
    doc! {
        "approval_instance_id": approval_instance_id.to_string(),
        "status": {
            "$in": [
                entities::approval::ApprovalStepStatus::Active.as_str(),
                entities::approval::ApprovalStepStatus::Blocked.as_str(),
            ]
        },
    }
}

fn blocked_instances_filter(owner_organization_ids: Option<&[String]>) -> Document {
    let mut filter = doc! {
        "status": ApprovalInstanceStatus::Blocked.as_str(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    };
    if let Some(owner_organization_ids) = owner_organization_ids {
        filter.insert("owner_organization_id", doc! { "$in": owner_organization_ids });
    }
    filter
}

#[cfg(test)]
mod tests {
    use entities::{
        approval::{
            ApprovalAssignmentMode, ApprovalDecision, ApprovalDefinition, ApprovalDefinitionData,
            ApprovalDefinitionId, ApprovalInstanceId, ApprovalInstanceStatus, ApprovalRuntimeKind,
            ApprovalStepDefinition, ApprovalStepDefinitionData, ApprovalStepDefinitionId,
        },
        common::time::Instant,
        work_item::WorkItemType,
    };
    use mongodb::bson::doc;

    use super::{
        blocked_instances_filter, current_step_filter, definition_key_version_filter,
        draft_with_steps_is_valid, non_terminal_subject_filter, published_definition_filter,
        start_idempotency_filter, ApprovalInstanceFilter, QueryFilter,
    };

    #[test]
    fn definition_filters_use_business_version_field_not_lock_version() {
        assert_eq!(
            definition_key_version_filter("SALES_ORDER_APPROVAL", 3),
            doc! {
                "definition_key": "SALES_ORDER_APPROVAL",
                "definition_version": 3_i64,
            }
        );
        let published = published_definition_filter("SALES_ORDER_APPROVAL");
        assert_eq!(published.get_str("status").unwrap(), "PUBLISHED");
        assert!(!published.contains_key("version"));
    }

    #[test]
    fn non_terminal_subject_filter_matches_partial_unique_states() {
        let filter = non_terminal_subject_filter(
            "SALES_ORDER_APPROVAL",
            "SALES_ORDER",
            "sales-order-1",
            "submission-2",
        );
        assert_eq!(filter.get_str("definition_key").unwrap(), "SALES_ORDER_APPROVAL");
        assert_eq!(filter.get_str("subject_version").unwrap(), "submission-2");
        assert_eq!(
            filter.get_document("status").unwrap(),
            &doc! { "$in": ["RUNNING", "BLOCKED"] }
        );
    }

    #[test]
    fn start_idempotency_filter_is_scoped_by_definition_and_includes_terminal_history() {
        let filter = start_idempotency_filter("SALES_ORDER_APPROVAL", "start-request-1");
        assert_eq!(
            filter,
            doc! {
                "definition_key": "SALES_ORDER_APPROVAL",
                "start_idempotency_key": "start-request-1",
            }
        );
        assert!(!filter.contains_key("status"));
    }

    #[test]
    fn current_step_filter_matches_active_and_blocked_states() {
        assert_eq!(
            current_step_filter(&ApprovalInstanceId::new("instance-1")),
            doc! {
                "approval_instance_id": "instance-1",
                "status": { "$in": ["ACTIVE", "BLOCKED"] },
            }
        );
    }

    #[test]
    fn blocked_filter_includes_lock_version_bearing_entities_and_scope() {
        let mut filter = ApprovalInstanceFilter::blocked(2, 25);
        filter.definition_key = Some("SALES_ORDER_APPROVAL".to_string());
        filter.business_object_type = Some("SALES_ORDER".to_string());
        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("status").unwrap(), "BLOCKED");
        assert_eq!(
            document.get_str("definition_key").unwrap(),
            "SALES_ORDER_APPROVAL"
        );
        assert_eq!(document.get_str("business_object_type").unwrap(), "SALES_ORDER");
        assert_eq!(filter.status, Some(ApprovalInstanceStatus::Blocked));
    }

    #[test]
    fn blocked_filter_applies_organization_scope_in_database_query() {
        let organizations = vec!["organization-1".to_string(), "organization-2".to_string()];
        let scoped = blocked_instances_filter(Some(&organizations));
        assert_eq!(scoped.get_str("status").unwrap(), "BLOCKED");
        assert_eq!(
            scoped.get_document("owner_organization_id").unwrap(),
            &doc! { "$in": ["organization-1", "organization-2"] }
        );
        assert!(!blocked_instances_filter(None).contains_key("owner_organization_id"));
        let empty: Vec<String> = Vec::new();
        assert_eq!(
            blocked_instances_filter(Some(&empty))
                .get_document("owner_organization_id")
                .unwrap(),
            &doc! { "$in": [] }
        );
    }

    #[test]
    fn draft_with_steps_requires_draft_parent_non_empty_and_matching_parent_id() {
        let mut definition = ApprovalDefinition::new(
            ApprovalDefinitionId::new("definition-1"),
            ApprovalDefinitionData {
                definition_key: "SALES_ORDER_APPROVAL".to_string(),
                definition_version: 1,
                name: "销售单审批".to_string(),
                runtime_kind: ApprovalRuntimeKind::Internal,
                external_definition_id: None,
            },
        )
        .unwrap();
        let matching = ApprovalStepDefinition::new(
            ApprovalStepDefinitionId::new("step-definition-1"),
            ApprovalStepDefinitionData {
                approval_definition_id: ApprovalDefinitionId::new("definition-1"),
                step_key: "SALES_MANAGER".to_string(),
                sequence_no: 1,
                work_item_type: WorkItemType::CardSalesManagerApproval,
                handler_key: "card_sales_approval".to_string(),
                assignment_mode: ApprovalAssignmentMode::Direct,
                assignee_resolver_key: "sales_manager_of_owner".to_string(),
                allowed_decisions: vec![ApprovalDecision::Approve],
            },
        )
        .unwrap();
        assert!(draft_with_steps_is_valid(
            &definition,
            std::slice::from_ref(&matching)
        ));
        assert!(!draft_with_steps_is_valid(&definition, &[]));

        let other_parent = ApprovalStepDefinition::new(
            ApprovalStepDefinitionId::new("step-definition-2"),
            ApprovalStepDefinitionData {
                approval_definition_id: ApprovalDefinitionId::new("definition-2"),
                step_key: "OPERATIONS".to_string(),
                sequence_no: 2,
                work_item_type: WorkItemType::CardSalesOperationApproval,
                handler_key: "card_sales_approval".to_string(),
                assignment_mode: ApprovalAssignmentMode::Pool,
                assignee_resolver_key: "card_operations_pool".to_string(),
                allowed_decisions: vec![ApprovalDecision::Approve],
            },
        )
        .unwrap();
        assert!(!draft_with_steps_is_valid(&definition, &[other_parent]));

        definition
            .publish("bootstrap", Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        assert!(!draft_with_steps_is_valid(&definition, &[matching]));
    }
}
