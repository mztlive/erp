//! 域 D06 `access_control` 仓储：permission、user_role、data_scope、audit_event。
//!
//! P0 已实现 accounts / roles / audit_logs 仓储（`account_core.rs` / `role.rs` /
//! `audit_log.rs`），本文件只承载四个新增集合。单一集合 CRUD 与乐观锁直接
//! 复用 [`Repository`] 基类（base.rs：`update`/`soft_delete`/`restore` 比较
//! `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `extensions::AccessControlExt`
//! 关联常量导入（conventions §4.3）。
//!
//! `audit_event` 是事实型审计留痕（§4.5.4：不可编辑、不可删除），本文件
//! **不提供**软删除/恢复方法；筛选/行类型定义在本文件，经 `AccessControlExt`
//! 的关联类型对外暴露。

use entities::access_control::{
    AuditEvent, AuditEventResult, DataScope, DataScopeSubjectType, DataScopeType, Permission, UserRole,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::AccessControlExt;
use super::{regex_filter::insert_literal_regex_filter, PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `user_role` 集合名（单一来源：`AccessControlExt` 关联常量）。
const USER_ROLES: &str = <mongodb::Database as AccessControlExt>::USER_ROLES;
/// `audit_event` 集合名（单一来源：`AccessControlExt` 关联常量）。
const AUDIT_EVENTS: &str = <mongodb::Database as AccessControlExt>::AUDIT_EVENTS;

/// 权限定义列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionRow {
    /// 实体主键。
    pub id: String,
    /// 权限资源。
    pub resource: String,
    /// 权限动作。
    pub action: String,
    /// 展示名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 系统内建权限标记。
    pub system: bool,
    /// 停用标记。
    pub disabled: bool,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 权限定义列表筛选条件。
#[derive(Debug, Clone)]
pub struct PermissionFilter {
    /// 权限资源（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub resource: Option<String>,
    /// 停用标记；`None` 表示不筛选。
    pub disabled: Option<bool>,
    /// 是否仅系统内建；`None` 表示不筛选。
    pub system: Option<bool>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PermissionFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "resource", self.resource.as_deref());
        if let Some(disabled) = self.disabled {
            filter.insert("disabled", disabled);
        }
        if let Some(system) = self.system {
            filter.insert("system", system);
        }
        filter
    }
}

impl Pagination for PermissionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, Permission> {
    /// 分页检索权限定义列表（投影查询，权限目录）。
    ///
    /// 只返回 [`PermissionRow`] 所需的目录字段，不加载整文档；`resource` 按
    /// 字面量忽略大小写模糊匹配（复用 `repository::regex_filter`），停用/系统
    /// 标记精确匹配覆盖 `idx_permissions_disabled`。
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
    pub async fn search_permissions(
        &self,
        filter: &PermissionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PermissionRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(permission_projection())
            .build();
        let collection = self.collection().clone_with_type::<PermissionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, UserRole> {
    /// 按用户批量取回全部角色绑定（W19：按当前、未来、已过期分开只读展示）。
    ///
    /// # 参数
    /// * `user_id` - 用户 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按生效时间倒序排列的绑定记录（含已撤权历史）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_user(&self, user_id: &str, executor: &mut dyn Executor) -> Result<Vec<UserRole>> {
        self.find_many_sorted(
            doc! { "user_id": user_id },
            doc! { "effective_from": -1 },
            executor,
        )
        .await
    }
}

/// 数据范围列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataScopeRow {
    /// 实体主键。
    pub id: String,
    /// 范围主体类型。
    pub subject_type: DataScopeSubjectType,
    /// 范围主体 ID。
    pub subject_id: String,
    /// 范围类型。
    pub scope_type: DataScopeType,
    /// 范围对象。
    pub scope_targets: Vec<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 数据范围列表筛选条件。
#[derive(Debug, Clone)]
pub struct DataScopeFilter {
    /// 范围主体类型；`None` 表示不筛选。
    pub subject_type: Option<DataScopeSubjectType>,
    /// 范围主体 ID；`None` 表示不筛选。
    pub subject_id: Option<String>,
    /// 范围类型；`None` 表示不筛选。
    pub scope_type: Option<DataScopeType>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for DataScopeFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(subject_type) = self.subject_type {
            filter.insert("subject_type", subject_type.as_str());
        }
        if let Some(subject_id) = &self.subject_id {
            filter.insert("subject_id", subject_id);
        }
        if let Some(scope_type) = self.scope_type {
            filter.insert("scope_type", scope_type.as_str());
        }
        filter
    }
}

