//! 授权 JSON 的只读预览与单事务应用入口。
mod apply;
mod export;
mod facts;

use application_core::AuditActor;
use erp_permissions::access_control;
use persistence_core::{Executor, Transactional};

use super::AccessControlService;
use super::resolve::AuthorizedDataScope;
use crate::dto::authorization_bundle::PolicyPreview;
use crate::entity::authorization_bundle::PolicyDocument;
use crate::entity::authorization_bundle::plan::PolicyPlan;
use crate::{Permission, Result};

/// 持有当前版本权限目录及既有范围配置端口。
#[derive(Clone)]
pub struct PolicyBundleService {
    access: AccessControlService,
    catalog: Vec<Permission>,
}

impl PolicyBundleService {
    /// 装配统一授权用例。
    /// 复用身份仓储及授权事务的配置服务。
    ///
    /// 缺少依赖或权限目录错误在调用入口拒绝。
    ///
    /// access 为已装配目标校验的范围服务；catalog 为运行版本生成的权限目录。
    pub fn new(access: AccessControlService, catalog: Vec<Permission>) -> Self {
        Self { access, catalog }
    }

    /// 只读生成可提交的配置计划；不持久化预览。
    /// 规范化文件、版本、审核摘要和逐项差异。
    ///
    /// 越权、未知权限、目标失效、复杂旧配置或读取失败时拒绝。
    ///
    /// document 为完整文件，actor 为已认证后台身份。
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
                    let authorization = service
                        .access
                        .authorization
                        .require_policy_bundle(
                            &actor,
                            &access_control::POLICY_BUNDLE_PREVIEW,
                            Some(&document),
                            executor,
                        )
                        .await?;

                    let plan = service.plan(document, &actor, &authorization.scope, executor).await?;
                    authorization.ensure_current(executor)?;
                    Ok(plan.preview)
                })
            })
            .await
    }

    // 为预览或应用按真实目标事实重新编译批次最终权限，复用范围目标存在性校验，不信任文件自报资格。
    async fn plan(
        &self,
        document: PolicyDocument,
        actor: &AuditActor,
        authorization: &AuthorizedDataScope,
        executor: &mut dyn Executor,
    ) -> Result<PolicyPlan> {
        let facts = self.facts(&document, executor).await?;
        for scope in &document.data_scopes {
            self.access
                .validate_person_scope_targets(
                    &scope.request(authorization.policy_version)?,
                    &authorization.organizations,
                    executor,
                )
                .await?;
        }

        let rbac = self.access.rbac.clone();
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
