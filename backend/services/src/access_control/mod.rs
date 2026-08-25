//! 域 D06 `access_control` 服务编排。
//!
//! 本域是既有 Casbin/RBAC/审计基线的**增补**（domains.md 注）：角色、账号、
//! Casbin policy 由 `services::iam` 承载，本服务只覆盖
//! - `permission` 定义目录（`resource:action` 唯一，经 `entities::rbac::Permission`
//!   解析规范化）；
//! - `data_scope` 数据范围增补；
//! - `user_role` 绑定记录（授权事实仍由 Casbin `g` 规则承载，本服务只维护
//!   绑定留痕表，不重建 iam 的授权能力）；
//! - `audit_event` 查询（`audit_log → audit_event` 字段对齐，事实型留痕，
//!   不可编辑不可删除）。
//!
//! 事务边界只在 Service：本域全部写入（业务行 + `audit_events` 留痕）→
//! `with_transaction` 内原子提交；涉及角色绑定的既有 policy 事务运行器
//! （`run_authorized_audited_policy_transaction`）属于 iam，不在此重建。
//!
//! 跨域：无（依赖列为空；只经 `AccessControlExt` 访问本域仓储）。

use database::{AccessControlExt, NoTransaction, Transactional};
use entities::access_control::{
    AuditEvent, AuditEventData, AuditEventId, AuditEventResult, DataScope, DataScopeId, Permission,
    PermissionId, UserRole, UserRoleId,
};
use entities::common::time::Instant;
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

pub use self::dto::{
    AssignUserRoleRequest, AuditEventListParams, AuditEventView, CreateDataScopeRequest,
    CreatePermissionRequest, DataScopeListParams, DataScopeView, PageView, PermissionListParams,
    PermissionView, RevokeUserRoleRequest, UpdatePermissionRequest, UserRoleListParams, UserRoleView,
};

/// 权限定义列表筛选条件类型（经 `AccessControlExt` 关联类型跨 crate 可达）。
type PermissionFilter = <mongodb::Database as AccessControlExt>::PermissionFilter;
/// 数据范围列表筛选条件类型。
type DataScopeFilter = <mongodb::Database as AccessControlExt>::DataScopeFilter;
/// 审计事件列表筛选条件类型。
type AuditEventFilter = <mongodb::Database as AccessControlExt>::AuditEventFilter;

/// 访问控制服务。
///
/// 提供权限目录、数据范围、用户角色绑定记录与审计事件的增补编排。
pub struct AccessControlService {
    db: Database,
}