impl Pagination for DataScopeFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, DataScope> {
    /// 判断指定主体是否存在至少一个未软删除的数据范围。
    ///
    /// 查询只投影 `_id` 并在首条命中后停止，不反序列化完整范围集合。
    ///
    /// # 参数
    /// * `subject_type` - 范围主体类型
    /// * `subject_id` - 范围主体 ID
    /// * `executor` - 调用方事务或非事务执行器
    ///
    /// # 返回
    /// 存在至少一个活跃范围时返回 `true`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn exists_by_subject(
        &self,
        subject_type: DataScopeSubjectType,
        subject_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.exists(
            doc! {
                "subject_type": subject_type.as_str(),
                "subject_id": subject_id,
            },
            executor,
        )
        .await
    }

    /// 分页检索数据范围列表（投影查询）。
    ///
    /// 只返回 [`DataScopeRow`] 所需的配置字段，不加载整文档。
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
    pub async fn search_data_scopes(
        &self,
        filter: &DataScopeFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<DataScopeRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(data_scope_projection())
            .build();
        let collection = self.collection().clone_with_type::<DataScopeRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按单个主体取回数据范围。
    ///
    /// # 参数
    /// * `subject_type` - 范围主体类型
    /// * `subject_id` - 范围主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该主体的全部数据范围。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_subject(
        &self,
        subject_type: DataScopeSubjectType,
        subject_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DataScope>> {
        self.find_many_sorted(
            doc! {
                "subject_type": subject_type.as_str(),
                "subject_id": subject_id,
            },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }

    /// 按同类主体 ID 集合批量取回数据范围。
    ///
    /// 查询复用 `uk_data_scopes_subject_scope` 的
    /// `(subject_type, subject_id)` 前缀；Repository 只返回未软删除的
    /// 持久化事实，不计算用户与角色范围的授权交集。
    ///
    /// # 参数
    /// * `subject_type` - 范围主体类型
    /// * `subject_ids` - 同类主体 ID 集合；为空时不访问 MongoDB
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未软删除的数据范围；缺失主体不会补齐结果。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_subjects(
        &self,
        subject_type: DataScopeSubjectType,
        subject_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<DataScope>> {
        let Some(filter) = data_scope_subjects_filter(subject_type, subject_ids) else {
            return Ok(Vec::new());
        };
        self.find_many_sorted(filter, doc! { "subject_id": 1, "created_at": 1 }, executor)
            .await
    }
}

/// 构造同类主体批量查询条件。
///
/// # 参数
/// * `subject_type` - 范围主体类型
/// * `subject_ids` - 同类主体 ID 集合
///
/// # 返回
/// 非空输入返回可使用现有主体复合索引的查询条件；空输入返回 `None`。
///
/// # 错误
/// 无；未删除条件由 [`Repository::find_many_sorted`] 统一追加。
fn data_scope_subjects_filter(
    subject_type: DataScopeSubjectType,
    subject_ids: &[String],
) -> Option<Document> {
    if subject_ids.is_empty() {
        return None;
    }
    Some(doc! {
        "subject_type": subject_type.as_str(),
        "subject_id": { "$in": subject_ids },
    })
}

/// 审计事件列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventRow {
    /// 实体主键。
    pub id: String,
    /// 操作者 ID。
    pub actor_id: String,
    /// 操作者名称快照。
    pub actor_label: String,
    /// 责任角色快照。
    pub actor_role: String,
    /// 动作代码。
    pub action_type: String,
    /// 业务对象类型代码。
    pub object_type: String,
    /// 业务对象 ID。
    pub object_id: Option<String>,
    /// 业务对象安全标题。
    pub object_label: Option<String>,
    /// 请求追踪号。
    pub request_id: Option<String>,
    /// 最终结果。
    pub result: AuditEventResult,
    /// 变更字段名（只记录字段名和「已变更」）。
    pub changed_field_names: Vec<String>,
    /// 来源 IP。
    pub source_ip: Option<String>,
    /// 创建时间（秒级时间戳，即事件发生时间）。
    pub created_at: u64,
}

/// 审计事件列表筛选条件。
#[derive(Debug, Clone)]
pub struct AuditEventFilter {
    /// 操作者 ID（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub actor_id: Option<String>,
    /// 动作代码（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub action_type: Option<String>,
    /// 业务对象类型代码；`None` 表示不筛选。
    pub object_type: Option<String>,
    /// 业务对象 ID；`None` 表示不筛选。
    pub object_id: Option<String>,
    /// 最终结果；`None` 表示不筛选。
    pub result: Option<AuditEventResult>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for AuditEventFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "actor_id", self.actor_id.as_deref());
        insert_literal_regex_filter(&mut filter, "action_type", self.action_type.as_deref());
        if let Some(object_type) = &self.object_type {
            filter.insert("object_type", object_type);
        }
        if let Some(object_id) = &self.object_id {
            filter.insert("object_id", object_id);
        }
        if let Some(result) = self.result {
            filter.insert("result", result.as_str());
        }
        filter
    }
}

