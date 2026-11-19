use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

use casbin::{CoreApi, DefaultModel, Enforcer};
use erp_core::AccountKind;
use persistence_core::{Executor, NoTransaction};
use tokio::sync::RwLock;

use super::policy::{ensure_policy_snapshot_revision, role_ids_for_account};
use super::{MAX_STABLE_POLICY_LOAD_ATTEMPTS, RBAC_MODEL, RbacService, RolePermissionSnapshot};
use crate::entity::Permission;
use crate::error::{Error, Result};

impl RbacService {
    // 首次访问时按固定模型和 MongoDB 策略初始化本地 Enforcer；关闭自动保存，由策略事务负责写入。
    // 模型或策略加载失败时保留未初始化状态，供下次重试。
    async fn enforcer(&self) -> Result<&RwLock<Enforcer>> {
        self.enforcer
            .get_or_try_init(|| async {
                let model = DefaultModel::from_str(RBAC_MODEL).await?;
                let adapter = self.policy_store.clone();
                let mut enforcer = Enforcer::new_raw(model, adapter).await?;
                enforcer.enable_auto_save(false);

                self.reload_enforcer(&mut enforcer).await?;
                Ok(RwLock::new(enforcer))
            })
            .await
    }

    /// 鉴权或查询策略前取得与数据库策略版本对齐的 Enforcer 锁；缓存过期时在写锁内复查并重载。
    /// 提交结果未知时停止授权；策略持续变化或重载失败时直接返回错误，下次读取重试刷新，旧缓存保持不可用。
    pub(super) async fn fresh_enforcer(&self) -> Result<&RwLock<Enforcer>> {
        self.ensure_policy_consistency_known()?;
        let enforcer = self.enforcer().await?;
        if self.policy_cache_is_current().await? {
            self.ensure_policy_consistency_known()?;
            return Ok(enforcer);
        }

        let mut guard = enforcer.write().await;
        if !self.policy_cache_is_current().await? {
            self.reload_enforcer(&mut guard).await?;
        }
        drop(guard);
        self.ensure_policy_consistency_known()?;
        Ok(enforcer)
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
    async fn reload_enforcer(&self, enforcer: &mut Enforcer) -> Result<()> {
        self.policy_stale.store(true, Ordering::Release);
        for _ in 0..MAX_STABLE_POLICY_LOAD_ATTEMPTS {
            let before = self.policy_store.policy_revision(&mut NoTransaction).await?;
            enforcer.load_policy().await?;
            let after = self.policy_store.policy_revision(&mut NoTransaction).await?;
            if before == after {
                self.loaded_policy_revision.store(after, Ordering::Release);
                self.policy_stale.store(false, Ordering::Release);
                return Ok(());
            }
        }
        Err(Error::Rbac("授权策略持续变化，无法加载稳定快照，请稍后重试".to_string()))
    }

    /// 策略事务提交后立即将本地 Enforcer 对齐数据库版本；刷新失败时返回错误，后续鉴权或查询须先刷新成功。
    pub(super) async fn refresh_policy(&self) -> Result<()> {
        self.fresh_enforcer().await.map(|_| ())
    }

    /// 复核策略版本时，先将本地 Enforcer 对齐数据库，再返回已稳定加载的版本；读取或刷新失败时返回错误。
    pub async fn current_policy_revision(&self) -> Result<u64> {
        self.fresh_enforcer().await?;
        Ok(self.loaded_policy_revision.load(Ordering::Acquire))
    }

    /// 从调用方执行器所见的数据库快照读取策略版本，供销售形式化等事务在原快照内复核；读取失败时返回错误。
    pub async fn policy_revision_with_executor(&self, executor: &mut dyn Executor) -> Result<u64> {
        Ok(self.policy_store.policy_revision(executor).await?)
    }

    /// 为功能资格检查在一次 Enforcer 读锁下冻结直接绑定的角色、各角色对指定权限的命中及策略版本。
    /// `permissions` 为空时只取角色和版本；角色启用状态由调用方核验，刷新或匹配失败时返回错误。
    pub async fn role_permission_snapshot(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        permissions: &[Permission],
    ) -> Result<RolePermissionSnapshot> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        let role_ids = role_ids_for_account(&enforcer, account_kind, account_id)?;
        let mut grants = HashMap::with_capacity(role_ids.len());
        for role_id in &role_ids {
            let role_subject = format!("role:{role_id}");
            let mut granted = HashSet::new();
            for permission in permissions {
                if enforcer.enforce((role_subject.as_str(), permission.resource(), permission.action()))? {
                    granted.insert(permission.clone());
                }
            }
            grants.insert(role_id.clone(), granted);
        }
        Ok(RolePermissionSnapshot {
            role_ids,
            grants,
            policy_revision: self.loaded_policy_revision.load(Ordering::Acquire),
        })
    }

    /// 使用冻结的授权事实前，在调用方原执行器中核对可见策略版本与 `expected_revision` 完全一致。
    /// 版本读取失败或不一致时拒绝使用该快照。
    pub async fn ensure_policy_snapshot_with_executor(
        &self,
        expected_revision: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let visible_revision = self.policy_revision_with_executor(executor).await?;
        ensure_policy_snapshot_revision(expected_revision, visible_revision)
    }
}
