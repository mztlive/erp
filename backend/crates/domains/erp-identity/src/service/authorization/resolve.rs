use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{internal, supplier, work_item};
use persistence_core::Executor;

use super::{AuthorizationService, rules};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::{AccessContext, AccessRule, QueryModel, QueryOutput};
use crate::entity::fingerprint::Fingerprint;
use crate::repository::authorization::AccessProof;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, QualifiedPermissions, Result};

#[derive(Default)]
pub(super) struct Resolution {
    pub(super) versions: Vec<String>,
    pub(super) valid_until: Option<Instant>,
    pub(super) organization_version: u64,
    pub(super) whole_rule: Option<AccessRule>,
    pub(super) has_role_scope: bool,
}

impl AuthorizationService {
    /// 检查目标账号的完整功能资格，并按该资格解析本动作的固定范围来源。
    /// # 参数
    /// actor、permission 为待检查身份及动作；executor 为检查人的原读取事务。
    /// # 返回
    /// 范围动作返回本次合格角色的范围；其他动作返回 None，仍须取得对象或任务上下文。
    /// # 错误
    /// 账号失效、完整资格不足、未知动作及范围设施错误原样传播。
    pub(crate) async fn inspection_scope(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<Option<AuthorizedDataScope>> {
        let qualified = self.qualify(actor, permission, executor).await?;
        if scope_spec::kind(permission)? != ActionKind::Scope {
            return Ok(None);
        }
        let context = AccessContext {
            actor: actor.clone(),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: None,
            as_of: Instant::now(),
        };
        self.resolve_scope(&context, permission, &qualified, executor).await.map(Some)
    }

    /// 身份先于业务事实读取；全部资格由一次账号及启用角色取证完成。
    pub(super) async fn qualify(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<QualifiedPermissions> {
        self.qualify_target(actor, permission, false, executor).await
    }

    async fn qualify_target(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        task: bool,
        executor: &mut dyn Executor,
    ) -> Result<QualifiedPermissions> {
        let required = if task { rules::task_permissions(permission) } else { Vec::new() };
        let mut requested = rules::qualifications(permission);
        requested.extend(required.iter().cloned());
        requested.sort();
        requested.dedup();
        self.permissions(actor, &requested, executor)
            .await?
            .filter(|qualified| {
                rules::qualified(permission, qualified)
                    && (!task
                        || permission == &internal::WORK_ITEM_RECIPIENT
                        || qualified.same_role(&required))
            })
            .ok_or_else(|| Error::Forbidden("账号或启用角色不具备本操作的完整权限".into()))
    }

    /// 保存本次精确任务语义，使非事务复验不能退回普通记录规则。
    pub(super) async fn resolve_target(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        model: Option<&QueryModel>,
        as_of: Option<Instant>,
        task: bool,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let qualified = self.qualify_target(actor, permission, task, executor).await?;
        let context = AccessContext {
            actor: actor.clone(),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: model.map(|model| model.key.clone()),
            as_of: as_of.unwrap_or_else(Instant::now),
        };
        self.resolve_context(context, model, task, &qualified, executor).await
    }

    /// 复用同次账号与角色资格解析固定来源，保持原读取阶段。
    pub(super) async fn resolve_context(
        &self,
        context: AccessContext,
        model: Option<&QueryModel>,
        task: bool,
        qualified: &QualifiedPermissions,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let boundary = executor
            .boundary()
            .ok_or_else(|| Error::ValidationError("授权要求可核验的事务或读取阶段".into()))?;
        let mut resolution = Resolution { versions: vec![qualified.version().into()], ..Default::default() };
        let rule = self
            .target_condition(
                &context,
                qualified,
                model.map(|model| &model.output),
                task,
                &mut resolution,
                executor,
            )
            .await?;
        let management_rule =
            self.management_condition(&context, qualified, &mut resolution, executor).await?;
        resolution.versions.extend(
            self.grant_versions(&context.actor, &rule, resolution.whole_rule.as_ref(), executor).await?,
        );
        let mut version = Fingerprint::new("authorization-proof-v3");
        version.unordered(&resolution.versions, |hash, value| hash.text(value));
        version.rule(&rule);
        version.optional(resolution.whole_rule.as_ref(), Fingerprint::rule);
        version.optional(management_rule.as_ref(), Fingerprint::rule);
        version.boolean(task);
        version.optional(model, Fingerprint::model);
        let proof = AccessProof {
            context,
            qualified: qualified.clone(),
            approval_source: None,
            task,
            rule,
            whole_rule: resolution.whole_rule,
            management_rule,
            has_role_scope: resolution.has_role_scope,
            version: version.finish(),
            valid_until: resolution.valid_until,
            policy_version: qualified.policy_revision(),
            organization_version: resolution.organization_version,
            boundary,
            service: self.token.clone(),
        };
        proof.ensure_current(executor)?;
        Ok(proof)
    }

    /// 可选管理范围使用自己的合格角色，与根读取共享身份、有效期和读取阶段。
    async fn management_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccessRule>> {
        let Some(permission) =
            context.model.as_ref().and_then(|_| rules::management_permission(&context.permission))
        else {
            return Ok(None);
        };
        if !qualified.same_role(&rules::same_role_permissions(&permission)) {
            return Ok(Some(AccessRule::Deny));
        }
        let context = AccessContext { permission, ..context.clone() };
        let scope = self.resolve_scope(&context, &context.permission, qualified, executor).await?;
        let has_role_scope = resolution.has_role_scope;
        resolution.include(&scope);
        resolution.has_role_scope = has_role_scope;
        Ok(Some(rules::data_condition(&context.permission, Some(&scope.scope))?))
    }

    async fn grant_versions(
        &self,
        actor: &AuditActor,
        rule: &AccessRule,
        whole_rule: Option<&AccessRule>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut resources = BTreeSet::new();
        rule.grant_resources(&mut resources);
        if let Some(whole) = whole_rule {
            whole.grant_resources(&mut resources);
        }
        let mut versions = Vec::new();
        for resource in resources {
            let revision = self.facilities.grant_revision(actor, resource, executor).await?;
            versions.push(format!("{}:{revision}", resource.code()));
        }
        Ok(versions)
    }

    /// 任务保留入口基础规则，并追加固定关系；普通目标继续原数据条件。
    async fn target_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        output: Option<&QueryOutput>,
        task: bool,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        if !task {
            let base = self.condition(context, qualified, output, resolution, executor).await?;
            return if context.permission == work_item::DETAIL && context.model.is_some() {
                self.work_item_query_condition(context, qualified, resolution, base, executor).await
            } else {
                Ok(base)
            };
        }
        let base = if scope_spec::kind(&context.permission)? == ActionKind::Function {
            None
        } else {
            Some(self.condition(context, qualified, None, resolution, executor).await?)
        };
        rules::task_condition(&context.permission, base)
    }

    async fn condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        output: Option<&QueryOutput>,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        if super::materials::supports(&context.permission) {
            return self.material_condition(context, qualified, resolution, executor).await;
        }
        if context.permission == internal::CONNECTION_CATALOG_READ {
            return self.supplier_catalog_scope(context, qualified, resolution, executor).await;
        }
        if context.permission == internal::APPROVAL_MANAGEMENT_READ && context.model.is_some() {
            return self.approval_query_condition(context, qualified, resolution, executor).await;
        }
        match scope_spec::kind(&context.permission)? {
            ActionKind::Scope => {
                let scope = self.resolve_scope(context, &context.permission, qualified, executor).await?;
                resolution.include(&scope);
                let base = rules::data_condition(&context.permission, Some(&scope.scope))?;
                let mut scopes = Vec::new();
                for (entry, source) in rules::selection::scope_requests(&context.permission)
                    .into_iter()
                    .chain(rules::quality::scope_requests(&context.permission))
                {
                    if !qualified.same_role(&rules::same_role_permissions(&entry)) {
                        continue;
                    }
                    let source_context = AccessContext { permission: entry.clone(), ..context.clone() };
                    let scope = self.resolve_scope(&source_context, &source, qualified, executor).await?;
                    resolution.include(&scope);
                    scopes.push((entry, source, scope.scope.clone()));
                }
                rules::quality::condition(
                    &context.permission,
                    qualified,
                    &scopes,
                    rules::selection::condition(&context.permission, qualified, &scopes, base)?,
                )
            },
            ActionKind::Task => Ok(rules::records::resolved(
                &context.permission,
                qualified,
                rules::data_condition(&context.permission, None)?,
            )),
            ActionKind::Source => {
                self.source_condition(context, qualified, output, resolution, executor).await
            },
            ActionKind::Function => Err(Error::ValidationError("无对象操作不解析数据条件".into())),
        }
    }