impl Pagination for AuditEventFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, AuditEvent> {
    /// 分页检索审计事件（投影查询）。
    ///
    /// 只返回 [`AuditEventRow`] 所需的审计字段，不加载整文档；`actor_id` /
    /// `action_type` 按字面量忽略大小写模糊匹配（复用 `repository::regex_filter`），
    /// 对象/结果精确匹配覆盖 `idx_audit_events_object_created`。审计事件是
    /// 追加式留痕，本集合**不提供**软删除/恢复方法。
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
    pub async fn search_audit_events(
        &self,
        filter: &AuditEventFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<AuditEventRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(audit_event_projection())
            .build();
        let collection = self.collection().clone_with_type::<AuditEventRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// D06 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `AccessControlExt::access_control()` 访问。
pub struct AccessControlRepository<'a> {
    db: &'a Database,
}

impl<'a> AccessControlRepository<'a> {
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

    /// 分配用户角色并追加审计事件（跨集合多步骤写入）。
    ///
    /// 依次写入 `user_roles` 与 `audit_events`，保证「授权绑定 + 审计留痕」
    /// 原子可见（§4.5.4 安全审计与变更留痕）。**必须收到事务执行器**：本方法
    /// 不构成原子边界，传入 `NoTransaction` 时两笔写入各自自动提交，审计失败
    /// 会留下没有审计的绑定；Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `binding` - 待写入的用户角色绑定
    /// * `event` - 待追加的审计事件
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn assign_user_role_with_audit(
        &self,
        binding: &UserRole,
        event: &AuditEvent,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<UserRole>(USER_ROLES), binding, executor).await?;
        mongo_ops::insert_one(&self.db.collection::<AuditEvent>(AUDIT_EVENTS), event, executor).await?;
        Ok(())
    }
}

/// 构建排序文档（排序字段白名单化，禁止透传任意字段名）。
///
/// 仅允许 `created_at` / `updated_at`；未知字段回落默认 `created_at`。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或白名单外字段时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("updated_at") => "updated_at",
        _ => "created_at",
    };
    doc! { field: direction, "id": direction }
}

