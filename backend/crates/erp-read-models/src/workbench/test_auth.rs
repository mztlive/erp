//! 工作台库单元测试授权替身；只运行生产编排，不发起数据库或 HTTP 请求。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_workflow::ports::{RolePermissionSnapshotFact, WorkflowAccountFact, WorkflowQueueAccessFact};
use erp_workflow::{Error, Result, WorkflowAuthorizationPort};
use persistence_core::Executor;

/// 共享观测轨迹与可变 policy revision，验证只读阶段缓存的命中和重建。
#[derive(Clone, Default)]
pub(super) struct TestAuth {
    pub(super) trace: Arc<Mutex<Vec<String>>>,
    pub(super) revision: Arc<AtomicU64>,
    pub(super) fail_revision: Arc<AtomicBool>,
    pub(super) queue: Arc<Mutex<Option<WorkflowQueueAccessFact>>>,
    pub(super) fail_queue: Arc<AtomicBool>,
    pub(super) portal_readable: Arc<Mutex<HashMap<String, bool>>>,
    pub(super) portal_reviewable: Arc<Mutex<HashMap<String, bool>>>,
    pub(super) fail_portal_reviewable: Arc<AtomicBool>,
}

#[allow(clippy::manual_async_fn)]
impl WorkflowAuthorizationPort for TestAuth {
    /// 记录精确申请范围读取，并返回当前可配置资格。
    async fn supplier_portal_request_readable(
        &self,
        actor: &AuditActor,
        request_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.trace.lock().unwrap().push(format!("portal:{}:{request_id}", actor.id()));
        Ok(self.portal_readable.lock().unwrap().get(request_id).copied().unwrap_or(false))
    }
    /// 记录当前申请业务动作资格，明确区分越界与基础设施失败。
    async fn supplier_portal_request_reviewable(
        &self,
        actor: &AuditActor,
        request_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.trace.lock().unwrap().push(format!("portal-review:{}:{request_id}", actor.id()));
        if self.fail_portal_reviewable.load(Ordering::SeqCst) {
            return Err(Error::Rbac("portal business authorization failed".into()));
        }
        Ok(self.portal_reviewable.lock().unwrap().get(request_id).copied().unwrap_or(false))
    }
    /// 返回可配置的队列身份事实，并验证原调用人的身份透传。
    async fn queue_access_facts(
        &self,
        actor: &AuditActor,
        include_version: bool,
        _executor: &mut dyn Executor,
    ) -> Result<Option<WorkflowQueueAccessFact>> {
        self.trace.lock().unwrap().push(format!("queue:{}:{include_version}", actor.id()));
        if self.fail_queue.load(Ordering::SeqCst) {
            return Err(Error::Rbac("queue failed".into()));
        }
        Ok(self.queue.lock().unwrap().clone())
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn role_ids(
        &self,
        _account_kind: AccountKind,
        _account_id: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn role_ids_with_executor(
        &self,
        _account_kind: AccountKind,
        _account_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn permission_codes(
        &self,
        _account_kind: AccountKind,
        _account_id: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn enforce(&self, _subject: &str, _permission_code: &str) -> impl Future<Output = Result<bool>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn permissions_cover(&self, _owned: &[String], _required: &[&str]) -> Result<bool> {
        Err(Error::Internal("unexpected authorization call".into()))
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn roles_granting_permission(
        &self,
        _role_ids: &[String],
        _permission_code: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 返回替身配置的事实并记录实际生产调用。
    async fn role_permission_snapshot(
        &self,
        _kind: AccountKind,
        account: &str,
        required: &[&str],
    ) -> Result<RolePermissionSnapshotFact> {
        self.trace.lock().unwrap().push(format!("snapshot:{account}"));
        Ok(RolePermissionSnapshotFact::new(
            vec!["executor".into()],
            HashMap::from([("executor".into(), required.iter().map(|code| (*code).to_string()).collect())]),
            self.revision.load(Ordering::SeqCst),
        ))
    }

    /// 返回替身配置的事实并记录实际生产调用。
    async fn enabled_role_ids(&self, roles: &[String], _executor: &mut dyn Executor) -> Result<Vec<String>> {
        self.trace.lock().unwrap().push("enabled_roles".into());
        Ok(roles.to_vec())
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn ensure_policy_snapshot_with_executor(
        &self,
        _expected_revision: u64,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 返回替身配置的事实并记录实际生产调用。
    async fn current_policy_revision(&self) -> Result<u64> {
        self.trace.lock().unwrap().push("revision".into());
        if self.fail_revision.load(Ordering::SeqCst) {
            return Err(Error::Rbac("revision failed".into()));
        }
        Ok(self.revision.load(Ordering::SeqCst))
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn policy_revision_with_executor(
        &self,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<u64>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn load_account(
        &self,
        _account_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Option<WorkflowAccountFact>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn load_accounts(
        &self,
        _account_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn list_accounts_by_kind(
        &self,
        _account_kind: AccountKind,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn list_active_approval_candidates(
        &self,
        _search: Option<&str>,
        _limit: u32,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    fn run_authorized_policy_transaction<T, E, F>(
        &self,
        _policy_revision: u64,
        _transaction: F,
    ) -> impl Future<Output = std::result::Result<T, E>> + Send
    where
        T: Send + 'static,
        E: From<Error>
            + From<persistence_core::Error>
            + From<application_core::Error>
            + std::error::Error
            + std::fmt::Display
            + Send
            + 'static,
        F: for<'a> FnOnce(
                &'a mut dyn Executor,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'a>>
            + Send
            + 'static,
    {
        async move { Err(E::from(Error::Internal("授权端口未接线".to_string()))) }
    }
}
