//! 显式生成岗位角色，整批授权、整批事务及既有身份保护。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use application_core::AuditActor;
use erp_permissions::admin;
use persistence_core::{Executor, Transactional};

use super::RbacService;
use super::policy::{permission_pairs, permissions_for_actor, role_key};
use crate::dto::{BuiltinRoleCatalog, BuiltinRoleOption, GenerateBuiltinRolesRequest, GeneratedBuiltinRole};
use crate::entity::role_template::{BuiltinRoleState, BuiltinRoleTemplate};
use crate::entity::role_workbench::RoleWorkbench;
use crate::entity::{Role, RoleData};
use crate::service::iam::builtin_role_templates;
use crate::service::iam::management::ManagementAccess;
use crate::{AccessControlExt, AuthorizationService, Error, Result, RoleId};

impl RbacService {
    /// 生成岗位前预览固定模板的权限、已有角色状态、可生成标记及提交所需的策略版本。
    /// 在预览事务中检查入口资格；资格不足、策略变化或存储失败时返回错误。
    pub async fn builtin_role_catalog(
        self: &Arc<Self>,
        actor: &AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<BuiltinRoleCatalog> {
        let service = self.clone();
        let actor = actor.clone();
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ROLE_LIST_ROLES);
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require(executor).await?;
                    let catalog = service.role_catalog(&actor, executor).await?;
                    access.require(executor).await?;
                    Ok(catalog)
                })
            })
            .await
    }

    // 装配目录时沿原事务读取固定角色的状态；仅当角色缺失、操作人可创建且覆盖全部模板权限时标记可生成。
    // 返回前复核用于计算标记的策略版本。
    async fn role_catalog(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BuiltinRoleCatalog> {
        let (permissions, policy_version) = {
            let enforcer = self.fresh_enforcer().await?.read().await;
            (permissions_for_actor(&enforcer, actor)?, self.loaded_policy_revision.load(Ordering::Acquire))
        };
        let can_create = permissions.covers_one(&admin::ROLE_CREATE_ROLE);

        let mut templates = Vec::new();
        for template in builtin_role_templates()? {
            let role = self.db.roles().find_by_id_including_deleted(&template.id, executor).await?;
            let state = BuiltinRoleState::from_role(role.as_ref());
            templates.push(BuiltinRoleOption {
                can_generate: can_create
                    && state == BuiltinRoleState::Missing
                    && template.permissions.iter().all(|required| permissions.covers_one(required)),
                template,
                state,
                existing_name: role.map(|role| role.name),
            });
        }

        if self.current_policy_revision().await? != policy_version {
            return Err(Error::ConflictError("权限配置已变化，请重新加载岗位模板".into()));
        }
        Ok(BuiltinRoleCatalog { policy_version, templates })
    }

    /// 提交预览版本及模板选择后，按服务端模板原子生成整批岗位，逐项返回新建或保留结果。
    /// 仅创建缺失角色；选择非法、超出授予上限、版本冲突或任一写入失败时整批拒绝。
    pub async fn generate_builtin_roles(
        self: &Arc<Self>,
        request: GenerateBuiltinRolesRequest,
        actor: AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<Vec<GeneratedBuiltinRole>> {
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ROLE_CREATE_ROLE);
        access.preflight(&self.db).await?;
        let selected = BuiltinRoleTemplate::select(&builtin_role_templates()?, &request.template_ids)?;
        let mut required = vec![admin::ROLE_CREATE_ROLE];
        required.extend(selected.iter().flat_map(|template| template.permissions.iter().cloned()));
        let authorized = self.authorize_permissions(&actor, required).await?;
        if authorized.policy_revision != request.expected_policy_version {
            return Err(Error::ConflictError("权限配置已变化，请重新加载岗位模板后生成".into()));
        }

        let service = Arc::clone(self);
        self.run_authorized_policy_transaction(authorized.policy_revision, move |executor| {
            Box::pin(async move {
                access.require(executor).await?;
                let mut results = Vec::new();
                for template in selected {
                    results.push(service.generate_builtin_role(template, &actor, executor).await?);
                }
                Ok::<_, Error>(results)
            })
        })
        .await
    }

    // 在批量事务中按固定 ID 重查角色，保留已有角色（含停用和软删除）；缺失时保存角色、权限和审计。
    async fn generate_builtin_role(
        &self,
        template: BuiltinRoleTemplate,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<GeneratedBuiltinRole> {
        let existing = self.db.roles().find_by_id_including_deleted(&template.id, executor).await?;
        let state = BuiltinRoleState::from_role(existing.as_ref());
        if let Some(role) = existing {
            return Ok(GeneratedBuiltinRole { id: role.base.id, name: role.name, created: false, state });
        }

        let mut data = RoleData::new(template.name).with_description(template.description);
        data.workbenches = RoleWorkbench::recommended(&template.id);
        let role = Role::new(RoleId::parse(template.id)?, data)?;
        let audit = self.audit.resource_log(actor.clone(), "role.create", "role", role.base.id.clone())?;

        self.db.roles().create(&role, executor).await?;
        self.policy_store
            .replace_role_permissions(
                &role_key(&role.base.id),
                &permission_pairs(template.permissions),
                executor,
            )
            .await?;
        self.audit.persist(&audit, executor).await?;
        Ok(GeneratedBuiltinRole {
            id: role.base.id,
            name: role.name,
            created: true,
            state: BuiltinRoleState::Existing,
        })
    }
}
