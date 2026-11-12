use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use super::{AuthorizationService, rules};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::{AccessContext, AccessRule, QueryModel, QueryOutput};
use crate::entity::fingerprint::Fingerprint;
use crate::repository::authorization::AccessProof;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, QualifiedPermissions, Result};

#[derive(Default)]
struct Resolution {
    versions: Vec<String>,
    valid_until: Option<Instant>,
    organization_version: u64,
}

impl AuthorizationService {
    /// 身份先于业务事实读取；全部资格由一次账号及启用角色取证完成。
    pub(super) async fn qualify(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<QualifiedPermissions> {
        let required = rules::same_role_permissions(permission);
        let requested = if scope_spec::kind(permission)? == ActionKind::Source {
            rules::sources::qualifications(permission)?
        } else {
            required.clone()
        };
        self.permissions(actor, &requested, executor)
            .await?
            .filter(|qualified| qualified.same_role(&required))
            .ok_or_else(|| Error::Forbidden("账号或启用角色不具备本操作的完整权限".into()))
    }

    /// 固定权限产生共享数据条件，复验时沿用原解释时点，重新读取真实依赖。
    pub(super) async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        model: Option<&QueryModel>,
        as_of: Option<Instant>,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let boundary = executor
            .boundary()
            .ok_or_else(|| Error::ValidationError("授权要求可核验的事务或读取阶段".into()))?;
        let qualified = self.qualify(actor, permission, executor).await?;
        let context = AccessContext {
            actor: actor.clone(),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: model.map(|model| model.key.clone()),
            as_of: as_of.unwrap_or_else(Instant::now),
        };
        let mut resolution = Resolution { versions: vec![qualified.version().into()], ..Default::default() };
        let rule = self
            .condition(&context, &qualified, model.map(|model| &model.output), &mut resolution, executor)
            .await?;
        let mut resources = BTreeSet::new();
        rule.grant_resources(&mut resources);
        for resource in resources {
            let revision = self.facilities.grant_revision(actor, resource, executor).await?;
            resolution.versions.push(format!("{}:{revision}", resource.code()));
        }
        let mut version = Fingerprint::new("authorization-proof-v3");
        version.unordered(&resolution.versions, |hash, value| hash.text(value));
        version.rule(&rule);
        version.optional(model, Fingerprint::model);
        let proof = AccessProof {
            context,
            rule,
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

    async fn condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        output: Option<&QueryOutput>,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        match scope_spec::kind(&context.permission)? {
            ActionKind::Scope => {
                let scope = self.resolve_scope(context, &context.permission, qualified, executor).await?;
                resolution.include(&scope);
                rules::data_condition(&context.permission, Some(&scope.scope))
            },
            ActionKind::Task => rules::data_condition(&context.permission, None),
            ActionKind::Source => {
                self.source_condition(context, qualified, output, resolution, executor).await
            },
            ActionKind::Function => Err(Error::ValidationError("无对象操作不解析数据条件".into())),
        }
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
        rules::sources::scoped_condition(&context.permission, qualified, &scopes, output)
    }

    async fn resolve_scope(
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
    fn include(&mut self, scope: &AuthorizedDataScope) {
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
