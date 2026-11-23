use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use application_core::owned_task::await_owned;
use persistence_core::{Executor, Transactional};

use super::RbacService;
use super::policy::commit_outcome_unknown;
use crate::error::{Error, Result};

type TxFuture<'a, T, E> = Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'a>>;

impl RbacService {
    /// 仅在初始化内建角色或超级管理员时使用，无需操作人授权快照。
    /// 事务启动后由独立任务完成写入和收尾，调用方取消等待时仍可继续。
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
            + Send
            + 'static,
        F: for<'a> FnOnce(&'a mut dyn Executor) -> TxFuture<'a, T, E> + Send + 'static,
    {
        self.run_policy_transaction_at_revision(None, transaction).await
    }

    /// 完成操作人授权后，用当时冻结的策略版本运行写事务并原子推进版本。
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
            + Send
            + 'static,
        F: for<'a> FnOnce(&'a mut dyn Executor) -> TxFuture<'a, T, E> + Send + 'static,
    {
        self.run_policy_transaction_at_revision(Some(policy_revision), transaction).await
    }

    /// 用于写入成功即需推进策略版本的事务；指定期望版本时须匹配，系统初始化可省略比较。
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
            + Send
            + 'static,
        F: for<'a> FnOnce(&'a mut dyn Executor) -> TxFuture<'a, T, E> + Send + 'static,
    {
        self.run_optional_policy_transaction(expected_revision, move |executor| {
            Box::pin(async move { transaction(executor).await.map(|value| (value, true)) })
        })
        .await
    }

    /// 为可能重放或无授权变更的命令串行执行策略事务，独立任务持有写锁直至提交收尾。
    /// 回调返回结果及变更标记；仅有变更时才按期望版本推进策略修订号。重放可直接返回原回执且无需写入。
    pub(crate) async fn run_optional_policy_transaction<T, E, F>(
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
            + Send
            + 'static,
        F: for<'a> FnOnce(&'a mut dyn Executor) -> TxFuture<'a, (T, bool), E> + Send + 'static,
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
                        let (value, changed) = transaction(executor).await?;
                        if changed {
                            match expected_revision {
                                Some(revision) => {
                                    policy_store.bump_policy_revision_if_matches(revision, executor).await?;
                                },
                                None => policy_store.bump_policy_revision(executor).await?,
                            }
                        }
                        Ok::<_, E>(value)
                    })
                })
                .await;

            rbac.finish_policy_transaction(result).await
        })
        .await
    }

    /// 管理写入需要留痕时，将预先准备的审计、业务写入及授权版本推进放入同一事务。
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
            + Send
            + 'static,
        F: for<'a> FnOnce(&'a mut dyn Executor) -> TxFuture<'a, T, E> + Send + 'static,
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

    /// 策略事务结束后，提交成功时尝试刷新本地快照并返回已提交的事务值。
    /// 刷新失败只记日志，后续授权仍须取得新策略；提交结果未知时停止当前实例的授权读写。
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
                        "RBAC policy committed but local snapshot refresh failed"
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

    /// 授权读取及策略写入前检查提交状态；当前实例曾遇到未知提交结果时返回 `Rbac`，避免使用可能过期的快照。
    pub(super) fn ensure_policy_consistency_known(&self) -> Result<()> {
        if self.policy_consistency_unknown.load(Ordering::Acquire) {
            return Err(Error::Rbac("授权策略提交结果未知，当前进程已停止授权，请重启服务".to_string()));
        }
        Ok(())
    }
}
