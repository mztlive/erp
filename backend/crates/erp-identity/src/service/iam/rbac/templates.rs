//! 显式生成岗位角色，整批授权、整批事务及既有身份保护。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use application_core::AuditActor;
use persistence_core::{Executor, NoTransaction};

use super::RbacService;
use super::policy::{permission_pairs, permissions_for_actor, role_key};
use crate::dto::{BuiltinRoleCatalog, BuiltinRoleOption, GenerateBuiltinRolesRequest, GeneratedBuiltinRole};
use crate::entity::role_template::{BuiltinRoleState, BuiltinRoleTemplate};
use crate::entity::{Permission, PermissionSet, Role, RoleData};
use crate::service::iam::builtin_role_templates;
use crate::{AccessControlExt, Error, Result};

impl RbacService {
    /// 查询可审阅的岗位权限及当前生成状态。
    /// # 参数
    /// `actor` 为当前操作人。
    /// # 返回
    /// 固定目录、可生成标记及权限版本。
    /// # 错误
    /// 读取权限不足、策略变化或存储失败时拒绝。
    pub async fn builtin_role_catalog(&self, actor: &AuditActor) -> Result<BuiltinRoleCatalog> {
        let (permissions, policy_version) = {
            let enforcer = self.fresh_enforcer().await?.read().await;
            (permissions_for_actor(&enforcer, actor)?, self.loaded_policy_revision.load(Ordering::Acquire))
        };
        if !permissions.covers_one(&Permission::parse("role:list")?) {
            return Err(Error::Forbidden("缺少角色查看权限".into()));
        }
        let can_create = permissions.covers_one(&Permission::parse("role:create")?);
        let mut templates = Vec::new();
        for template in builtin_role_templates()? {
            let role = self.db.roles().find_by_id_including_deleted(&template.id, &mut NoTransaction).await?;
            let state = BuiltinRoleState::from_role(role.as_ref());
            templates.push(BuiltinRoleOption {
                can_generate: can_create
                    && state == BuiltinRoleState::Missing
                    && permissions.covers(&PermissionSet::new(template.permissions.clone())),
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

    /// 以服务端模板原子生成全部所选岗位，任何既有身份保持不变。
    /// # 参数
    /// `request` 为已预览的模板选择及版本；`actor` 为发起人。
    /// # 返回
    /// 每个岗位的新建或保留结果。
    /// # 错误
    /// 模板、授予上限、版本或任一写入失败时整批拒绝。
    pub async fn generate_builtin_roles(
        self: &Arc<Self>,
        request: GenerateBuiltinRolesRequest,
        actor: AuditActor,
    ) -> Result<Vec<GeneratedBuiltinRole>> {
        let selected = BuiltinRoleTemplate::select(&builtin_role_templates()?, &request.template_ids)?;
        let mut required = vec![Permission::parse("role:create")?];
        required.extend(selected.iter().flat_map(|template| template.permissions.iter().cloned()));
        let authorized = self.authorize_permissions(&actor, required).await?;
        if authorized.policy_revision != request.expected_policy_version {
            return Err(Error::ConflictError("权限配置已变化，请重新加载岗位模板后生成".into()));
        }
        let service = Arc::clone(self);
        self.run_authorized_policy_transaction(authorized.policy_revision, move |executor| {
            Box::pin(async move {
                let mut results = Vec::new();
                for template in selected {
                    results.push(service.generate_builtin_role(template, &actor, executor).await?);
                }
                Ok::<_, Error>(results)
            })
        })
        .await
    }

    /// 在调用方事务内重验固定身份，写入角色、权限和审计。
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
        let role =
            Role::new(template.id, RoleData::new(template.name).with_description(template.description))?;
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
