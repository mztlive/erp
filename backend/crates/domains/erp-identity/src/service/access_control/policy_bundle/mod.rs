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

/// 预览、应用和导出授权配置文件时，统一复用权限目录与人员范围配置能力。
#[derive(Clone)]
pub struct PolicyBundleService {
    // 提供授权、事务和人员范围配置能力，供各文件入口复用。
    access: AccessControlService,
    // 当前运行版本生成的可配置权限目录，供各入口校验文件内的角色权限。
    catalog: Vec<Permission>,
}

impl PolicyBundleService {
    /// 装配文件入口时，传入已接入目标校验的范围服务和当前运行版本生成的权限目录。
    /// 各入口处理文件时再检查所需依赖和目录约束。
    pub fn new(access: AccessControlService, catalog: Vec<Permission>) -> Self {
        Self { access, catalog }
    }

    /// 应用前，由已认证后台人员提交完整文件，取得只读预览：规范化文件、策略版本、审核摘要和逐项差异。
    ///
    /// 不持久化预览，交付前复验原读取阶段和授权有效期。越权、未知权限、目标失效、旧配置无法处理或读取失败时返回错误。
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

    // 预览或应用时沿原事务读取真实目标事实，复用范围目标存在性校验，再按操作人当前权限计算批次最终配置和审核摘要。
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
