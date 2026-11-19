use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{internal, supplier, work_item};
use persistence_core::Executor;

use super::{AuthorizationService, rules};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::{AccessContext, AccessRule, QueryModel, QueryOutput};
use crate::entity::fingerprint::Fingerprint;
use crate::repository::authorization::{
    AccessProof, AccessValidity, Replay, ResolvedAccess, ResolvedFilter, ResolvedRules, ScopeMetadata,
};
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, QualifiedPermissions, Result};

#[derive(Default)]
pub(super) struct Resolution {
    pub(super) versions: Vec<String>,
    pub(super) valid_until: Option<Instant>,
    pub(super) organization_version: u64,
    pub(super) has_role_scope: bool,
}

impl AuthorizationService {
    // 检查目标账号的完整功能资格，并按该资格解析本动作的固定范围来源。
    // 范围动作返回本次合格角色的范围；其他动作返回 None，仍须取得对象或任务上下文。
    //
    // 账号失效、完整资格不足、未知动作及范围设施错误原样传播。
    //
    // actor、permission 为待检查身份及动作；executor 为检查人的原读取事务。
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

    // 统一授权读取业务对象前，一次取证账号及全部可用启用角色，为各固定分支提供同一身份快照。
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
        let generic_denial = || Error::Forbidden("账号或启用角色不具备本操作的完整权限".into());
        let qualified = self.permissions(actor, &requested, executor).await?.ok_or_else(generic_denial)?;
        let requires_task_role = task && permission != &internal::WORK_ITEM_RECIPIENT;
        if rules::qualified(permission, &qualified) && (!requires_task_role || qualified.same_role(&required))
        {
            return Ok(qualified);
        }

