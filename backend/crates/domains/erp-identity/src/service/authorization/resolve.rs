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
    WholeRule,
};
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, QualifiedPermissions, Result};

/// 累计本次规则实际使用的范围依赖，供授权证明生成版本和有效期。
#[derive(Default)]
pub(super) struct Resolution {
    /// 参与授权版本摘要的身份、人员范围及记录授权版本片段。
    pub(super) versions: Vec<String>,
    /// 已纳入的组织范围中最早发生变化的时点。
    pub(super) valid_until: Option<Instant>,
    /// 已纳入范围中的最大组织修订号，独立于策略修订号。
    pub(super) organization_version: u64,
    /// 本次纳入的范围是否包含至少一项角色范围。
    pub(super) has_role_scope: bool,
}

impl AuthorizationService {
    /// 为权限检查在检查人的原读取事务中验证目标账号的完整功能资格；范围动作返回合格角色的固定范围，其他动作返回 `None`，仍需对象或任务上下文。
    ///
    /// 账号失效、资格不足、未知动作及取证错误原样传播。
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

    /// 普通对象或管理操作取证前，一次取得该账号可用启用角色的功能资格，供各固定分支共用。
    pub(super) async fn qualify(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<QualifiedPermissions> {
        self.qualify_target(actor, permission, false, executor).await
    }

    // 在读取对象前检查固定入口及精确任务的同角色组合；任务接收者另按真实任务取证，缺权时仅报告可唯一识别的公开权限。
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

    /// 为根目标或查询取得资格、固定模型和授权时点并签发证明；保留精确任务语义，复验时不改用普通记录规则。
    pub(super) async fn resolve_target(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        model: Option<&QueryModel>,
        as_of: Option<Instant>,
        task: bool,
        executor: &mut dyn Executor,
    ) -> Result<(AccessProof, QualifiedPermissions)> {
        let qualified = self.qualify_target(actor, permission, task, executor).await?;
        let context = AccessContext {
            actor: actor.clone(),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: model.map(|model| model.key.clone()),
            as_of: as_of.unwrap_or_else(Instant::now),
        };
        let proof = self.resolve_context(context, model, task, &qualified, executor).await?;
        Ok((proof, qualified))
    }

    /// 用已取证的身份在原阶段解析根目标或固定附加来源，收集规则、版本和期限，不以新资格替换根身份。
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
        version.optional(rules.whole_rule(), Fingerprint::rule);
        let management_rule = match &rules.management {
            Some(ResolvedFilter::Separate { rule, .. }) => Some(rule),
            _ => None,
        };
        version.optional(management_rule, Fingerprint::rule);
        version.boolean(task);
        version.optional(model, Fingerprint::model);

        let proof = AccessProof {
            context,
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
            scope: ScopeMetadata { has_role_scope, organization_version: resolution.organization_version },
        };
        proof.validity.ensure_current(executor)?;
        Ok(proof)
    }

    // 为工作项查询单独确定管理展示过滤；管理入口复用根规则，其他支持入口按管理角色范围计算，无资格则拒绝该分支。
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

    /// 为根规则及独立整账规则实际引用的资源读取记录授权修订号，用于凭证版本和交付复验。
    pub(super) async fn grant_versions(
        &self,
        actor: &AuditActor,
        rules: &ResolvedRules,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut resources = BTreeSet::new();
        rules.root.grant_resources(&mut resources);
        if let Some(WholeRule::Separate(whole)) = &rules.whole {
            whole.grant_resources(&mut resources);
        }

        let mut versions = Vec::new();
        for resource in resources {
            let revision = self.facilities.grant_revision(actor, resource, executor).await?;
            versions.push(format!("{}:{revision}", resource.code()));
        }
        Ok(versions)
    }

    // 按动作、对象或查询输出选择根和整账条件；精确任务按固定方式附加责任要求，不套用普通记录资格。
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
        let base = match kind {
            ActionKind::Source => {
                let (root, whole) =
                    self.source_condition(context, qualified, output, task, resolution, executor).await?;
                return Ok(ResolvedRules { root, whole: Some(whole), management: None });
            },
            ActionKind::Function if task => {
                let root = rules::task_condition(&context.permission, None)?;
                return Ok(ResolvedRules { root, whole: None, management: None });
            },
            _ => self.condition(context, qualified, kind, resolution, executor).await?,
        };
        let root = if task {
            rules::task_condition(&context.permission, Some(base))?
        } else if context.permission == work_item::DETAIL && context.model.is_some() {
            self.work_item_query_condition(context, qualified, resolution, base, executor).await?
        } else {
            base
        };
        Ok(ResolvedRules { root, whole: None, management: None })
    }

    // 解析普通对象的条件；为支持的固定入口取实际范围，只计算当前身份合资格的选型或客户分支，并汇入依赖。
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
                    scopes.push((entry, source, scope.scope));
                }
                let scopes = rules::ScopeLookup::Resolved(&scopes);

                rules::scoped_extensions(&context.permission, Some(qualified), &scopes, base)
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

    // 读取连接商品目录时，只用独立供应商详情入口的合格角色解析真实供应商范围，不借用连接菜单角色。
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

    // 按入口与来源分别解析合格角色范围，生成根及整账条件。完整对象可复用根规则，份额、存在性和精确任务另保留完整对象条件。
    async fn source_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        output: Option<&QueryOutput>,
        task: bool,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<(AccessRule, WholeRule)> {
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
            scopes.push((entry, source, scope.scope));
        }

        let condition =
            |output| rules::sources::scoped_condition(&context.permission, qualified, &scopes, output);
        let whole = condition(rules::sources::Output::Object)?;
        if task {
            let root = rules::task_condition(&context.permission, Some(whole.clone()))?;
            return Ok((root, WholeRule::Separate(whole)));
        }
        match output {
            None | Some(QueryOutput::Object) => Ok((whole, WholeRule::Root)),
            Some(QueryOutput::Share { .. } | QueryOutput::Existence { .. }) => {
                Ok((condition(rules::sources::Output::Share)?, WholeRule::Separate(whole)))
            },
        }
    }

    /// 将范围权限归到实际配置动作，再用当前入口的完整同角色资格读取同次范围。
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
    /// 每解析一个范围即纳入其版本、组织修订、角色范围标记及最早失效时间。
    pub(super) fn include(&mut self, scope: &AuthorizedDataScope) {
        self.has_role_scope |= scope.scope.has_role_scope();
        self.versions.push(scope.scope_version.clone());
        self.organization_version = self.organization_version.max(scope.organizations.version);

        self.valid_until = self.valid_until.into_iter().chain(scope_deadline(scope)).min();
    }
}

/// 从组织成员关系的生效与到期时间中选出授权时点之后最早的变化，用作范围有效期上界。
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
