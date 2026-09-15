//! 将身份域权限与 policy 事务装配到工作流授权端口。

use std::collections::{BTreeSet, HashSet};
use std::future::Future;
use std::pin::Pin;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_identity::{
    subject, AccessControlExt, MongoCasbinAdapter, Permission, PermissionSet, SharedRbacService,
};
use erp_read_models::sales_center::access::SalesAccess;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::{
    OrderTaskSource, RolePermissionSnapshotFact, WorkflowAccountFact, WorkflowAuthorizationPort,
    WorkflowDataScope, WorkflowScopeObject, WorkflowScopeObjects,
};
use erp_workflow::{Error as WorkflowError, Result as WorkflowResult};
use mongodb::{ClientSession, Database};
use persistence_core::Executor;

use super::order_access::{approval_readable, readable_sources};
use super::{account_fact, map_service};
use crate::adapters::purchase_access;
use crate::errors::Error;

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
}

impl WorkflowAuthorizationPort for WorkflowAuth {
    async fn resolve_workflow_scope(
        &self,
        actor: &AuditActor,
        permission: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Option<WorkflowDataScope>> {
        super::approval_scope::resolve(&self.db, &self.rbac, actor, permission, executor).await
    }

    async fn binding_order_readable(
        &self,
        actor: &AuditActor,
        object: &WorkflowScopeObject,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<bool> {
        super::order_access::binding_readable(&self.db, &self.rbac, actor, object, executor)
            .await
            .map_err(map_service)
    }

    async fn approval_scope_objects(
        &self,
        keys: &HashSet<(DocumentType, String)>,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<WorkflowScopeObjects> {
        super::approval_objects::load(&self.db, keys, executor)
            .await
            .map_err(map_service)
    }

    async fn approval_scope_object(
        &self,
        document_type: DocumentType,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<WorkflowScopeObject> {
        self.approval_scope_objects(
            &HashSet::from([(document_type, document_id.to_string())]),
            executor,
        )
        .await?
        .remove(&(document_type, document_id.to_string()))
        .ok_or_else(|| WorkflowError::NotFound("审批当前业务对象不存在".into()))
    }

    async fn managed_task_owners(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Option<Vec<String>>> {
        super::task_scope::managed_owners(&self.db, &self.rbac, actor, executor).await
    }

    async fn queue_scope_version(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<String> {
        DataScopeService::new(self.db.clone(), self.rbac.clone())
            .authorization_version(actor, executor)
            .await
            .map_err(Self::map_identity)
    }

    async fn order_approval_readable(
        &self,
        actor: &AuditActor,
        document_type: DocumentType,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<bool> {
        approval_readable(&self.db, &self.rbac, actor, document_type, document_id, executor).await
    }

    async fn readable_order_sources(
        &self,
        actor: &AuditActor,
        sources: &BTreeSet<OrderTaskSource>,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<BTreeSet<OrderTaskSource>> {
        readable_sources(&self.db, &self.rbac, actor, sources, executor).await
    }

    async fn require_order_task_read(
        &self,
        actor: &AuditActor,
        source: &OrderTaskSource,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        match source {
            OrderTaskSource::Sales(id) => {
                SalesAccess::new(self.db.clone(), self.rbac.clone())
                    .require_object(actor, "detail", id, &[], executor)
                    .await
                    .map_err(|error| map_service(Error::from(error)))?;
            }
            OrderTaskSource::Purchase(id) => {
                purchase_access(self.db.clone(), self.rbac.clone())
                    .require_object(actor, "detail", id, &[], executor)
                    .await
                    .map_err(|error| map_service(Error::from(error)))?;
            }
        }
        Ok(())
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_error_mapping_preserves_source_variants_and_repository_reclassification() {
        let repository = map_service(Error::RepositoryError(
            persistence_core::Error::OptimisticLockingError,
        ));
        assert!(
            matches!(repository, WorkflowError::ConflictError(message) if message == "数据已被其他请求修改，请刷新后重试")
        );
        assert!(matches!(
            map_service(Error::ReceiptDuplicate(
                persistence_core::Error::OptimisticLockingError
            )),
            WorkflowError::ReceiptDuplicate(persistence_core::Error::OptimisticLockingError)
        ));
        assert!(matches!(
            map_service(Error::TransientTransaction(
                persistence_core::Error::OptimisticLockingError
            )),
            WorkflowError::TransientTransaction(persistence_core::Error::OptimisticLockingError)
        ));
        assert!(matches!(
            map_service(Error::OutcomeUnknown(
                persistence_core::Error::OptimisticLockingError
            )),
            WorkflowError::OutcomeUnknown(persistence_core::Error::OptimisticLockingError)
        ));
        macro_rules! preserves_message {
            ($variant:ident) => {
                let mapped = map_service(Error::$variant("original".into()));
                assert!(matches!(mapped, WorkflowError::$variant(message) if message == "original"));
            };
        }
        preserves_message!(Internal);
        preserves_message!(NotFound);
        preserves_message!(ValidationError);
        preserves_message!(BusinessLogicError);
        preserves_message!(ConflictError);
        preserves_message!(Forbidden);
        preserves_message!(Unauthenticated);
        preserves_message!(Rbac);
        assert!(
            matches!(map_service(Error::Logic(erp_core::Error::from("original"))), WorkflowError::Logic(error) if error.to_string() == "original")
        );
        for code in erp_workflow::ErrorCode::ALL {
            let mapped = map_service(Error::Coded(code));
            assert_eq!(mapped.to_string(), code.to_string());
            assert_eq!(mapped.class(), code.class());
            assert!(mapped.code().is_some());
        }
    }

    #[test]
    fn policy_transaction_caller_error_is_not_wrapped_or_reclassified() {
        #[derive(Debug, PartialEq)]
        struct Caller {
            marker: u64,
        }
        impl std::fmt::Display for Caller {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "caller {}", self.marker)
            }
        }
        impl std::error::Error for Caller {}
        let wrapped = PolicyTxnError::Caller(Caller { marker: 173 });
        assert_eq!(wrapped.to_string(), "caller 173");
        assert_eq!(format!("{wrapped:?}"), "Caller(Caller { marker: 173 })");
        let PolicyTxnError::Caller(original) = wrapped else {
            panic!("caller error kind changed")
        };
        assert_eq!(original, Caller { marker: 173 });
    }
}
