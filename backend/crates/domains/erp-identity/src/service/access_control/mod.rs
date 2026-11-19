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

/// 管理端访问控制用例，负责权限目录、人员范围、角色绑定留痕和审计查询；实际角色授权仍由 IAM 处理。
#[derive(Clone)]
pub struct AccessControlService {
    // 身份仓储及管理用例事务使用的数据库。
    db: Database,
    // 提供角色资格快照、策略版本及策略写事务的共享 RBAC 服务。
    rbac: crate::SharedRbacService,
    // 保存人员范围前校验仓库、结算主体等外域目标的端口。
    targets: Arc<dyn ScopeTargetPort>,
    // 检查目录、范围、绑定留痕和审计等管理入口的统一授权服务。
    authorization: Arc<AuthorizationService>,
}

impl AccessControlService {
    /// 应用装配时注入身份数据库、RBAC、外域目标校验端口和共享授权服务；不执行 I/O。
    pub fn new(
        db: Database,
        rbac: crate::SharedRbacService,
        targets: Arc<dyn ScopeTargetPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, rbac, targets, authorization }
    }

    /// 管理端打开权限目录时，按资源、停用状态和系统标识查询定义，在同一读取事务内检查权限并在返回前复验。
    ///
    /// 分页参数非法或排序字段不在白名单时返回 `ValidationError`；查询失败返回 `RepositoryError`。
    /// 返回 `items`、`total`、`page` 和 `page_size`。
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

    /// 管理端新增权限定义时，按权限值对象规范化 `resource:action`，将定义和审计一并写入，返回新定义。
    /// 输入非法时拒绝；资源与动作重复时由唯一索引报告冲突。
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

    /// 管理端编辑权限定义时，按请求的期望版本更新可变字段并记录审计，返回更新后的定义。
    /// 资源、动作及系统身份不可改；记录缺失或乐观锁冲突时拒绝。
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

    /// 管理端退役权限定义时，软删除普通定义并记录审计，保留历史标识供审计和绑定引用。
    /// 记录不存在或属于系统内建权限时拒绝。
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

    /// 管理端查看 W19 用户授权记录时，按生效时间倒序返回指定用户的绑定留痕，含已撤销记录。
    /// `user_id` 为空时返回 `ValidationError`；同一读取事务内在返回前复验查询资格。
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

    /// 管理端登记角色分配时，原子写入绑定留痕和审计事件，返回新绑定；不修改 Casbin `g` 规则。
    ///
    /// 请求非法返回 `ValidationError`，角色不存在返回 `NotFound`。
    /// 同一用户与角色只允许一条未撤销绑定；部分唯一索引 `uk_user_roles_active` 报告 `ConflictError`，撤销后可重新登记。
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

    /// 管理端撤销指定绑定记录时，要求结构化业务原因；在同一事务读取当前版本、乐观锁更新并写审计，返回保留的撤销记录。
    /// 此入口维护绑定留痕；要立即取消实际访问资格，调用方须通过账号角色管理更新 Casbin 策略。
    ///
    /// 绑定不存在返回 `NotFound`；并发更新冲突或记录已撤销时拒绝，请求无需提供版本。
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

    /// 管理端打开 W19 §5.2 审计查询时，按操作者、动作类型、对象类型或结果等条件分页，在同一读取事务内复验后交付。
    ///
    /// 分页参数非法或排序字段不在白名单时返回 `ValidationError`；查询失败返回 `RepositoryError`。
    /// 返回 `items`、`total`、`page` 和 `page_size`。
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

    // 编辑权限前按 ID 加载定义，与请求版本比对；定义缺失报 `NotFound`，版本不符报 `ConflictError`。
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

    // 写事务前构造与 `audit_log` 字段对齐的成功审计事件；动作未登记或字段不合规时拒绝。
    // 操作者快照取账号名称（无账号时用「系统操作人」）和账号类型；变更只留字段名，不留字段值。
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

    // 管理写入预检后开启事务，先复验入口权限，再执行业务闭包，成功才写审计；失败则一并回滚。
    // 若闭包处理范围变更，须在其中校验范围授权，并在业务写入后、追加审计前推进策略版本。
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
