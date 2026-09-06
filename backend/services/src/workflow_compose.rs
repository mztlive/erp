//! Composition adapters that inject identity/audit/domain facts into workflow ports.
//!
//! Remaining unmigrated services construct workflow command services through this
//! module. HTTP AppState uses the same adapters. This crate must not depend on
//! `erp-processes`.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use erp_audit::{AuditExt, AuditLog, AuditLogData};
use erp_core::common::time::Instant;
use erp_core::AccountKind;
use erp_identity::access_control::{DataScope, DataScopeSubjectType, DataScopeType};
use erp_identity::{
    subject, AccessControlExt, MongoCasbinAdapter, Permission, PermissionSet, SharedRbacService,
};
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::ports::{
    DataScopeFact, DataScopeTypeFact, ObjectFact, ObjectFactKey, ObjectFactMap, ObjectFactPort,
    PreparedWorkflowAudit, RolePermissionSnapshotFact, SubjectBrief, W29CloseFact, WorkflowAccountFact,
    WorkflowAuditFact, WorkflowAuditPort, WorkflowAuthorizationPort,
};
use erp_workflow::{Error as WorkflowError, Result as WorkflowResult, WorkItemService};
use mongodb::{ClientSession, Database};
use persistence_core::Executor;

use crate::errors::{Error, Result};
use crate::work_item::ProcessObjectFacts;
use application_core::CommandFingerprint;
use application_core::CommandReceiptFact;
use entities::integration_ops::{
    ErrorClass, ErrorTaskStatus, ReconciliationDifferenceId, ReconciliationDifferenceResolution,
    ReconciliationDifferenceResolutionId, ResolutionType, W29CloseDecision,
};
use entities::purchase_order::PurchaseOrderStatus;
use erp_core::ids::PurchaseOrderId;

/// Shared RBAC adapter consumed by workflow command and definition services.
#[derive(Clone)]
pub struct WorkflowAuth {
    db: Database,
    rbac: SharedRbacService,
}

impl WorkflowAuth {
    /// Wrap a shared RBAC service as the workflow authorization port.
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    fn map_identity(error: erp_identity::Error) -> WorkflowError {
        map_service(Error::from(error))
    }

    fn map_persistence(error: persistence_core::Error) -> WorkflowError {
        WorkflowError::from(error)
    }

    fn account_fact(account: erp_identity::AccountCore) -> WorkflowAccountFact {
        account_fact(&account)
    }

    fn parse_permission(code: &str) -> WorkflowResult<Permission> {
        Permission::parse(code).map_err(|error| WorkflowError::ValidationError(error.to_string()))
    }

    fn scope_type_fact(scope_type: DataScopeType) -> DataScopeTypeFact {
        match scope_type {
            DataScopeType::Company => DataScopeTypeFact::Company,
            DataScopeType::Organization => DataScopeTypeFact::Organization,
            DataScopeType::Team => DataScopeTypeFact::Team,
            DataScopeType::SelfOwned => DataScopeTypeFact::SelfOwned,
            DataScopeType::Collaborative => DataScopeTypeFact::Collaborative,
        }
    }

    fn scope_facts(scopes: Vec<DataScope>) -> Vec<DataScopeFact> {
        scopes
            .into_iter()
            .map(|scope| {
                DataScopeFact::new(
                    scope.subject_id,
                    Self::scope_type_fact(scope.scope_type),
                    scope.scope_targets,
                )
            })
            .collect()
    }
}

impl WorkflowAuthorizationPort for WorkflowAuth {
    fn role_ids(
        &self,
        account_kind: AccountKind,
        account_id: &str,
    ) -> impl Future<Output = WorkflowResult<Vec<String>>> + Send {
        let rbac = self.rbac.clone();
        let account_id = account_id.to_string();
        async move {
            rbac.role_ids(account_kind, &account_id)
                .await
                .map_err(Self::map_identity)
        }
    }

