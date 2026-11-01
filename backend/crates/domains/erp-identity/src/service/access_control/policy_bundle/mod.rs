//! 授权 JSON 的只读预览与单事务应用入口。
mod apply;
mod export;
mod facts;

use application_core::AuditActor;
use persistence_core::{Executor, Transactional};

use super::AccessControlService;
use super::resolve::{AuthorizedDataScope, DataScopeService};
use crate::dto::authorization_bundle::PolicyPreview;
use crate::entity::authorization_bundle::PolicyDocument;
use crate::entity::authorization_bundle::plan::PolicyPlan;
use crate::{Error, Permission, Result};

/// 持有当前版本权限目录及既有范围配置端口。
#[derive(Clone)]
pub struct PolicyBundleService {
    access: AccessControlService,
    catalog: Vec<Permission>,
}

impl PolicyBundleService {
    /// 装配统一授权用例。
    /// # 参数
    /// access 为已装配目标校验的范围服务；catalog 为运行版本生成的权限目录。
    /// # 返回
    /// 复用身份仓储及授权事务的配置服务。
    /// # 错误
    /// 无；缺少依赖或权限目录错误在调用入口拒绝。
    pub fn new(access: AccessControlService, catalog: Vec<Permission>) -> Self {
        Self { access, catalog }
    }

    /// 只读生成可提交的配置计划；不持久化预览。
    /// # 参数
    /// document 为完整文件，actor 为已认证后台身份。
    /// # 返回
    /// 规范化文件、版本、审核摘要和逐项差异。
    /// # 错误
    /// 越权、未知权限、目标失效、复杂旧配置或读取失败时拒绝。
    pub async fn preview(&self, document: PolicyDocument, actor: AuditActor) -> Result<PolicyPreview> {
        let document = document.normalized()?;
        document.validate_catalog(&self.catalog)?;
        let service = self.clone();
        self.access
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let authorization = service.authorize(&document, &actor, executor).await?;
                    Ok(service.plan(document, &actor, authorization, executor).await?.preview)
                })
            })
            .await
    }

    /// 所有写配置能力在开始时一次性证明；同批增权不能成为后续授权依据。
    async fn authorize(
        &self,
        document: &PolicyDocument,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let mut codes = vec![
            "authorization_policy:preview",
            "authorization_policy:apply",
            "admin:list",
            "role:list",
            "data_scope:list",
        ];
        if !document.roles.is_empty() {
            codes.extend(["role:create", "role:update"]);
        }
        if !document.bindings.is_empty() {
            codes.push("admin:update");
        }
        if !document.data_scopes.is_empty() {
            codes.push("data_scope:create");
        }
        self.company_access(actor, &codes, "manage", executor).await
    }

    /// 按既有同角色完整资格及公司组织边界证明配置入口。
    async fn company_access(
        &self,
        actor: &AuditActor,
        codes: &[&str],
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let permissions = codes.iter().map(Permission::parse).collect::<std::result::Result<Vec<_>, _>>()?;
        let access = DataScopeService::new(self.access.db.clone(), self.access.scope_rbac()?)
            .resolve_permissions(actor, "org_unit", action, &permissions, executor)
            .await?;
        if !access.scope.role_clauses.iter().any(|clause| clause.company)
            || access.scope.user_limit.as_ref().is_some_and(|clause| !clause.company)
        {
            return Err(Error::Forbidden("授权文件操作需要公司范围的组织配置或查看资格".into()));
        }
        Ok(access)
    }

    /// 读取真实事实，复用目标校验，编译批次最终权限。
    async fn plan(
        &self,
        document: PolicyDocument,
        actor: &AuditActor,
        authorization: AuthorizedDataScope,
        executor: &mut dyn Executor,
    ) -> Result<PolicyPlan> {
        let facts = self.facts(&document, executor).await?;
        for scope in &document.data_scopes {
            self.access
                .validate_person_scope_targets(
                    &scope.user_id,
                    &scope.request(authorization.policy_version)?,
                    &authorization.organizations,
                    executor,
                )
                .await?;
        }
        let rbac = self.access.scope_rbac()?;
        let actor_permissions = rbac.bundle_actor_permissions(actor, executor).await?;
        rbac.ensure_policy_snapshot_with_executor(authorization.policy_version, executor).await?;
        PolicyPlan::build(
            document,
            &facts,
            actor.id(),
            &actor_permissions,
            authorization.policy_version,
            &authorization.scope_version,
        )
    }
}
