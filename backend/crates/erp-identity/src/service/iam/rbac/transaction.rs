use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use application_core::owned_task::await_owned;
use persistence_core::{Executor, Transactional};

use super::RbacService;
use super::policy::commit_outcome_unknown;
use crate::error::{Error, Result};

impl RbacService {
    /// 在取消安全的所有权任务内运行系统初始化 policy 事务。
    ///
    /// 该入口没有操作人授权快照，只允许内建角色和超级管理员初始化使用。
    ///
    /// # 参数
    /// * `transaction` - 在事务执行器中执行的写入函数
    ///
    /// # 返回值
    /// 返回事务函数的结果。
    ///
    /// # 错误
    /// 当事务、policy 刷新或所有权任务失败时返回错误。
    pub async fn run_system_policy_transaction<T, E, F>(
        self: &Arc<Self>,
        transaction: F,
    ) -> std::result::Result<T, E>
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
        self.run_policy_transaction_at_revision(None, transaction).await
    }

    /// 使用授权快照对应的 policy 版本运行写事务。
    ///
    /// # 错误
    /// 授权后 policy 已变化、事务失败或本地刷新失败时返回错误。
    pub async fn run_authorized_policy_transaction<T, E, F>(
        self: &Arc<Self>,
        policy_revision: u64,
        transaction: F,
    ) -> std::result::Result<T, E>
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
        self.run_policy_transaction_at_revision(Some(policy_revision), transaction).await
    }

    /// 运行 policy 写事务，并按需绑定事务前授权使用的版本。
    pub(super) async fn run_policy_transaction_at_revision<T, E, F>(
        self: &Arc<Self>,
        expected_revision: Option<u64>,
        transaction: F,
    ) -> std::result::Result<T, E>
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
        let client = self.db.client().clone();
        self.ensure_policy_consistency_known()?;
        let policy_write = self.policy_write.clone().lock_owned().await;
        let rbac = self.clone();
        await_owned("RBAC", async move {
            let _policy_write = policy_write;
            rbac.ensure_policy_consistency_known()?;
            let policy_store = rbac.policy_store.clone();
            let result = client
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        let value = transaction(executor).await?;
                        match expected_revision {
                            Some(revision) => {
                                policy_store.bump_policy_revision_if_matches(revision, executor).await?;
                            },
                            None => policy_store.bump_policy_revision(executor).await?,
                        }
                        Ok::<_, E>(value)
                    })
                })
                .await;
            rbac.finish_policy_transaction(result).await
        })
        .await
    }

    /// 使用授权快照对应版本运行 policy 与审计原子事务。
    ///
    /// # 错误
    /// 业务写入、审计、policy 版本比较、提交或刷新失败时返回错误。
    pub async fn run_authorized_audited_policy_transaction<T, E, F>(
        self: &Arc<Self>,
        policy_revision: u64,
        audit: crate::ports::PreparedResourceAudit,
        transaction: F,
    ) -> std::result::Result<T, E>
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
        let audit_port = self.audit.clone();
        self.run_authorized_policy_transaction(policy_revision, move |executor| {
            Box::pin(async move {
                let value = transaction(executor).await?;
                audit_port.persist(&audit, executor).await?;
                Ok::<_, E>(value)
            })
        })
        .await
    }

    /// 完成 policy 事务，并在提交成功时刷新本地 Enforcer。
    ///
    /// 提交结果未知时进入不可自动恢复的一致性未知状态，避免单次旧快照 reload 误解锁。
    ///
    /// # 参数
    /// * `transaction` - 已完成的 MongoDB policy 事务结果
    /// # 返回值
    /// 成功时返回事务值；失败时返回原始事务错误。
    ///
    /// # 错误
    /// 事务提交失败时返回错误。提交成功后的刷新失败只记录错误并保持本地失败关闭，
    /// 避免把已提交写入误报为可安全重试的失败。
    pub(super) async fn finish_policy_transaction<T, E>(
        &self,
        transaction: std::result::Result<T, E>,
    ) -> std::result::Result<T, E>
    where
        E: std::error::Error + 'static,
    {
        match transaction {
            Ok(value) => {
                if let Err(error) = self.refresh_policy().await {
                    tracing::error!(
                        error = %error,
                        committed = true,
                        "RBAC policy committed but local Enforcer refresh failed"
                    );
                }
                Ok(value)
            },
            Err(error) if commit_outcome_unknown(&error) => {
                self.policy_consistency_unknown.store(true, Ordering::Release);
                self.policy_stale.store(true, Ordering::Release);
                tracing::error!(
                    error = %error,
                    restart_required = true,
                    "RBAC policy commit outcome is unknown; authorization has been stopped"
                );
                Err(error)
            },
            Err(error) => Err(error),
        }
    }

    /// 在提交结果未知后阻止授权读取和后续 policy 写入，避免旧快照被误判为最新状态。
    pub(super) fn ensure_policy_consistency_known(&self) -> Result<()> {
        if self.policy_consistency_unknown.load(Ordering::Acquire) {
            return Err(Error::Rbac("授权策略提交结果未知，当前进程已停止授权，请重启服务".to_string()));
        }
        Ok(())
    }
}