    fn role_ids_with_executor(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<Vec<String>>> + Send {
        let db = self.db.clone();
        let account_id = account_id.to_string();
        async move {
            let subject = subject(account_kind, &account_id);
            let mut role_ids = MongoCasbinAdapter::new(db.clone())
                .subject_roles(&subject, executor)
                .await
                .map_err(Self::map_persistence)?
                .into_iter()
                .filter_map(|role_key| role_key.strip_prefix("role:").map(str::to_string))
                .collect::<Vec<_>>();
            role_ids.sort();
            role_ids.dedup();
            if role_ids.is_empty() {
                return Ok(role_ids);
            }
            let enabled = db
                .roles()
                .enabled_roles(&role_ids, executor)
                .await
                .map_err(Self::map_persistence)?
                .into_iter()
                .map(|role| role.base.id)
                .collect::<HashSet<_>>();
            role_ids.retain(|role_id| enabled.contains(role_id));
            Ok(role_ids)
        }
    }

    fn permission_codes(
        &self,
        account_kind: AccountKind,
        account_id: &str,
    ) -> impl Future<Output = WorkflowResult<Vec<String>>> + Send {
        let rbac = self.rbac.clone();
        let account_id = account_id.to_string();
        async move {
            Ok(rbac
                .permissions(account_kind, &account_id)
                .await
                .map_err(Self::map_identity)?
                .into_iter()
                .map(|permission| permission.to_string())
                .collect())
        }
    }

    fn enforce(
        &self,
        subject: &str,
        permission_code: &str,
    ) -> impl Future<Output = WorkflowResult<bool>> + Send {
        let rbac = self.rbac.clone();
        let subject = subject.to_string();
        let permission_code = permission_code.to_string();
        async move {
            let permission = Self::parse_permission(&permission_code)?;
            rbac.enforce(&subject, &permission)
                .await
                .map_err(Self::map_identity)
        }
    }

    fn permissions_cover(&self, owned: &[String], required: &[&str]) -> WorkflowResult<bool> {
        let owned = PermissionSet::new(
            owned
                .iter()
                .map(|code| Self::parse_permission(code))
                .collect::<WorkflowResult<Vec<_>>>()?,
        );
        let required = PermissionSet::new(
            required
                .iter()
                .map(|code| Self::parse_permission(code))
                .collect::<WorkflowResult<Vec<_>>>()?,
        );
        Ok(owned.covers(&required))
    }

    fn roles_granting_permission(
        &self,
        role_ids: &[String],
        permission_code: &str,
    ) -> impl Future<Output = WorkflowResult<Vec<String>>> + Send {
        let rbac = self.rbac.clone();
        let role_ids = role_ids.to_vec();
        let permission_code = permission_code.to_string();
        async move {
            let permission = Self::parse_permission(&permission_code)?;
            let mut granting = Vec::new();
            for role_id in role_ids {
                if rbac
                    .enforce(&format!("role:{role_id}"), &permission)
                    .await
                    .map_err(Self::map_identity)?
                {
                    granting.push(role_id);
                }
            }
            Ok(granting)
        }
    }

    fn role_permission_snapshot(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        required: &[&str],
    ) -> impl Future<Output = WorkflowResult<RolePermissionSnapshotFact>> + Send {
        let rbac = self.rbac.clone();
        let account_id = account_id.to_string();
        let required = required
            .iter()
            .map(|code| (*code).to_string())
            .collect::<Vec<_>>();
        async move {
            let permissions = required
                .iter()
                .map(|code| Self::parse_permission(code))
                .collect::<WorkflowResult<Vec<_>>>()?;
            let snapshot = rbac
                .role_permission_snapshot(account_kind, &account_id, &permissions)
                .await
                .map_err(Self::map_identity)?;
            let grants = snapshot
                .role_ids()
                .iter()
                .map(|role_id| {
                    let codes = snapshot
                        .granting_role_ids_for_all(&[])
                        .into_iter()
                        .filter(|_| false)
                        .collect::<Vec<_>>();
                    let _ = codes;
                    let granted = permissions
                        .iter()
                        .filter(|permission| {
                            snapshot
                                .granting_role_ids(permission)
                                .iter()
                                .any(|id| id == role_id)
                        })
                        .map(|permission| permission.to_string())
                        .collect::<Vec<_>>();
                    (role_id.clone(), granted)
                })
                .collect();
            Ok(RolePermissionSnapshotFact::new(
                snapshot.role_ids().to_vec(),
                grants,
                snapshot.policy_revision(),
            ))
        }
    }

    fn enabled_role_ids(
        &self,
        role_ids: &[String],
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<Vec<String>>> + Send {
        let db = self.db.clone();
        let role_ids = role_ids.to_vec();
        async move {
            if role_ids.is_empty() {
                return Ok(Vec::new());
            }
            Ok(db
                .roles()
                .enabled_roles(&role_ids, executor)
                .await
                .map_err(Self::map_persistence)?
                .into_iter()
                .map(|role| role.base.id)
                .collect())
        }
    }

    fn ensure_policy_snapshot_with_executor(
        &self,
        expected_revision: u64,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<()>> + Send {
        let rbac = self.rbac.clone();
        async move {
            rbac.ensure_policy_snapshot_with_executor(expected_revision, executor)
                .await
                .map_err(Self::map_identity)
        }
    }

    fn current_policy_revision(&self) -> impl Future<Output = WorkflowResult<u64>> + Send {
        let rbac = self.rbac.clone();
        async move { rbac.current_policy_revision().await.map_err(Self::map_identity) }
    }

    fn policy_revision_with_executor(
        &self,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<u64>> + Send {
        let rbac = self.rbac.clone();
        async move {
            rbac.policy_revision_with_executor(executor)
                .await
                .map_err(Self::map_identity)
        }
    }

    fn load_account(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<Option<WorkflowAccountFact>>> + Send {
        let db = self.db.clone();
        let account_id = account_id.to_string();
        async move {
            Ok(db
                .accounts()
                .find_account(&account_id, executor)
                .await
                .map_err(Self::map_persistence)?
                .map(Self::account_fact))
        }
    }

    fn load_accounts(
        &self,
        account_ids: &[String],
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<Vec<WorkflowAccountFact>>> + Send {
        let db = self.db.clone();
        let account_ids = account_ids.to_vec();
        async move {
            Ok(db
                .accounts()
                .list_by_ids(&account_ids, executor)
                .await
                .map_err(Self::map_persistence)?
                .into_iter()
                .map(Self::account_fact)
                .collect())
        }
    }

    fn list_accounts_by_kind(
        &self,
        account_kind: AccountKind,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<Vec<WorkflowAccountFact>>> + Send {
        let db = self.db.clone();
        async move {
            Ok(db
                .accounts()
                .list_by_kind(account_kind, executor)
                .await
                .map_err(Self::map_persistence)?
                .into_iter()
                .map(Self::account_fact)
                .collect())
        }
    }

    fn list_active_approval_candidates(
        &self,
        search: Option<&str>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<Vec<WorkflowAccountFact>>> + Send {
        let db = self.db.clone();
        let search = search.map(str::to_string);
        async move {
            Ok(db
                .accounts()
                .list_active_approval_candidates(search.as_deref(), limit, executor)
                .await
                .map_err(Self::map_persistence)?
                .into_iter()
                .map(Self::account_fact)
                .collect())
        }
    }

    fn load_data_scopes(
        &self,
        subject_type: &str,
        subject_id: &str,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<Vec<DataScopeFact>>> + Send {
        let db = self.db.clone();
        let subject_type = subject_type.to_string();
        let subject_id = subject_id.to_string();
        async move {
            let parsed = match subject_type.as_str() {
                "user" => DataScopeSubjectType::User,
                "role" => DataScopeSubjectType::Role,
                _ => {
                    return Err(WorkflowError::ValidationError(
                        "数据范围主体类型不支持".to_string(),
                    ))
                }
            };
            Ok(Self::scope_facts(
                db.data_scopes()
                    .list_by_subjects(parsed, &[subject_id], executor)
                    .await
                    .map_err(Self::map_persistence)?,
            ))
        }
    }

    fn load_data_scopes_for_subjects(
        &self,
        subject_type: &str,
        subject_ids: &[String],
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<Vec<DataScopeFact>>> + Send {
        let db = self.db.clone();
        let subject_type = subject_type.to_string();
        let subject_ids = subject_ids.to_vec();
        async move {
            let parsed = match subject_type.as_str() {
                "user" => DataScopeSubjectType::User,
                "role" => DataScopeSubjectType::Role,
                _ => {
                    return Err(WorkflowError::ValidationError(
                        "数据范围主体类型不支持".to_string(),
                    ))
                }
            };
            Ok(Self::scope_facts(
                db.data_scopes()
                    .list_by_subjects(parsed, &subject_ids, executor)
                    .await
                    .map_err(Self::map_persistence)?,
            ))
        }
    }

    fn organization_ids(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<Vec<String>>> + Send {
        let this = self.clone();
        let account_id = account_id.to_string();
        async move {
            let pairs = this
                .responsibility_scopes(account_kind, &account_id, executor)
                .await?;
            let mut ids = pairs
                .into_iter()
                .filter_map(|(_, organization_id)| organization_id)
                .collect::<Vec<_>>();
            ids.sort();
            ids.dedup();
            Ok(ids)
        }
    }

    fn responsibility_scopes(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = WorkflowResult<Vec<(String, Option<String>)>>> + Send {
        let this = self.clone();
        let account_id = account_id.to_string();
        async move {
            let role_ids = this
                .role_ids_with_executor(account_kind, &account_id, executor)
                .await?;
            let user_scopes = this.load_data_scopes("user", &account_id, executor).await?;
            let role_scopes = this
                .load_data_scopes_for_subjects("role", &role_ids, executor)
                .await?;
            let mut grouped: HashMap<String, Vec<DataScopeFact>> = HashMap::new();
            for scope in role_scopes {
                grouped.entry(scope.subject_id.clone()).or_default().push(scope);
            }
            let mut pairs = Vec::new();
            for role_id in role_ids {
                let role_scopes = grouped.get(&role_id).map(Vec::as_slice).unwrap_or_default();
                pairs.extend(responsibility_pairs(&role_id, role_scopes, &user_scopes));
            }
            pairs.sort();
            pairs.dedup();
            Ok(pairs)
        }
    }

    fn run_authorized_policy_transaction<T, E, F>(
        &self,
        policy_revision: u64,
        transaction: F,
    ) -> impl Future<Output = std::result::Result<T, E>> + Send
    where
        T: Send + 'static,
        E: From<WorkflowError>
            + From<persistence_core::Error>
            + From<application_core::Error>
            + std::error::Error
            + std::fmt::Display
            + Send
            + 'static,
        F: for<'a> FnOnce(
                &'a mut ClientSession,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'a>>
            + Send
            + 'static,
    {
        let rbac = self.rbac.clone();
        async move {
            rbac.run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    transaction(session)
                        .await
                        .map_err(|error| PolicyTxnError::<E>::Caller(error))
                })
            })
            .await
            .map_err(|error| match error {
                PolicyTxnError::Caller(error) => error,
                PolicyTxnError::Identity(error) => E::from(map_service(Error::from(error))),
                PolicyTxnError::Persistence(error) => E::from(error),
                PolicyTxnError::Application(error) => E::from(error),
            })
        }
    }
}

enum PolicyTxnError<E> {
    Caller(E),
    Identity(erp_identity::Error),
    Persistence(persistence_core::Error),
    Application(application_core::Error),
}

impl<E> From<erp_identity::Error> for PolicyTxnError<E> {
    fn from(error: erp_identity::Error) -> Self {
        Self::Identity(error)
    }
}

impl<E> From<persistence_core::Error> for PolicyTxnError<E> {
    fn from(error: persistence_core::Error) -> Self {
        Self::Persistence(error)
    }
}

impl<E> From<application_core::Error> for PolicyTxnError<E> {
    fn from(error: application_core::Error) -> Self {
        Self::Application(error)
    }
}

impl<E> std::fmt::Display for PolicyTxnError<E>
where
    E: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Caller(error) => write!(f, "{error}"),
            Self::Identity(error) => write!(f, "{error}"),
            Self::Persistence(error) => write!(f, "{error}"),
            Self::Application(error) => write!(f, "{error}"),
        }
    }
}

impl<E> std::fmt::Debug for PolicyTxnError<E>
where
    E: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Caller(error) => write!(f, "Caller({error:?})"),
            Self::Identity(error) => write!(f, "Identity({error:?})"),
            Self::Persistence(error) => write!(f, "Persistence({error:?})"),
            Self::Application(error) => write!(f, "Application({error:?})"),
        }
    }
}

impl<E> std::error::Error for PolicyTxnError<E> where E: std::error::Error {}

fn responsibility_pairs(
    role_id: &str,
    role_scopes: &[DataScopeFact],
    user_scopes: &[DataScopeFact],
) -> Vec<(String, Option<String>)> {
    let role_coverage = organization_coverage(role_scopes);
    let user_coverage = organization_coverage(user_scopes);
    let coverage = match (role_coverage, user_coverage) {
        (Some(role), Some(user)) => role.intersect(&user),
        (Some(role), None) => Some(role),
        (None, Some(user)) => Some(user),
        (None, None) => None,
    };
    let Some(coverage) = coverage else {
        return Vec::new();
    };
    coverage
        .targets()
        .into_iter()
        .map(|organization_id| (role_id.to_string(), organization_id))
        .collect()
}

fn organization_coverage(
    scopes: &[DataScopeFact],
) -> Option<erp_identity::access_control::OrganizationCoverage> {
    use erp_identity::access_control::OrganizationCoverage;
    if scopes
        .iter()
        .any(|scope| scope.scope_type == DataScopeTypeFact::Company)
    {
        return Some(OrganizationCoverage::All);
    }
    let targets = scopes
        .iter()
        .filter(|scope| {
            matches!(
                scope.scope_type,
                DataScopeTypeFact::Organization | DataScopeTypeFact::Team
            )
        })
        .flat_map(|scope| scope.scope_targets.iter().cloned())
        .collect::<Vec<_>>();
    OrganizationCoverage::from_targets(targets)
}

/// Persist workflow audits through `erp-audit`.
#[derive(Clone)]
pub struct WorkflowAudit {
    db: Database,
}

impl WorkflowAudit {
    /// Bind audit persistence to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl WorkflowAuditPort for WorkflowAudit {
    async fn persist(
        &self,
        audit: &PreparedWorkflowAudit,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        let actor_type = AccountKind::parse(&audit.actor_type)
            .map_err(|error| WorkflowError::ValidationError(error.to_string()))?;
        let log = AuditLog::new(
            audit.id.clone(),
            AuditLogData {
                actor_id: audit.actor_id.clone(),
                actor_account: audit.actor_account.clone(),
                actor_type,
                action: audit.action.clone(),
                resource_type: audit.resource_type.clone(),
                resource_id: Some(audit.resource_id.clone()),
                success: true,
                message: audit.message.clone(),
            },
        )
        .map_err(|error| map_service(Error::from(error)))?;
        self.db
            .audit_logs()
            .create(&log, executor)
            .await
            .map_err(WorkflowError::from)?;
        Ok(())
    }

    async fn find_command_receipts_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Vec<CommandReceiptFact>> {
        self.db
            .audit_logs()
            .find_command_receipts_by_ids(ids, executor)
            .await
            .map_err(|error| map_service(Error::from(error)))
    }

    async fn list_successful_resource_audits(
        &self,
        resource_type: &str,
        resource_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Vec<WorkflowAuditFact>> {
        Ok(self
            .db
            .audit_logs()
            .list_successful_by_resource(resource_type, resource_id, executor)
            .await
            .map_err(|error| map_service(Error::from(error)))?
            .into_iter()
            .map(|log| WorkflowAuditFact {
                actor_id: log.actor_id,
                action: log.action,
                resource_type: log.resource_type,
                resource_id: log.resource_id,
                success: log.success,
                message: log.message,
            })
            .collect())
    }
}

/// Domain-object facts consumed by work-item commands.
#[derive(Clone)]
pub struct WorkflowObjectFacts {
    inner: ProcessObjectFacts,
}

impl WorkflowObjectFacts {
    /// Bind remaining domain repositories used by work-item authorization.
    pub fn new(db: Database) -> Self {
        Self {
            inner: ProcessObjectFacts::new(db),
        }
    }

    fn db(&self) -> &Database {
        &self.inner.db
    }
}

#[async_trait]
impl ObjectFactPort for WorkflowObjectFacts {
    async fn load_object_facts(
        &self,
        keys: &HashSet<ObjectFactKey>,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<ObjectFactMap> {
        let loaded = self
            .inner
            .load_object_facts(keys, executor)
            .await
            .map_err(map_service)?;
        Ok(loaded
            .into_iter()
            .map(|(key, fact)| {
                (
                    key,
                    ObjectFact {
                        root_document_id: fact.root_document_id,
                        label: fact.label,
                        created_by: fact.created_by,
                        subject_versions: fact.subject_versions,
                        counterparty_label: fact.counterparty_label,
                        impact_summary: fact.impact_summary,
                        subject_briefs: fact
                            .subject_briefs
                            .into_iter()
                            .map(|(version, brief)| {
                                (
                                    version,
                                    SubjectBrief {
                                        counterparty_label: brief.counterparty_label,
                                        impact_summary: brief.impact_summary,
                                    },
                                )
                            })
                            .collect(),
                    },
                )
            })
            .collect())
    }

    async fn counterparty_is_active(
        &self,
        kind: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<bool> {
        self.inner
            .counterparty_is_active(kind, id, executor)
            .await
            .map_err(map_service)
    }

    async fn counterparty_numbers(
        &self,
        kind: &str,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> WorkflowResult<HashMap<String, String>> {
        self.inner
            .counterparty_numbers(kind, ids, executor)
            .await
            .map_err(map_service)
    }

    async fn external_identity_map_exists(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<bool> {
        use database::SourceRegistryExt;
        Ok(self
            .db()
            .external_identity_maps()
            .find_by_id(id, executor)
            .await
            .map_err(WorkflowError::from)?
            .is_some())
    }

    async fn assignment_separation_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Vec<String>> {
        self.inner
            .assignment_separation_actors(item, executor)
            .await
            .map_err(map_service)
    }

    async fn purchase_order_fulfillment_scope(
        &self,
        selected: &WorkItem,
        purchase_order_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<(String, Vec<WorkItem>)> {
        use database::PurchaseOrderExt;
        use erp_workflow::WorkItemExt;
        let responsibility_key = format!("purchase_order:{purchase_order_id}");
        let order = self
            .db()
            .purchase_orders()
            .find_by_id(&PurchaseOrderId::new(purchase_order_id.to_string()), executor)
            .await
            .map_err(WorkflowError::from)?
            .ok_or_else(|| WorkflowError::BusinessLogicError("履约任务引用的采购单不存在".to_string()))?;
        if matches!(
            order.stable.status,
            PurchaseOrderStatus::Completed | PurchaseOrderStatus::Voided
        ) {
            return Err(WorkflowError::BusinessLogicError(
                "已完成或已作废采购单不能变更责任人".to_string(),
            ));
        }
        let original_owner = order
            .current_owner_user_id()
            .map_err(|error| map_service(Error::from(error)))?
            .to_string();
        if selected.owner_user_id.as_deref() != Some(original_owner.as_str()) {
            return Err(WorkflowError::ConflictError(
                "采购单责任人与当前履约任务责任不一致，请刷新责任事实后重试".to_string(),
            ));
        }
        let tasks = self
            .db()
            .work_items()
            .list_open_fulfillment_by_responsibility_key(&responsibility_key, executor)
            .await
            .map_err(WorkflowError::from)?;
        if tasks.is_empty() || !tasks.iter().any(|task| task.base.id == selected.base.id) {
            return Err(WorkflowError::ConflictError(
                "采购单开放履约任务已变化，请刷新后重试".to_string(),
            ));
        }
        Ok((original_owner, tasks))
    }

    async fn reassign_purchase_order_owner(
        &self,
        purchase_order_id: &str,
        target_user_id: &str,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        use database::PurchaseOrderExt;
        let mut order = self
            .db()
            .purchase_orders()
            .find_by_id(&PurchaseOrderId::new(purchase_order_id.to_string()), executor)
            .await
            .map_err(WorkflowError::from)?
            .ok_or_else(|| WorkflowError::BusinessLogicError("履约任务引用的采购单不存在".to_string()))?;
        order
            .reassign_owner(target_user_id.to_string(), actor_id.to_string())
            .map_err(|error| map_service(Error::from(error)))?;
        self.db()
            .purchase_orders()
            .update(&mut order, executor)
            .await
            .map_err(WorkflowError::from)?;
        Ok(())
    }

    fn prepare_w29_close(
        &self,
        reason_code: &str,
        comment: Option<&str>,
        replacement_work_item_id: Option<&str>,
    ) -> WorkflowResult<W29CloseFact> {
        let decision = W29CloseDecision::new(reason_code, comment, replacement_work_item_id)
            .map_err(|error| WorkflowError::ValidationError(error.to_string()))?;
        Ok(W29CloseFact {
            close_reason: decision.close_reason().to_string(),
            replacement_work_item_id: decision.replacement_work_item_id().map(str::to_string),
        })
    }

    async fn persist_w29_close(
        &self,
        item: &WorkItem,
        decision: &W29CloseFact,
        evidence_reference: &str,
        actor_id: &str,
        receipt_id: &str,
        closed_at: Instant,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        use database::IntegrationOpsExt;
        use erp_workflow::WorkItemExt;
        if let Some(replacement_work_item_id) = decision.replacement_work_item_id.as_deref() {
            let replacement = self
                .db()
                .work_items()
                .find_work_item(replacement_work_item_id, executor)
                .await
                .map_err(WorkflowError::from)?
                .ok_or_else(|| WorkflowError::NotFound("替代任务不存在".to_string()))?;
            if !replacement.is_w29_replacement_for(item) {
                return Err(WorkflowError::ConflictError(
                    "替代任务必须在关闭事务中仍是同一 W29 对象类别的开放正式任务".to_string(),
                ));
            }
        }
        match item.business_object_type.as_str() {
            "integration_error_task" => {
                let mut task = self
                    .db()
                    .integration_error_tasks()
                    .find_work_item_integration_error_task(&item.business_object_id, executor)
                    .await
                    .map_err(WorkflowError::from)?
                    .ok_or_else(|| WorkflowError::NotFound("集成异常任务不存在".to_string()))?;
                let registered_type = if task.error_class == ErrorClass::ResultUnknown {
                    erp_workflow::WorkItemType::IntegrationResultUnknown
                } else {
                    erp_workflow::WorkItemType::BusinessException
                };
                if item.work_item_type != registered_type {
                    return Err(WorkflowError::ConflictError(
                        "任务类型与集成异常分类不一致，请刷新".to_string(),
                    ));
                }
                task.transition(
                    ErrorTaskStatus::Closed,
                    Some(ResolutionType::Close),
                    Some(evidence_reference.to_string()),
                    closed_at,
                )
                .map_err(|error| map_service(Error::from(error)))?;
                self.db()
                    .integration_error_tasks()
                    .update(&mut task, executor)
                    .await
                    .map_err(WorkflowError::from)?;
                Ok(())
            }
            "reconciliation_difference" => {
                let difference_id = ReconciliationDifferenceId::new(item.business_object_id.clone());
                self.db()
                    .reconciliation_differences()
                    .find_work_item_reconciliation_difference(&item.business_object_id, executor)
                    .await
                    .map_err(WorkflowError::from)?
                    .ok_or_else(|| WorkflowError::NotFound("对账差异不存在".to_string()))?;
                let latest = self
                    .db()
                    .reconciliation_difference_resolutions()
                    .find_latest_by_difference(&difference_id, executor)
                    .await
                    .map_err(WorkflowError::from)?;
                if latest
                    .as_ref()
                    .is_some_and(|resolution| resolution.resulting_status.is_terminal())
                {
                    return Err(WorkflowError::ConflictError(
                        "对账差异已经关闭或形成正式结论".to_string(),
                    ));
                }
                let resolution_no = W29CloseDecision::next_resolution_no(
                    latest.as_ref().map(|resolution| resolution.resolution_no),
                )
                .map_err(|error| map_service(Error::from(error)))?;
                let resolution_id_digest = CommandFingerprint::from_parts([receipt_id.to_string()]);
                let resolution = ReconciliationDifferenceResolution::new_close_evidence(
                    ReconciliationDifferenceResolutionId::new(format!(
                        "w29-close-{}",
                        resolution_id_digest.digest_hex()
                    )),
                    difference_id,
                    resolution_no,
                    if decision.replacement_work_item_id.is_some() {
                        entities::integration_ops::ResolutionAction::CloseDuplicate
                    } else {
                        entities::integration_ops::ResolutionAction::CloseMisrouted
                    },
                    entities::integration_ops::W29EvidenceReference::parse(evidence_reference)
                        .map_err(|error| map_service(Error::from(error)))?,
                    actor_id.to_string(),
                    closed_at,
                )
                .map_err(|error| map_service(Error::from(error)))?;
                self.db()
                    .reconciliation_difference_resolutions()
                    .create(&resolution, executor)
                    .await
                    .map_err(WorkflowError::from)?;
                Ok(())
            }
            _ => Err(WorkflowError::BusinessLogicError(
                "只有 W29 登记的异常对象允许受控关闭".to_string(),
            )),
        }
    }
}

fn map_service(error: Error) -> WorkflowError {
    match error {
        Error::Internal(message) => WorkflowError::Internal(message),
        Error::NotFound(message) => WorkflowError::NotFound(message),
        Error::ValidationError(message) => WorkflowError::ValidationError(message),
        Error::BusinessLogicError(message) => WorkflowError::BusinessLogicError(message),
        Error::ConflictError(message) => WorkflowError::ConflictError(message),
        Error::ReceiptDuplicate(error) => WorkflowError::ReceiptDuplicate(error),
        Error::TransientTransaction(error) => WorkflowError::TransientTransaction(error),
        Error::Forbidden(message) => WorkflowError::Forbidden(message),
        Error::Unauthenticated(message) => WorkflowError::Unauthenticated(message),
        Error::Logic(error) => WorkflowError::Logic(error),
        Error::Rbac(message) => WorkflowError::Rbac(message),
        Error::OutcomeUnknown(error) => WorkflowError::OutcomeUnknown(error),
        Error::RepositoryError(error) => WorkflowError::from(error),
        Error::Coded(code) => WorkflowError::Coded(map_workflow_code(code)),
    }
}

fn map_workflow_code(code: crate::errors::ErrorCode) -> erp_workflow::ErrorCode {
    match code {
        crate::errors::ErrorCode::ApprovalPolicyNotRegistered => {
            erp_workflow::ErrorCode::ApprovalPolicyNotRegistered
        }
        crate::errors::ErrorCode::ApprovalProcessNotConfigured => {
            erp_workflow::ErrorCode::ApprovalProcessNotConfigured
        }
        crate::errors::ErrorCode::ApprovalDraftSourceNotAvailable => {
            erp_workflow::ErrorCode::ApprovalDraftSourceNotAvailable
        }
        crate::errors::ErrorCode::ApprovalDefinitionNotDraft => {
            erp_workflow::ErrorCode::ApprovalDefinitionNotDraft
        }
        crate::errors::ErrorCode::ApprovalDefinitionVersionConflict => {
            erp_workflow::ErrorCode::ApprovalDefinitionVersionConflict
        }
        crate::errors::ErrorCode::ApprovalDefinitionInvalid => {
            erp_workflow::ErrorCode::ApprovalDefinitionInvalid
        }
        crate::errors::ErrorCode::ApprovalDefinitionBindingCorrupted => {
            erp_workflow::ErrorCode::ApprovalDefinitionBindingCorrupted
        }
        crate::errors::ErrorCode::ApprovalAlreadyStarted => erp_workflow::ErrorCode::ApprovalAlreadyStarted,
        crate::errors::ErrorCode::ApprovalTaskNotOpen => erp_workflow::ErrorCode::ApprovalTaskNotOpen,
        crate::errors::ErrorCode::ApprovalTaskNotAssignedToActor => {
            erp_workflow::ErrorCode::ApprovalTaskNotAssignedToActor
        }
        crate::errors::ErrorCode::ApprovalTaskVersionConflict => {
            erp_workflow::ErrorCode::ApprovalTaskVersionConflict
        }
        crate::errors::ErrorCode::ApprovalInstanceVersionConflict => {
            erp_workflow::ErrorCode::ApprovalInstanceVersionConflict
        }
        crate::errors::ErrorCode::ApprovalExecutionVersionConflict => {
            erp_workflow::ErrorCode::ApprovalExecutionVersionConflict
        }
        crate::errors::ErrorCode::ApprovalSubjectVersionConflict => {
            erp_workflow::ErrorCode::ApprovalSubjectVersionConflict
        }
        crate::errors::ErrorCode::ApprovalRejectReasonRequired => {
            erp_workflow::ErrorCode::ApprovalRejectReasonRequired
        }
        crate::errors::ErrorCode::ApprovalInstanceBlocked => erp_workflow::ErrorCode::ApprovalInstanceBlocked,
        crate::errors::ErrorCode::ApprovalResumeNotAllowedForBlocker => {
            erp_workflow::ErrorCode::ApprovalResumeNotAllowedForBlocker
        }
        crate::errors::ErrorCode::ApprovalCurrentApproverNotRecovered => {
            erp_workflow::ErrorCode::ApprovalCurrentApproverNotRecovered
        }
        crate::errors::ErrorCode::ApprovalBlockedCancelNotAllowed => {
            erp_workflow::ErrorCode::ApprovalBlockedCancelNotAllowed
        }
        crate::errors::ErrorCode::ApprovalGenericWorkItemMutationForbidden => {
            erp_workflow::ErrorCode::ApprovalGenericWorkItemMutationForbidden
        }
        crate::errors::ErrorCode::ApprovalIdempotencyPayloadConflict => {
            erp_workflow::ErrorCode::ApprovalIdempotencyPayloadConflict
        }
    }
}

/// Convert an identity account into the workflow account fact.
pub fn account_fact(account: &erp_identity::AccountCore) -> WorkflowAccountFact {
    WorkflowAccountFact::new(account.base.id.clone(), account.kind, account.can_login())
        .with_display_name(account.name.clone())
        .with_login_account(account.secret.account().to_string())
}

/// Construct a fully wired work-item command service for remaining callers.
pub fn work_item_service(db: Database, rbac: SharedRbacService) -> WorkItemService<WorkflowAuth> {
    let auth = WorkflowAuth::new(db.clone(), rbac);
    let facts = Arc::new(WorkflowObjectFacts::new(db.clone()));
    let audit = Arc::new(WorkflowAudit::new(db.clone()));
    WorkItemService::with_ports(db, auth, facts, audit)
}

/// Construct workflow authorization for composition roots.
pub fn workflow_auth(db: Database, rbac: SharedRbacService) -> WorkflowAuth {
    WorkflowAuth::new(db, rbac)
}

/// Construct the workflow audit adapter.
pub fn workflow_audit(db: Database) -> Arc<WorkflowAudit> {
    Arc::new(WorkflowAudit::new(db))
}

/// Construct the work-item object-fact adapter.
pub fn workflow_object_facts(db: Database) -> Arc<WorkflowObjectFacts> {
    Arc::new(WorkflowObjectFacts::new(db))
}

/// Bind a published approval definition through composition-root ports.
///
/// Remaining domain services must receive `object_read` from the composition
/// root. This helper does not open a nested transaction.
pub async fn bind_published_definition_on_document_create(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    command: &erp_workflow::service::approval::binding::BindPublishedDefinitionCommand,
    actor: &application_core::AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding>> {
    let auth = WorkflowAuth::new(db.clone(), rbac.clone());
    let audit = WorkflowAudit::new(db.clone());
    erp_workflow::service::approval::binding::bind_published_definition_on_document_create(
        db,
        &auth,
        object_read,
        &audit,
        command,
        actor,
        executor,
    )
    .await
    .map_err(map_service_err)
}

fn map_service_err(error: erp_workflow::Error) -> Error {
    Error::from(error)
}

/// Attach a computed binding onto a registered business document.
pub fn attach_published_binding(
    document: &mut erp_workflow::entity::document_registry::BusinessDocument,
    binding: erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    erp_workflow::service::approval::binding::attach_published_binding(document, binding).map_err(Error::from)
}
