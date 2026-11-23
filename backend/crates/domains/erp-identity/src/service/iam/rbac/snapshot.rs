use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

use erp_core::AccountKind;
use persistence_core::{Executor, NoTransaction};
use tokio::sync::RwLock;

use super::policy::{ensure_policy_snapshot_revision, role_ids_for_account, role_key};
use super::{MAX_STABLE_POLICY_LOAD_ATTEMPTS, RbacService, RolePermissionSnapshot};
use crate::entity::Permission;
use crate::error::{Error, Result};
use crate::repository::policy_store::PolicySnapshot;

impl RbacService {
    // 首次访问时加载稳定策略快照；加载失败保留未初始化状态供下次重试。
    async fn snapshot(&self) -> Result<&RwLock<PolicySnapshot>> {
        self.snapshot
            .get_or_try_init(|| async {
                let mut snapshot = PolicySnapshot::default();
                self.reload_snapshot(&mut snapshot).await?;
                Ok(RwLock::new(snapshot))
            })
            .await
    }

    /// 鉴权或查询策略前取得与数据库策略版本对齐的快照锁；缓存过期时在写锁内复查并重载。
    /// 提交结果未知时停止授权；策略持续变化或重载失败时直接返回错误，旧缓存保持不可用。
    pub(super) async fn fresh_snapshot(&self) -> Result<&RwLock<PolicySnapshot>> {
        self.ensure_policy_consistency_known()?;
        let snapshot = self.snapshot().await?;
        if self.policy_cache_is_current().await? {
            self.ensure_policy_consistency_known()?;
            return Ok(snapshot);
        }

        let mut guard = snapshot.write().await;
        if !self.policy_cache_is_current().await? {
            self.reload_snapshot(&mut guard).await?;
        }
        drop(guard);
        self.ensure_policy_consistency_known()?;
        Ok(snapshot)
    }

    // 取用缓存和获取写锁后都检查过期标记及数据库版本，避免使用旧策略或重复刷新。
    async fn policy_cache_is_current(&self) -> Result<bool> {
        if self.policy_stale.load(Ordering::Acquire) {
            return Ok(false);
        }
        let database_revision = self.policy_store.policy_revision(&mut NoTransaction).await?;
        Ok(self.loaded_policy_revision.load(Ordering::Acquire) == database_revision)
    }

    // 初始化或刷新时用加载前后版本相同确认稳定快照；达到重试上限或读取失败时保持过期，禁止使用旧授权。
    async fn reload_snapshot(&self, target: &mut PolicySnapshot) -> Result<()> {
        self.policy_stale.store(true, Ordering::Release);
        for _ in 0..MAX_STABLE_POLICY_LOAD_ATTEMPTS {
            let before = self.policy_store.policy_revision(&mut NoTransaction).await?;
            let loaded = self.policy_store.load_snapshot().await?;
            let after = self.policy_store.policy_revision(&mut NoTransaction).await?;
            if before == after {
                *target = loaded;
                self.loaded_policy_revision.store(after, Ordering::Release);
                self.policy_stale.store(false, Ordering::Release);
                return Ok(());
            }
        }
        Err(Error::Rbac("授权策略持续变化，无法加载稳定快照，请稍后重试".to_string()))
    }

    /// 策略事务提交后立即将本地快照对齐数据库版本；刷新失败时返回错误，后续鉴权须先刷新成功。
    pub(super) async fn refresh_policy(&self) -> Result<()> {
        self.fresh_snapshot().await.map(|_| ())
    }

    /// 复核策略版本时，先将本地快照对齐数据库，再返回已稳定加载的版本。
    pub async fn current_policy_revision(&self) -> Result<u64> {
        self.fresh_snapshot().await?;
        Ok(self.loaded_policy_revision.load(Ordering::Acquire))
    }

    /// 从调用方执行器所见的数据库快照读取策略版本，供销售形式化等事务在原快照内复核。
    pub async fn policy_revision_with_executor(&self, executor: &mut dyn Executor) -> Result<u64> {
        Ok(self.policy_store.policy_revision(executor).await?)
    }

    /// 为功能资格检查在一次快照读锁下冻结直接绑定的角色、各角色对指定权限的命中及策略版本。
    /// `permissions` 为空时只取角色和版本；角色启用状态由调用方核验。
    pub async fn role_permission_snapshot(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        permissions: &[Permission],
    ) -> Result<RolePermissionSnapshot> {
        let snapshot = self.fresh_snapshot().await?.read().await;
        let role_ids = role_ids_for_account(&snapshot, account_kind, account_id)?;
        let mut grants = HashMap::with_capacity(role_ids.len());
        for role_id in &role_ids {
            let role_subject = role_key(role_id.as_str());
            let granted = permissions
                .iter()
                .filter(|permission| snapshot.allows(&role_subject, permission))
                .cloned()
                .collect::<HashSet<_>>();
            grants.insert(role_id.clone(), granted);
        }
        Ok(RolePermissionSnapshot {
            role_ids,
            grants,
            policy_revision: self.loaded_policy_revision.load(Ordering::Acquire),
        })
    }

    /// 使用冻结的授权事实前，在调用方原执行器中核对可见策略版本与 `expected_revision` 完全一致。
    pub async fn ensure_policy_snapshot_with_executor(
        &self,
        expected_revision: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let visible_revision = self.policy_revision_with_executor(executor).await?;
        ensure_policy_snapshot_revision(expected_revision, visible_revision)
    }
}