    /// 商品目录的供应商范围来自其独立详情角色，不与连接菜单角色拼接。
    async fn supplier_catalog_scope(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        let source =
            AccessContext { permission: supplier::DETAIL, resource: "supplier".into(), ..context.clone() };
        let scope = self.resolve_scope(&source, &supplier::DETAIL, qualified, executor).await?;
        resolution.include(&scope);
        rules::supplier_catalog_condition(rules::data_condition(&supplier::DETAIL, Some(&scope.scope))?)
    }

    async fn source_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        output: Option<&QueryOutput>,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        let mut scopes = Vec::new();
        let mut used = BTreeSet::new();
        for (entry, source) in rules::sources::scope_requests(&context.permission)? {
            if !used.insert((entry.clone(), source.clone())) {
                continue;
            }
            if !qualified.same_role(&rules::same_role_permissions(&entry)) {
                continue;
            }
            let source_context = AccessContext { permission: entry.clone(), ..context.clone() };
            let scope = self.resolve_scope(&source_context, &source, qualified, executor).await?;
            resolution.include(&scope);
            scopes.push((entry, source, scope.scope.clone()));
        }
        let output = match output {
            None | Some(QueryOutput::Object) => rules::sources::Output::Object,
            Some(_) => rules::sources::Output::Share,
        };
        resolution.whole_rule = Some(rules::sources::scoped_condition(
            &context.permission,
            qualified,
            &scopes,
            rules::sources::Output::Object,
        )?);
        rules::sources::scoped_condition(&context.permission, qualified, &scopes, output)
    }

    pub(super) async fn resolve_scope(
        &self,
        context: &AccessContext,
        source: &Permission,
        qualified: &QualifiedPermissions,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let origin = scope_spec::runtime_source(scope_spec::require_scope(source)?)?.permission();
        self.facilities
            .scope(context, origin, &rules::same_role_permissions(&context.permission), qualified, executor)
            .await
    }
}

impl Resolution {
    pub(super) fn include(&mut self, scope: &AuthorizedDataScope) {
        self.has_role_scope |= scope.scope.has_role_scope();
        self.versions.push(scope.scope_version.clone());
        self.organization_version = self.organization_version.max(scope.organizations.version);
        for membership in &scope.organizations.memberships {
            for until in
                [Some(membership.validity.valid_from), membership.validity.valid_to].into_iter().flatten()
            {
                if until > scope.as_of {
                    self.valid_until = Some(self.valid_until.map_or(until, |old| old.min(until)));
                }
            }
        }
    }
}
