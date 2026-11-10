use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use futures_util::future::BoxFuture;
use persistence_core::Executor;

use super::{AccessMode, AccessPolicy, AccessRegistration, AuthorizationService};
use crate::entity::access_control::authorization_policy::AuthorizationPolicy;
use crate::entity::authorization::{AccessContext, AccessFact, AccessRule, AccessSubject, ReadAccess};
use crate::entity::fingerprint::Fingerprint;
use crate::repository::authorization::AccessProof;
use crate::repository::resource_grants;
use crate::service::access_control::resolve::DataScopeService;
use crate::{Error, Permission, Result};

struct Resolution {
    versions: Vec<String>,
    valid_until: Option<Instant>,
}

impl AuthorizationService {
    /// 冻结身份与登记模型，范围节点共享解释时点。
    pub(super) async fn resolve(
        &self,
        actor: &AuditActor,
        entry: Arc<AccessRegistration>,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let data = entry.access.data()?;
        let version = self.identity(actor, &entry, executor).await?;
        let model = match (&data.subject, &entry.access) {
            (AccessSubject::View(key), _) => Some(key.clone()),
            (_, AccessMode::Query { mapping, .. } | AccessMode::QueryAndObject { mapping, .. }) => {
                Some(mapping.model().key.clone())
            },
            _ => None,
        };
        let context = AccessContext {
            actor: actor.clone(),
            permission: entry.permission.clone(),
            resource: entry.permission.resource().into(),
            model: model.clone(),
            as_of: Instant::now(),
        };
        let mut resolution = Resolution { versions: vec![version], valid_until: None };
        let rule = self
            .policy(&data.policy, &entry, &context, &mut resolution, executor)
            .await?
            .requiring(&data.required_facts);
        // v2 字段顺序：无序版本依赖、主体、规则、可选模型标识。
        let mut version = Fingerprint::new("authorization-proof-v2");
        version.unordered(&resolution.versions, |hash, value| hash.text(value));
        version.subject(&data.subject);
        version.rule(&rule);
        version.optional(model.as_ref(), |hash, key| hash.text(key.as_str()));
        Ok(AccessProof {
            context,
            subject: data.subject.clone(),
            rule,
            version: version.finish(),
            valid_until: resolution.valid_until,
            session: executor.session().map(|s| s.id().clone()),
            registry: self.token.clone(),
        })
    }

    /// 无对象资格不装载组织树，多个权限必须由同一启用角色证明。
    pub(super) async fn identity(
        &self,
        actor: &AuditActor,
        entry: &AccessRegistration,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let mut permissions = entry.required_permissions.clone();
        permissions.push(entry.permission.clone());
        self.qualification(actor, &permissions, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("账号或有效角色不具备本操作的完整权限".into()))
    }

    /// 功能资格与对象入口共享账号、启用角色和同角色完整权限证明。
    pub(super) async fn qualification(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Ok(self
            .rbac
            .qualified_permissions(actor, permissions, executor)
            .await?
            .filter(|qualified| qualified.same_role(permissions))
            .map(|qualified| qualified.version().to_owned()))
    }

    /// 按登记树解析，不接受调用方传入的授权表达式。
    fn policy<'a>(
        &'a self,
        policy: &'a AccessPolicy,
        entry: &'a AccessRegistration,
        context: &'a AccessContext,
        resolution: &'a mut Resolution,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<AccessRule>> {
        Box::pin(async move {
            match policy {
                AccessPolicy::Assigned => Ok(AccessRule::Fact(AccessFact::AssignedTo)),
                AccessPolicy::Participant => Ok(AccessRule::Fact(AccessFact::Participant)),
                AccessPolicy::Scope { permission, read } => {
                    self.scope(permission, *read, entry, context, resolution, executor).await
                },
                AccessPolicy::All(policies) | AccessPolicy::Any(policies) => {
                    let mut rules = Vec::new();
                    for policy in policies {
                        rules.push(self.policy(policy, entry, context, resolution, executor).await?);
                    }
                    Ok(if matches!(policy, AccessPolicy::All(_)) {
                        AccessRule::all(rules)
                    } else {
                        AccessRule::any(rules)
                    })
                },
                AccessPolicy::Related { relation, resource, mode, required_facts, policy } => {
                    let context = AccessContext { resource: resource.clone(), ..context.clone() };
                    let rule = self
                        .policy(policy, entry, &context, resolution, executor)
                        .await?
                        .requiring(required_facts);
                    Ok(AccessRule::Related {
                        relation: relation.clone(),
                        resource: resource.clone(),
                        mode: *mode,
                        rule: Box::new(rule),
                    })
                },
            }
        })
    }

    /// 范围配置与对象资源分别取证；读取授权只加入获准读取的资源。
    async fn scope(
        &self,
        permission: &Permission,
        read: ReadAccess,
        entry: &AccessRegistration,
        context: &AccessContext,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        if permission.resource() != context.resource {
            return Err(Error::ValidationError("范围政策与对象资源不匹配".into()));
        }
        let source = AuthorizationPolicy::scope_source(permission)?;
        let scope = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .registered(
                &context.actor,
                &entry.permission,
                &source,
                &entry.required_permissions,
                context.as_of,
                executor,
            )
            .await?;
        resolution.versions.push(scope.scope_version.clone());
        for member in &scope.organizations.memberships {
            for boundary in [Some(member.validity.valid_from), member.validity.valid_to].into_iter().flatten()
            {
                if boundary > context.as_of {
                    resolution.valid_until =
                        Some(resolution.valid_until.map_or(boundary, |old| old.min(boundary)));
                }
            }
        }
        let base = read.base(&scope.scope);
        let resource = read.resource(permission)?;
        match resource {
            Some(resource) if base != AccessRule::Allow => {
                resource.scope_action(&entry.permission)?;
                let revision =
                    resource_grants::revision(&self.db, context.actor.id(), resource, executor).await?;
                // v2 字段顺序：固定资源代码、授权修订号。
                let mut version = Fingerprint::new("resource-grant-v2");
                version.text(resource.code());
                version.number(revision);
                resolution.versions.push(version.finish());
                Ok(AccessRule::any(vec![base, AccessRule::RecordReadGrant(resource)]))
            },
            _ => Ok(base),
        }
    }
}
