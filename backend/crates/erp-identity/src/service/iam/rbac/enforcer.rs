use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

use casbin::{CoreApi, DefaultModel, Enforcer};
use erp_core::AccountKind;
use persistence_core::{Executor, NoTransaction};
use tokio::sync::RwLock;

use super::policy::{
    ensure_policy_snapshot_revision, policy_revisions_match, rbac_error, role_ids_for_account,
    stable_policy_revision,
};
use super::{MAX_STABLE_POLICY_LOAD_ATTEMPTS, RBAC_MODEL, RbacService, RolePermissionSnapshot};
use crate::entity::Permission;
use crate::error::{Error, Result};

impl RbacService {
    /// 首次使用时从 MongoDB 装载 Enforcer；模型或策略加载失败则不缓存。
    async fn enforcer(&self) -> Result<&RwLock<Enforcer>> {
        self.enforcer
            .get_or_try_init(|| async {
                let model = DefaultModel::from_str(RBAC_MODEL).await.map_err(rbac_error)?;
                let adapter = self.policy_store.clone();
                let mut enforcer = Enforcer::new_raw(model, adapter).await.map_err(rbac_error)?;
                enforcer.enable_auto_save(false);
                self.reload_enforcer(&mut enforcer).await?;
                Ok(RwLock::new(enforcer))
            })
            .await
    }

    /// 返回已恢复到最新 MongoDB policy 的 Enforcer。
    ///
    /// 上一次持久化或 reload 失败后，任何鉴权和 policy 查询都会先重试 reload；
    /// reload 继续失败时直接返回错误，禁止使用旧授权缓存。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与当前数据库 policy 版本一致的 Enforcer 锁。
    ///
    /// # 错误
    /// 提交结果未知、策略持续变化或重新加载失败时返回错误。
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

    /// 判断本地 Enforcer 是否对应数据库当前 policy 版本。
    async fn policy_cache_is_current(&self) -> Result<bool> {
        if self.policy_stale.load(Ordering::Acquire) {
            return Ok(false);
        }
        let database_revision = self.policy_store.policy_revision(&mut NoTransaction).await?;
        Ok(policy_revisions_match(self.loaded_policy_revision.load(Ordering::Acquire), database_revision))
    }

    /// 从 MongoDB 重新加载稳定 policy 快照，并维护 fail-closed stale 状态。
    async fn reload_enforcer(&self, enforcer: &mut Enforcer) -> Result<()> {
        self.policy_stale.store(true, Ordering::Release);
        for _ in 0..MAX_STABLE_POLICY_LOAD_ATTEMPTS {
            let before = self.policy_store.policy_revision(&mut NoTransaction).await?;
            enforcer.load_policy().await.map_err(rbac_error)?;
            let after = self.policy_store.policy_revision(&mut NoTransaction).await?;
            if let Some(revision) = stable_policy_revision(before, after) {
                self.loaded_policy_revision.store(revision, Ordering::Release);
                self.policy_stale.store(false, Ordering::Release);
                return Ok(());
            }
        }
        Err(Error::Rbac("授权策略持续变化，无法加载稳定快照，请稍后重试".to_string()))
    }

    /// 提交外部 policy 事务后，立即刷新本地 Enforcer。
    ///
    /// reload 失败时服务保持 stale；后续鉴权和 policy 查询会先重试，
    /// 在恢复前不会继续使用旧缓存。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 本地 Enforcer 已对齐当前 policy 时无返回值。
    ///
    /// # 错误
    /// 当 MongoDB policy 无法重新加载时返回错误。
    pub(super) async fn refresh_policy(&self) -> Result<()> {
        self.fresh_enforcer().await.map(|_| ())
    }

    /// 判断账号是否具有指定权限。
    ///
    /// # 参数
    /// * `subject` - Casbin 主体标识。
    /// * `permission` - 待判断的资源动作。
    ///
    /// # 返回
    /// 当前策略允许该主体执行该权限时返回 `true`。
    ///
    /// # 错误
    /// 当 Casbin policy 加载或匹配失败时返回错误。
    pub async fn enforce(&self, subject: &str, permission: &Permission) -> Result<bool> {
        self.fresh_enforcer()
            .await?
            .read()
            .await
            .enforce((subject, permission.resource(), permission.action()))
            .map_err(rbac_error)
    }

    /// 返回当前已稳定加载的授权策略版本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与本地 Enforcer 一致的 MongoDB policy revision。
    ///
    /// # 错误
    /// 策略缓存无法刷新或数据库版本读取失败时返回错误。
    pub async fn current_policy_revision(&self) -> Result<u64> {
        self.fresh_enforcer().await?;
        Ok(self.loaded_policy_revision.load(Ordering::Acquire))
    }

    /// 在指定执行器快照中读取授权策略版本。
    ///
    /// # 参数
    /// * `executor` - 数据库执行器，可为销售形式化事务会话
    ///
    /// # 返回
    /// 返回执行器所见的 policy revision。
    ///
    /// # 错误
    /// 数据库读取失败时返回错误。
    pub async fn policy_revision_with_executor(&self, executor: &mut dyn Executor) -> Result<u64> {
        Ok(self.policy_store.policy_revision(executor).await?)
    }

    /// 在一次 Enforcer 读锁下冻结账号角色及指定权限的逐角色授权结果。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_id` - 账号 ID。
    /// * `permissions` - 需要逐角色核对的权限；为空时只冻结角色列表。
    ///
    /// # 返回
    /// 返回角色、各角色命中的权限以及当时的 policy 版本。
    ///
    /// # 错误
    /// Enforcer 无法刷新或权限匹配失败时返回错误。
    pub async fn role_permission_snapshot(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        permissions: &[Permission],
    ) -> Result<RolePermissionSnapshot> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        let role_ids = role_ids_for_account(&enforcer, account_kind, account_id);
        let mut grants = HashMap::with_capacity(role_ids.len());
        for role_id in &role_ids {
            let role_subject = format!("role:{role_id}");
            let mut granted = HashSet::new();
            for permission in permissions {
                if enforcer
                    .enforce((role_subject.as_str(), permission.resource(), permission.action()))
                    .map_err(rbac_error)?
                {
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

    /// 证明冻结 Enforcer revision 与调用方事务快照可见 revision 完全一致。
    ///
    /// # 参数
    /// * `expected_revision` - 冻结快照中的 policy 版本。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 两个版本一致时无返回值。
    ///
    /// # 错误
    /// 版本读取失败或两边不一致时返回错误。
    pub async fn ensure_policy_snapshot_with_executor(
        &self,
        expected_revision: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let visible_revision = self.policy_revision_with_executor(executor).await?;
        ensure_policy_snapshot_revision(expected_revision, visible_revision)
    }
}
