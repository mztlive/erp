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

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_permissions::access_control;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use crate::entity::access_control::{
    AuditEvent, AuditEventData, AuditEventId, AuditEventResult, PermissionDefinition, PermissionId, UserRole,
    UserRoleId,
};
use crate::ports::ScopeTargetPort;
use crate::repository::prelude::*;
use crate::service::iam::management::ManagementAccess;
use crate::{AccessControlExt, AuthorizationService};
mod person_scope;
mod person_scope_query;
pub mod policy_bundle;
pub(crate) mod resolve;

use application_core::AuditActor;

pub use crate::dto::{
    AssignUserRoleRequest, AuditEventListParams, AuditEventView, CreatePermissionRequest, PageView,
    PermissionListParams, PermissionView, RevokeUserRoleRequest, UpdatePermissionRequest, UserRoleListParams,
    UserRoleView,
};
use crate::error::{Error, Result};

/// 访问控制服务。
///
/// 提供权限目录、数据范围、用户角色绑定记录与审计事件的增补编排。
#[derive(Clone)]
pub struct AccessControlService {
    db: Database,
    rbac: crate::SharedRbacService,
    targets: Arc<dyn ScopeTargetPort>,
    authorization: Arc<AuthorizationService>,
}

impl AccessControlService {
    /// 创建已完整装配的访问控制服务。
    /// 可执行目录查询和范围配置的完整服务。
    ///
    /// 不执行 I/O，不返回错误。
    ///
    /// db 为身份数据库，rbac 为身份设施，targets 为外域目标校验端口，authorization 为共享授权服务。
    pub fn new(
        db: Database,
        rbac: crate::SharedRbacService,
        targets: Arc<dyn ScopeTargetPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, rbac, targets, authorization }
    }

    /// 分页查询权限定义列表（权限目录）。
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// 失败情形：
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    ///
    /// actor 为当前已认证账号。
    /// `params`：查询参数（`resource`/`disabled`/`system` 扁平筛选）。
    pub async fn permission_list(
        &self,
        params: &PermissionListParams,
        actor: &AuditActor,
    ) -> Result<PageView<PermissionView>> {
        let db = self.db.clone();
        let params = params.clone();
        let access = ManagementAccess::new(
            self.authorization.clone(),
            actor.clone(),
            &access_control::PERMISSION_LIST,
        );
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require(executor).await?;

                    params.validate()?;
                    let filter = params.normalized()?;
                    let page = db.permissions().search_permissions(&filter, executor).await?;
                    let items = page.items;
                    let result = Ok(PageView {
                        items,
                        total: page.total,
                        page: filter.paging.page,
                        page_size: filter.paging.page_size,
                    });

                    access.require(executor).await?;
                    result
                })
            })
            .await
    }

    /// 管理端登记权限定义时按统一值对象规范化 `resource:action`，再保存并返回管理视图。输入非法时拒绝；重复资源动作由唯一索引报冲突。
    pub async fn create_permission(
        &self,
        req: CreatePermissionRequest,
        actor: &AuditActor,
    ) -> Result<PermissionView> {
        let access = ManagementAccess::new(
            self.authorization.clone(),
            actor.clone(),
            &access_control::PERMISSION_CREATE,
        );
        access.preflight(&self.db).await?;

        req.validate()?;
        let permission = PermissionDefinition::new(PermissionId::new(next_id()), req.into_data())?;

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
        let permission = self
            .with_audited_write(event, access, move |executor| {
                Box::pin(async move {
                    db.permissions().create(&permission, executor).await?;
                    Ok(permission)
                })
            })
            .await?;
        Ok(permission.into())
    }

    /// 管理端修改权限定义的可变字段时，按请求中的期望版本写入并返回视图。资源动作和系统身份不可改；记录缺失或乐观锁不符时拒绝。
    pub async fn update_permission(
        &self,
        id: &str,
        req: UpdatePermissionRequest,
        actor: &AuditActor,
    ) -> Result<PermissionView> {
        let access = ManagementAccess::new(
            self.authorization.clone(),
            actor.clone(),
            &access_control::PERMISSION_UPDATE,
        );
        access.preflight(&self.db).await?;

        req.validate()?;
        let mut permission = self.load_permission_with_version(id, req.version).await?;
        let changed = req.changed_field_names();
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
        let updated = self
            .with_audited_write(event, access, move |executor| {
                Box::pin(async move {
                    db.permissions().update(&mut permission, executor).await?;
                    Ok(permission)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 管理端退役普通权限定义时只软删除，保留历史身份，避免复用破坏审计与绑定。不存在或为系统内建权限时拒绝。
    pub async fn delete_permission(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let access = ManagementAccess::new(
            self.authorization.clone(),
            actor.clone(),
            &access_control::PERMISSION_DELETE,
        );
        access.preflight(&self.db).await?;

        let mut permission = self
            .db
            .permissions()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("权限定义不存在".to_string()))?;
        permission.ensure_deletable()?;

        let event = self
            .build_audit_event(actor, "permission.delete", "permission", Some(id.to_string()), Vec::new())
            .await?;

        let db = self.db.clone();
        self.with_audited_write(event, access, move |executor| {
            Box::pin(async move {
                db.permissions().soft_delete(&mut permission, executor).await?;
                Ok(())
            })
        })
        .await
    }

    /// 按用户查询角色绑定（W19 用户授权，含撤权历史）。
    /// 返回按生效时间倒序排列的绑定视图。
    ///
    /// 失败情形：
    /// * `ValidationError` - 用户 ID 缺失
    ///
    /// actor 为当前已认证账号。
    /// `params`：查询参数（`user_id` 必填）。
    pub async fn user_role_list(
        &self,
        params: &UserRoleListParams,
        actor: &AuditActor,
    ) -> Result<Vec<UserRoleView>> {
        let db = self.db.clone();
        let params = params.clone();
        let access =
            ManagementAccess::new(self.authorization.clone(), actor.clone(), &access_control::USER_ROLE_LIST);
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require(executor).await?;
                    params.validate()?;
                    let items = db.user_roles().list_by_user(&params.user_id, executor).await?;
                    let result = Ok(items.into_iter().map(Into::into).collect());

                    access.require(executor).await?;
                    result
                })
            })
            .await
    }

    /// 分配用户角色（绑定记录 + 审计事件原子写入）。
    ///
    /// 目标角色必须存在（本域 `roles` 仓储读取）；同一用户同一角色同时仅一条
    /// 未撤权绑定由部分唯一索引 `uk_user_roles_active` 承担（撤权后再授权）。
    /// 授权事实仍由 Casbin `g` 规则承载（既有 `iam` 能力），本方法只维护
    /// `user_role` 绑定留痕表。
    ///
    /// 返回新建的绑定视图。
    ///
    /// 失败情形：
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 目标角色不存在
    /// * `ConflictError` - 同一用户同一角色已有未撤权绑定（唯一索引透出）
    pub async fn assign_user_role(
        &self,
        req: AssignUserRoleRequest,
        actor: &AuditActor,
    ) -> Result<UserRoleView> {
        let access = ManagementAccess::new(
            self.authorization.clone(),
            actor.clone(),
            &access_control::USER_ROLE_CREATE,
        );
        access.preflight(&self.db).await?;

        let data = req.into_data(actor.id())?;
        self.db
            .roles()
            .find_by_id(data.role_id.as_str(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("角色不存在".to_string()))?;
        let binding = UserRole::new(UserRoleId::new(next_id()), data)?;

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
        let binding = self
            .with_audited_write(event, access, move |executor| {
                Box::pin(async move {
                    db.user_roles().create(&binding, executor).await?;
                    Ok(binding)
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
    /// 返回撤权后的绑定视图。
    ///
    /// 失败情形：
    /// * `NotFound` - 绑定不存在
    /// * `ConflictError` - 并发修改或绑定已撤权
    ///
    /// `req`：撤权命令（只携带业务原因，当前版本由服务端读取）。
    pub async fn revoke_user_role(
        &self,
        id: &str,
        req: RevokeUserRoleRequest,
        actor: &AuditActor,
    ) -> Result<UserRoleView> {
        let access = ManagementAccess::new(
            self.authorization.clone(),
            actor.clone(),
            &access_control::USER_ROLE_REVOKE,
        );
        access.preflight(&self.db).await?;

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
        let id = id.to_string();
        let revoked_by = actor.id().to_string();
        let revoke_data = req.into_revoke_data();
        let updated = self
            .with_audited_write(event, access, move |executor| {
                Box::pin(async move {
                    let mut binding = db
                        .user_roles()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("用户角色绑定不存在".to_string()))?;
                    binding.revoke(revoke_data, &revoked_by, Instant::now())?;
                    db.user_roles().update(&mut binding, executor).await?;
                    Ok(binding)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 分页查询审计事件（W19 §5.2 审计查询）。
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// 失败情形：
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    ///
    /// actor 为当前已认证账号。
    /// `params`：查询参数（`actor_id`/`action_type`/`object_type`/`result` 等）。
    pub async fn audit_event_list(
        &self,
        params: &AuditEventListParams,
        actor: &AuditActor,
    ) -> Result<PageView<AuditEventView>> {
        let db = self.db.clone();
        let params = params.clone();
        let access = ManagementAccess::new(
            self.authorization.clone(),
            actor.clone(),
            &access_control::AUDIT_EVENT_LIST,
        );
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require(executor).await?;

                    params.validate()?;
                    let filter = params.normalized()?;
                    let page = db.audit_events().search_audit_events(&filter, executor).await?;
                    let items = page.items.into_iter().map(AuditEventView::from).collect();
                    let result = Ok(PageView {
                        items,
                        total: page.total,
                        page: filter.paging.page,
                        page_size: filter.paging.page_size,
                    });

                    access.require(executor).await?;
                    result
                })
            })
            .await
    }

    // 按 ID 加载权限定义并校验期望版本。
    // 返回加载的权限定义实体。
    //
    // 权限不存在返回 `NotFound`；版本不一致返回 `ConflictError`。
    //
    // `id`：权限定义 ID；`expected_version`：请求携带的期望版本。
    async fn load_permission_with_version(
        &self,
        id: &str,
        expected_version: u64,
    ) -> Result<PermissionDefinition> {
        let permission = self
            .db
            .permissions()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("权限定义不存在".to_string()))?;
        if permission.base.version != expected_version {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }
        Ok(permission)
    }

    // 构造本域写入的审计事件（`audit_log → audit_event` 字段对齐形态）。
    //
    // 操作者名称快照取账号 `name`（账号不存在时退回登录账号语义的空安全
    // 快照）；责任角色快照取账号类型；结果恒为成功。
    //
    // 返回新建的审计事件。
    //
    // 审计字段违反领域约束时返回错误。
    //
    // `changed_field_names`：变更字段名（只记录字段名和「已变更」）。
    async fn build_audit_event(
        &self,
        actor: &AuditActor,
        action_type: &str,
        object_type: &str,
        object_id: Option<String>,
        changed_field_names: Vec<String>,
    ) -> Result<AuditEvent> {
        if !crate::entity::access_control::audit_actions::AUDIT_ACTIONS
            .iter()
            .any(|(code, _)| *code == action_type)
        {
            return Err(Error::ValidationError("审计写入动作未登记".into()));
        }

        let account = self.db.accounts().find_by_id(actor.id(), &mut NoTransaction).await?;
        let actor_label = account.map(|account| account.name).unwrap_or_else(|| "系统操作人".to_string());
        Ok(AuditEvent::new(
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
        )?)
    }

    // 在一个事务内执行业务写入，成功后再追加审计事件。
    //
    // 调用方闭包负责业务写入；范围场景须在闭包内完成授权校验与
    // `bump_policy_revision`。顺序固定为「业务写入 →（范围场景）revision bump → 审计」。
    async fn with_audited_write<T, F>(
        &self,
        event: AuditEvent,
        access: ManagementAccess,
        write: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut dyn Executor) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
            + Send
            + 'static,
    {
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require(executor).await?;

                    let value = write(executor).await?;
                    db.audit_events().create(&event, executor).await?;
                    Ok(value)
                })
            })
            .await
    }
}

pub mod inspection;
