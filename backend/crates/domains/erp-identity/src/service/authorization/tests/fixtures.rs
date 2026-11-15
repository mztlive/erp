use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_permissions::MODULES;
use persistence_core::Executor;

use super::super::facilities::Facilities;
use super::super::{AuthorizationBuilder, AuthorizationService, rules};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessContext, FactValue, ObjectFacts, ResourceTargets};
use crate::entity::organization::{OrgMembership, OrgValidity};
use crate::entity::organization_change::OrganizationState;
use crate::entity::resource_access::ResourceKind;
use crate::ports::AccessFactsPort;
pub(super) use crate::repository::authorization::testing::{inquiry_members, mapping};
use crate::service::access_control::resolve::{AuthorizedDataScope, DataScopeFacts};
use crate::{Error, Permission, QualifiedPermissions, Result};

pub(super) struct IdentityState {
    pub active: bool,
    pub grants: Vec<(String, Vec<Permission>)>,
    pub fail: bool,
    pub denied_scope: bool,
}
pub(super) struct Identity {
    pub state: Mutex<IdentityState>,
    pub revision: AtomicU64,
    pub permissions_calls: AtomicU64,
    pub grant_revision: AtomicU64,
    pub record_grants: Mutex<BTreeSet<(ResourceKind, String)>>,
    pub scopes: Mutex<BTreeMap<(Permission, Permission), ResolvedScope>>,
    pub role_scopes: Mutex<BTreeMap<(String, Permission), ScopeClause>>,
    pub scope_deadlines: Mutex<BTreeMap<Permission, Instant>>,
}
impl Default for Identity {
    fn default() -> Self {
        let mut permissions: Vec<Permission> = MODULES
            .iter()
            .flat_map(|module| module.entries)
            .map(|(_, entry)| entry.permission.clone())
            .collect();
        permissions.extend(rules::permissions().into_iter().cloned());
        Self {
            state: Mutex::new(IdentityState {
                active: true,
                grants: vec![("role".into(), permissions)],
                fail: false,
                denied_scope: false,
            }),
            revision: AtomicU64::new(1),
            permissions_calls: AtomicU64::new(0),
            grant_revision: AtomicU64::new(1),
            record_grants: Mutex::new(BTreeSet::new()),
            scopes: Mutex::new(BTreeMap::new()),
            role_scopes: Mutex::new(BTreeMap::new()),
            scope_deadlines: Mutex::new(BTreeMap::new()),
        }
    }
}
#[async_trait]
impl Facilities for Identity {
    async fn permissions(
        &self,
        _: &AuditActor,
        permissions: &[Permission],
        _: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        self.permissions_calls.fetch_add(1, Ordering::SeqCst);
        let state = self.state.lock().unwrap();
        if state.fail {
            return Err(Error::Internal("identity storage unavailable".into()));
        }
        if !state.active {
            return Ok(None);
        }
        let grants = state
            .grants
            .iter()
            .map(|(role, granted)| {
                (
                    role.clone(),
                    granted
                        .iter()
                        .filter(|permission| permissions.contains(permission))
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        let grants = grants
            .iter()
            .map(|(role, permissions)| (role.as_str(), permissions.as_slice()))
            .collect::<Vec<_>>();
        Ok(Some(QualifiedPermissions::test_grants(&grants)))
    }
    async fn scope(
        &self,
        context: &AccessContext,
        source: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        _: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let role_scopes = self.role_scopes.lock().unwrap();
        let scoped_roles =
            role_scopes.keys().any(|(_, permission)| permission == source).then(|| ResolvedScope {
                role_clauses: qualified
                    .roles_for_all(required)
                    .into_iter()
                    .filter_map(|role| role_scopes.get(&(role.to_string(), source.clone())).cloned())
                    .collect(),
                user_limit: None,
            });
        let mut organizations = OrganizationState::default();
        if let Some(deadline) = self.scope_deadlines.lock().unwrap().get(&context.permission) {
            organizations.memberships.push(OrgMembership {
                base: BaseModel::fake(),
                user_id: context.actor.id().into(),
                org_unit_id: "test".into(),
                validity: OrgValidity { valid_from: Instant::from_unix_secs(1), valid_to: Some(*deadline) },
                changed_by: "admin".into(),
                reason: "test".into(),
            });
        }
        Ok(AuthorizedDataScope::test_fixture(DataScopeFacts {
            resource: context.permission.resource().into(),
            action: context.permission.action().into(),
            scope: scoped_roles.unwrap_or_else(|| {
                self.scopes
                    .lock()
                    .unwrap()
                    .get(&(context.permission.clone(), source.clone()))
                    .cloned()
                    .unwrap_or_else(|| {
                        if self.state.lock().unwrap().denied_scope {
                            ResolvedScope { role_clauses: vec![], user_limit: None }
                        } else {
                            ResolvedScope {
                                role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                                user_limit: None,
                            }
                        }
                    })
            }),
            organizations,
            policy_version: 1,
            scope_version: self.revision.load(Ordering::SeqCst).to_string(),
            as_of: context.as_of,
        }))
    }
    async fn grant_revision(&self, _: &AuditActor, _: ResourceKind, _: &mut dyn Executor) -> Result<u64> {
        Ok(self.grant_revision.load(Ordering::SeqCst))
    }
    async fn grants(
        &self,
        _: &AccessContext,
        resource: ResourceKind,
        ids: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Ok(self
            .record_grants
            .lock()
            .unwrap()
            .iter()
            .filter(|(kind, id)| *kind == resource && ids.ids().contains(id))
            .map(|(_, id)| id.clone())
            .collect())
    }
}

#[derive(Default)]
pub(super) struct Facts {
    pub objects: Mutex<Vec<ObjectFacts>>,
    pub calls: AtomicU64,
}
impl Facts {
    pub fn new(objects: Vec<ObjectFacts>) -> Self {
        Self { objects: Mutex::new(objects), calls: AtomicU64::new(0) }
    }
}
#[async_trait]
impl AccessFactsPort for Facts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .objects
            .lock()
            .unwrap()
            .iter()
            .filter(|object| ids.ids().contains(&object.id))
            .cloned()
            .collect())
    }
}

pub(super) fn empty() -> AuthorizationBuilder {
    AuthorizationBuilder {
        facilities: Arc::new(Identity::default()),
        facts: BTreeMap::new(),
        mappings: BTreeMap::new(),
    }
}
pub(super) fn complete() -> AuthorizationBuilder {
    let mut builder = empty();
    for permission in rules::permissions() {
        if !matches!(scope_spec::kind(permission).unwrap(), ActionKind::Function) {
            builder.facts.entry(permission.resource().into()).or_insert_with(|| Arc::new(Facts::default()));
        }
        if rules::query_supported(permission).unwrap() {
            builder.mappings.insert(permission.clone(), mapping(permission));
        }
    }
    builder
}
pub(super) fn service(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("sales_inquiry".into(), facts);
    builder.build().unwrap()
}
pub(super) fn actor() -> AuditActor {
    AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
}
pub(super) fn candidate(resource: &str, owner: &str) -> ObjectFacts {
    ObjectFacts {
        resource: resource.into(),
        owner_id: FactValue::Known(Some(owner.into())),
        org_unit_id: FactValue::Known(None),
        ..Default::default()
    }
}
pub(super) fn object(resource: &str, id: &str, owner: &str) -> ObjectFacts {
    ObjectFacts {
        id: id.into(),
        version: "v1".into(),
        history_participants: FactValue::Known(BTreeSet::new()),
        ..candidate(resource, owner)
    }
}