/// 权限定义列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn permission_projection() -> Document {
    doc! {
        "id": 1,
        "resource": 1,
        "action": 1,
        "name": 1,
        "description": 1,
        "system": 1,
        "disabled": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 数据范围列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn data_scope_projection() -> Document {
    doc! {
        "id": 1,
        "subject_type": 1,
        "subject_id": 1,
        "scope_type": 1,
        "scope_targets": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 审计事件列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn audit_event_projection() -> Document {
    doc! {
        "id": 1,
        "actor_id": 1,
        "actor_label": 1,
        "actor_role": 1,
        "action_type": 1,
        "object_type": 1,
        "object_id": 1,
        "object_label": 1,
        "request_id": 1,
        "result": 1,
        "changed_field_names": 1,
        "source_ip": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        data_scope_subjects_filter, sort_doc, AuditEventFilter, DataScopeFilter, PermissionFilter,
        QueryFilter, Repository,
    };
    use crate::NoTransaction;
    use entities::access_control::{AuditEventResult, DataScope, DataScopeSubjectType, DataScopeType};
    use mongodb::bson::{doc, Bson};

    #[test]
    fn permission_filter_applies_resource_regex_and_flags() {
        let filter = PermissionFilter {
            resource: Some("sales_order".to_string()),
            disabled: Some(false),
            system: Some(true),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        let resource = document.get_document("resource").unwrap();
        assert_eq!(resource.get_str("$regex").unwrap(), "sales_order");
        assert!(!document.get_bool("disabled").unwrap());
        assert!(document.get_bool("system").unwrap());
    }

    #[test]
    fn data_scope_filter_applies_subject_and_scope_type() {
        let filter = DataScopeFilter {
            subject_type: Some(DataScopeSubjectType::Role),
            subject_id: Some("role-sales".to_string()),
            scope_type: Some(DataScopeType::Team),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("subject_type").unwrap(), "role");
        assert_eq!(document.get_str("subject_id").unwrap(), "role-sales");
        assert_eq!(document.get_str("scope_type").unwrap(), "team");
    }

    /// 批量主体查询保留正常与可能缺失的 ID，由 MongoDB 只返回实际事实。
    #[test]
    fn data_scope_subjects_filter_uses_subject_type_and_id_set() {
        let ids = vec!["role-1".to_string(), "missing-role".to_string()];
        let filter = data_scope_subjects_filter(DataScopeSubjectType::Role, &ids).unwrap();

        assert_eq!(filter.get_str("subject_type").unwrap(), "role");
        assert_eq!(
            filter
                .get_document("subject_id")
                .unwrap()
                .get_array("$in")
                .unwrap(),
            &vec![
                Bson::String("role-1".to_string()),
                Bson::String("missing-role".to_string())
            ]
        );
    }

    /// 空主体集合必须短路，不得构造可扩大范围的查询。
    #[tokio::test]
    async fn data_scope_subjects_empty_input_does_not_touch_database() {
        assert!(data_scope_subjects_filter(DataScopeSubjectType::Role, &[]).is_none());
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .unwrap();
        let database = client.database("repository_data_scope_empty_subject_ids");
        let repository = Repository::<DataScope>::new(&database, "data_scopes");

        let scopes = repository
            .list_by_subjects(DataScopeSubjectType::Role, &[], &mut NoTransaction)
            .await
            .unwrap();

        assert!(scopes.is_empty());
    }

    #[test]
    fn audit_event_filter_applies_regex_and_object_fields() {
        let filter = AuditEventFilter {
            actor_id: Some("user-1".to_string()),
            action_type: Some("sales_order.approve".to_string()),
            object_type: Some("sales_order".to_string()),
            object_id: Some("SO-1".to_string()),
            result: Some(AuditEventResult::Denied),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        let actor = document.get_document("actor_id").unwrap();
        assert_eq!(actor.get_str("$regex").unwrap(), r"user\-1");
        let action = document.get_document("action_type").unwrap();
        assert_eq!(action.get_str("$regex").unwrap(), r"sales_order\.approve");
        assert_eq!(document.get_str("object_type").unwrap(), "sales_order");
        assert_eq!(document.get_str("object_id").unwrap(), "SO-1");
        assert_eq!(document.get_str("result").unwrap(), "DENIED");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1, "id": -1 });
        assert_eq!(
            sort_doc(Some("updated_at"), true),
            doc! { "updated_at": 1, "id": 1 }
        );
        assert_eq!(
            sort_doc(Some("actor_id"), false),
            doc! { "created_at": -1, "id": -1 },
            "白名单外字段回落默认排序"
        );
    }
}