impl AccessControlService {
    /// 创建访问控制服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询权限定义列表（权限目录）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`resource`/`disabled`/`system` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn permission_list(&self, params: &PermissionListParams) -> Result<PageView<PermissionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PermissionFilter {
            resource: query.resource,
            disabled: query.disabled,
            system: query.system,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .permissions()
            .search_permissions(&filter, &mut NoTransaction)
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结，无法命名），
        // 此处按字段映射为响应视图，避免把仓储类型泄漏到接口层。
        let items = page
            .items
            .into_iter()
            .map(|row| PermissionView {
                id: row.id,
                resource: row.resource,
                action: row.action,
                name: row.name,
                description: None,
                system: row.system,
                disabled: row.disabled,
                version: 0,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建权限定义。
    ///
    /// `resource:action` 经实体复用 `rbac::Permission::parse` 规范化（小写）；
    /// 同键重复由唯一索引 `uk_permissions_resource_action` 透出冲突（409）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的权限定义视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 同 `resource:action` 已存在（唯一索引透出）
    pub async fn create_permission(
        &self,
        req: CreatePermissionRequest,
        actor: &AuditActor,
    ) -> Result<PermissionView> {
        req.validate()?;
        let permission = Permission::new(PermissionId::new(next_id()), req.into_data())?;
        let event = self
            .build_audit_event(
                actor,
                "permission.create",
                "permission",
                Some(permission.base.id.clone()),
                Vec::new(),
            )
            .await?;
        let db = self.db.clone();
        let client = db.client().clone();
        let permission_for_tx = permission.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.permissions().create(&permission_for_tx, session).await?;
                    db.audit_events().create(&event, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(permission.into())
    }

    /// 更新权限定义。
    ///
    /// `resource:action` 与 `system` 是权限身份与安全边界，不允许在通用更新
    /// 中修改（实体约束）；乐观锁版本不一致时返回 409。
    ///
    /// # 参数
    /// * `id` - 权限定义 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后的权限定义视图。
    ///
    /// # 错误
    /// * `NotFound` - 权限定义不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_permission(
        &self,
        id: &str,
        req: UpdatePermissionRequest,
        actor: &AuditActor,
    ) -> Result<PermissionView> {
        req.validate()?;
        let mut permission = self.load_permission_with_version(id, req.version).await?;
        let changed = changed_permission_fields(&permission, &req);
        permission.update(req.into_update())?;
        let event = self
            .build_audit_event(
                actor,
                "permission.update",
                "permission",
                Some(permission.base.id.clone()),
                changed,
            )
            .await?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.permissions().update(&mut permission, session).await?;
                    db.audit_events().create(&event, session).await?;
                    Ok::<Permission, crate::errors::Error>(permission)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 删除权限定义（软删除）。
    ///
    /// 系统内建权限禁止删除（实体 `ensure_deletable`）；软删除保留身份，
    /// 避免复用破坏审计与授权绑定语义。
    ///
    /// # 参数
    /// * `id` - 权限定义 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// * `NotFound` - 权限定义不存在
    /// * `BusinessLogicError` - 系统内建权限禁止删除
    pub async fn delete_permission(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut permission = self
            .db
            .permissions()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("权限定义不存在".to_string()))?;
        permission.ensure_deletable()?;
        let event = self
            .build_audit_event(
                actor,
                "permission.delete",
                "permission",
                Some(id.to_string()),
                Vec::new(),
            )
            .await?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.permissions().soft_delete(&mut permission, session).await?;
                    db.audit_events().create(&event, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    /// 分页查询数据范围列表。
    ///
    /// 携带 `subject_id`（与 `subject_type` 成对）时按主体批量取回。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法、排序字段不在白名单或按主体查询缺
    ///   少主体类型
    /// * `RepositoryError` - 数据库查询失败
    pub async fn data_scope_list(&self, params: &DataScopeListParams) -> Result<PageView<DataScopeView>> {
        params.validate()?;
        let query = params.normalized()?;
        if let Some(subject_id) = &query.subject_id {
            let subject_type = query
                .subject_type
                .ok_or_else(|| Error::ValidationError("按主体查询时必须提供范围主体类型".to_string()))?;
            let items: Vec<DataScopeView> = self
                .db
                .data_scopes()
                .list_by_subject(subject_type, subject_id, &mut NoTransaction)
                .await?
                .into_iter()
                .map(Into::into)
                .collect();
            let total = items.len() as i64;
            return Ok(PageView {
                items,
                total,
                page: query.paging.page,
                page_size: query.paging.page_size,
            });
        }
        let filter = DataScopeFilter {
            subject_type: query.subject_type,
            scope_type: query.scope_type,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .data_scopes()
            .search_data_scopes(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| DataScopeView {
                id: row.id,
                subject_type: row.subject_type,
                subject_id: row.subject_id,
                scope_type: row.scope_type,
                scope_targets: row.scope_targets,
                version: 0,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建数据范围。
    ///
    /// 范围类型与目标携带一致性由实体校验（组织/团队必须携带目标，公司/本人
    /// 负责/协作参与不允许携带）；同主体同范围类型唯一由
    /// `uk_data_scopes_subject_scope` 透出冲突（409）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的数据范围视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 同主体同范围类型已存在（唯一索引透出）
    pub async fn create_data_scope(
        &self,
        req: CreateDataScopeRequest,
        actor: &AuditActor,
    ) -> Result<DataScopeView> {
        req.validate()?;
        let scope = DataScope::new(DataScopeId::new(next_id()), req.into_data())?;
        let event = self
            .build_audit_event(
                actor,
                "data_scope.create",
                "data_scope",
                Some(scope.base.id.clone()),
                Vec::new(),
            )
            .await?;
        let db = self.db.clone();
        let client = db.client().clone();
        let scope_for_tx = scope.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.data_scopes().create(&scope_for_tx, session).await?;
                    db.audit_events().create(&event, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(scope.into())
    }

    /// 删除数据范围（软删除）。
    ///
    /// # 参数
    /// * `id` - 数据范围 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// * `NotFound` - 数据范围不存在
    pub async fn delete_data_scope(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut scope = self
            .db
            .data_scopes()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("数据范围不存在".to_string()))?;
        let event = self
            .build_audit_event(
                actor,
                "data_scope.delete",
                "data_scope",
                Some(id.to_string()),
                Vec::new(),
            )
            .await?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.data_scopes().soft_delete(&mut scope, session).await?;
                    db.audit_events().create(&event, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    /// 按用户查询角色绑定（W19 用户授权，含撤权历史）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`user_id` 必填）
    ///
    /// # 返回
    /// 返回按生效时间倒序排列的绑定视图。
    ///
    /// # 错误
    /// * `ValidationError` - 用户 ID 缺失
    pub async fn user_role_list(&self, params: &UserRoleListParams) -> Result<Vec<UserRoleView>> {
        params.validate()?;
        let items = self
            .db
            .user_roles()
            .list_by_user(&params.user_id, &mut NoTransaction)
            .await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// 分配用户角色（绑定记录 + 审计事件原子写入）。
    ///
    /// 目标角色必须存在（本域 `roles` 仓储读取）；同一用户同一角色同时仅一条
    /// 未撤权绑定由部分唯一索引 `uk_user_roles_active` 承担（撤权后再授权）。
    /// 授权事实仍由 Casbin `g` 规则承载（既有 `iam` 能力），本方法只维护
    /// `user_role` 绑定留痕表。
    ///
    /// # 参数
    /// * `req` - 分配请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的绑定视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 目标角色不存在
    /// * `ConflictError` - 同一用户同一角色已有未撤权绑定（唯一索引透出）
    pub async fn assign_user_role(
        &self,
        req: AssignUserRoleRequest,
        actor: &AuditActor,
    ) -> Result<UserRoleView> {
        req.validate()?;
        self.db
            .roles()
            .find_by_id(req.role_id.as_str(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("角色不存在".to_string()))?;
        let binding = UserRole::new(UserRoleId::new(next_id()), req.into_data(actor.id()))?;
        let event = self
            .build_audit_event(
                actor,
                "user_role.assign",
                "user_role",
                Some(binding.base.id.clone()),
                Vec::new(),
            )
            .await?;
        let db = self.db.clone();
        let client = db.client().clone();
        let binding_for_tx = binding.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.access_control()
                        .assign_user_role_with_audit(&binding_for_tx, &event, session)
                        .await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(binding.into())
    }

    /// 撤权（立即紧急撤权语义，必须记录结构化原因）。
    ///
    /// 撤权是审计动作，历史撤权记录累积保留；绑定读取、实体撤权、乐观锁
    /// 更新和审计事件写入在同一事务内完成。已撤权绑定不可重复撤权。
    ///
    /// # 参数
    /// * `id` - 绑定 ID
    /// * `req` - 撤权命令（只携带业务原因，当前版本由服务端读取）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤权后的绑定视图。
    ///
    /// # 错误
    /// * `NotFound` - 绑定不存在
    /// * `ConflictError` - 并发修改或绑定已撤权
    pub async fn revoke_user_role(
        &self,
        id: &str,
        req: RevokeUserRoleRequest,
        actor: &AuditActor,
    ) -> Result<UserRoleView> {
        req.validate()?;
        let event = self
            .build_audit_event(
                actor,
                "user_role.revoke",
                "user_role",
                Some(id.to_string()),
                vec!["revoked_at".to_string()],
            )
            .await?;
        let db = self.db.clone();
        let client = db.client().clone();
        let id = id.to_string();
        let revoked_by = actor.id().to_string();
        let revoke_data = req.into_revoke_data();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut binding = db
                        .user_roles()
                        .find_by_id(&id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("用户角色绑定不存在".to_string()))?;
                    binding.revoke(revoke_data, &revoked_by, Instant::now())?;
                    db.user_roles().update(&mut binding, session).await?;
                    db.audit_events().create(&event, session).await?;
                    Ok::<UserRole, crate::errors::Error>(binding)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 分页查询审计事件（W19 §5.2 审计查询）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`actor_id`/`action_type`/`object_type`/`result` 等）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn audit_event_list(&self, params: &AuditEventListParams) -> Result<PageView<AuditEventView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = AuditEventFilter {
            actor_id: query.actor_id,
            action_type: query.action_type,
            object_type: query.object_type,
            object_id: query.object_id,
            result: query.result,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .audit_events()
            .search_audit_events(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| AuditEventView {
                id: row.id,
                actor_id: row.actor_id,
                actor_label: row.actor_label,
                actor_role: row.actor_role,
                action_type: row.action_type,
                object_type: row.object_type,
                object_id: row.object_id,
                object_label: row.object_label,
                request_id: row.request_id,
                trace_id: None,
                result: row.result,
                changed_field_names: row.changed_field_names,
                safe_digest: None,
                source_ip: row.source_ip,
                device_context: None,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 按 ID 加载权限定义并校验期望版本。
    ///
    /// # 参数
    /// * `id` - 权限定义 ID
    /// * `expected_version` - 请求携带的期望版本
    ///
    /// # 返回
    /// 返回加载的权限定义实体。
    ///
    /// # 错误
    /// 权限不存在返回 `NotFound`；版本不一致返回 `ConflictError`。
    async fn load_permission_with_version(&self, id: &str, expected_version: u64) -> Result<Permission> {
        let permission = self
            .db
            .permissions()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("权限定义不存在".to_string()))?;
        if permission.base.version != expected_version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(permission)
    }

    /// 构造本域写入的审计事件（`audit_log → audit_event` 字段对齐形态）。
    ///
    /// 操作者名称快照取账号 `name`（账号不存在时退回登录账号语义的空安全
    /// 快照）；责任角色快照取账号类型；结果恒为成功。
    ///
    /// # 参数
    /// * `actor` - 已通过鉴权的审计操作人
    /// * `action_type` - 动作代码
    /// * `object_type` - 业务对象类型代码
    /// * `object_id` - 业务对象 ID
    /// * `changed_field_names` - 变更字段名（只记录字段名和「已变更」）
    ///
    /// # 返回
    /// 返回新建的审计事件。
    ///
    /// # 错误
    /// 审计字段违反领域约束时返回错误。
    async fn build_audit_event(
        &self,
        actor: &AuditActor,
        action_type: &str,
        object_type: &str,
        object_id: Option<String>,
        changed_field_names: Vec<String>,
    ) -> Result<AuditEvent> {
        let account = self
            .db
            .accounts()
            .find_by_id(actor.id(), &mut NoTransaction)
            .await?;
        let actor_label = account
            .map(|account| account.name)
            .unwrap_or_else(|| "系统操作人".to_string());
        AuditEvent::new(
            AuditEventId::new(next_id()),
            AuditEventData {
                actor_id: actor.id().to_string(),
                actor_label,
                actor_role: actor.kind().as_str().to_string(),
                action_type: action_type.to_string(),
                object_type: object_type.to_string(),
                object_id,
                object_label: None,
                request_id: None,
                trace_id: None,
                result: AuditEventResult::Success,
                changed_field_names,
                safe_digest: None,
                source_ip: None,
                device_context: None,
            },
        )
        .map_err(Into::into)
    }
}

/// 提取权限更新请求涉及的字段名（只记录「已变更」，不记录旧值/新值）。
///
/// # 参数
/// * `req` - 更新请求
///
/// # 返回
/// 返回请求携带的变更字段名列表。
fn changed_permission_fields(_permission: &Permission, req: &UpdatePermissionRequest) -> Vec<String> {
    let mut changed = Vec::new();
    if req.name.is_some() {
        changed.push("name".to_string());
    }
    if req.description.is_some() {
        changed.push("description".to_string());
    }
    if req.disabled.is_some() {
        changed.push("disabled".to_string());
    }
    changed
}