        let task_required = if requires_task_role { required.as_slice() } else { &[] };
        let missing = rules::missing_public_permission(permission, task_required, &qualified);
        Err(missing.map_or_else(generic_denial, |permission| {
            Error::Forbidden(format!("当前账号缺少 {permission} 权限"))
        }))
    }

    // 解析根目标时在证明中保存精确任务语义，确保非事务重验仍使用精确目标规则而不是普通记录权限。
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

    // 需要解析固定附加来源规则时，沿用已取证账号、角色和原读取阶段，不以新资格替换根身份。
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
        let mut rules = self
            .target_condition(
                &context,
                qualified,
                model.map(|model| &model.output),
                task,
                &mut resolution,
                executor,
            )
            .await?;
        let has_role_scope = resolution.has_role_scope;
        rules.management = self.management_filter(&context, qualified, &mut resolution, executor).await?;

        resolution.versions.extend(self.grant_versions(&context.actor, &rules, executor).await?);

        let mut version = Fingerprint::new("authorization-proof-v3");
        version.unordered(&resolution.versions, |hash, value| hash.text(value));
        version.rule(&rules.root);
        version.optional(rules.whole.as_ref(), Fingerprint::rule);
        let management_rule = match &rules.management {
            Some(ResolvedFilter::Separate { rule, .. }) => Some(rule),
            _ => None,
        };
        version.optional(management_rule, Fingerprint::rule);
        version.boolean(task);
        version.optional(model, Fingerprint::model);

        let proof = AccessProof {
            context,
            qualified: qualified.clone(),
            resolved: ResolvedAccess {
                rules,
                replay: if task { Replay::ExactTask } else { Replay::Target },
                version: version.finish(),
            },
            validity: AccessValidity {
                boundary,
                service: self.token.clone(),
                valid_until: resolution.valid_until,
            },
            scope: ScopeMetadata {
                has_role_scope,
                policy_version: qualified.policy_revision(),
                organization_version: resolution.organization_version,
            },
        };
        proof.validity.ensure_current(executor)?;
        Ok(proof)
    }

    // 为工作项读取批次独立解析管理角色的范围，仍与根查询共享账号、有效期和读取阶段。
    async fn management_filter(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<Option<ResolvedFilter>> {
        if context.model.is_none() {
            return Ok(None);
        }
        if context.permission == work_item::MANAGE {
            return Ok(Some(ResolvedFilter::Root));
        }
        let Some(permission) = rules::management_permission(&context.permission) else {
            return Ok(None);
        };
        let context = AccessContext { permission, ..context.clone() };
        let rule = if qualified.same_role(&rules::same_role_permissions(&context.permission)) {
            let scope = self.resolve_scope(&context, &context.permission, qualified, executor).await?;
            resolution.include(&scope);
            rules::data_condition(&context.permission, Some(&scope.scope))?
        } else {
            AccessRule::Deny
        };
        Ok(Some(ResolvedFilter::Separate { context: Box::new(context), rule }))
    }

    pub(super) async fn grant_versions(
        &self,
        actor: &AuditActor,
        rules: &ResolvedRules,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut resources = BTreeSet::new();
        rules.root.grant_resources(&mut resources);
        if let Some(whole) = &rules.whole {
            whole.grant_resources(&mut resources);
        }

        let mut versions = Vec::new();
        for resource in resources {
            let revision = self.facilities.grant_revision(actor, resource, executor).await?;
            versions.push(format!("{}:{revision}", resource.code()));
        }
        Ok(versions)
    }

    // 精确任务目标保留入口基础范围并附加固定责任关系；普通对象继续使用该入口原数据规则。
    async fn target_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        output: Option<&QueryOutput>,
        task: bool,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedRules> {
        let kind = scope_spec::kind(&context.permission)?;
        let (base, whole) = match kind {
            ActionKind::Source => {
                let (root, whole) = self
                    .source_condition(
                        context,
                        qualified,
                        if task { None } else { output },
                        resolution,
                        executor,
                    )
                    .await?;
                (root, Some(whole))
            },
            ActionKind::Function if task => {
                let root = rules::task_condition(&context.permission, None)?;
                return Ok(ResolvedRules { root, whole: None, management: None });
            },
            _ => (self.condition(context, qualified, kind, resolution, executor).await?, None),
        };
        let root = if task {
            rules::task_condition(&context.permission, Some(base))?
        } else if context.permission == work_item::DETAIL && context.model.is_some() {
            self.work_item_query_condition(context, qualified, resolution, base, executor).await?
        } else {
            base
        };
        Ok(ResolvedRules { root, whole, management: None })
    }

    async fn condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        kind: ActionKind,
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

        match kind {
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
                    scopes.push((entry, source, scope.facts.scope));
                }
                let scopes = rules::ScopeLookup::Resolved(&scopes);

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
            ActionKind::Source => Err(Error::ValidationError("来源入口须使用固定来源分区条件".into())),
            ActionKind::Function => Err(Error::ValidationError("无对象操作不解析数据条件".into())),
        }
    }

    // 为商品目录解析真实供应商范围时，只使用独立供应商详情入口的合格角色，不能用连接菜单角色拼接数据范围。
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
    ) -> Result<(AccessRule, AccessRule)> {
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
            scopes.push((entry, source, scope.facts.scope));
        }

        let condition =
            |output| rules::sources::scoped_condition(&context.permission, qualified, &scopes, output);
        let whole = condition(rules::sources::Output::Object)?;
        let root = match output {
            None | Some(QueryOutput::Object) => whole.clone(),
            Some(QueryOutput::Share { .. } | QueryOutput::Existence { .. }) => {
                condition(rules::sources::Output::Share)?
            },
        };
        Ok((root, whole))
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

        self.valid_until = self.valid_until.into_iter().chain(scope_deadline(scope)).min();
    }
}

pub(super) fn scope_deadline(scope: &AuthorizedDataScope) -> Option<Instant> {
    scope
        .organizations
        .memberships
        .iter()
        .flat_map(|membership| [Some(membership.validity.valid_from), membership.validity.valid_to])
        .flatten()
        .filter(|until| *until > scope.as_of)
        .min()
}
