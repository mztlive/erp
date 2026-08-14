//! 预定义角色启动种子与缺失权限补齐。

use std::sync::Arc;

use database::{AccessControlExt, NoTransaction};
use entities::{Permission, PermissionSet, RoleData};

use super::RbacService;
use crate::errors::{Error, Result};

impl RbacService {
    /// 若角色 ID 尚不存在则创建预定义角色及其权限；已存在（含软删除）则不改动。
    ///
    /// 与 [`super::ensure_root_role`] 不同：本方法**不会**修复名称、system 标记或 Casbin
    /// policy，以便管理员在首次种子之后调整展示信息，进程重启不会被覆盖。
    ///
    /// # 参数
    /// * `id` - 固定角色 ID
    /// * `data` - 角色展示信息
    /// * `permissions` - 首次创建时写入的权限集合
    ///
    /// # 返回值
    /// 新建成功返回 `true`；已存在或并发写入冲突时返回 `false`。
    ///
    /// # 错误
    /// 角色校验、MongoDB 或 Casbin policy 写入失败（非并发冲突）时返回错误。
    ///
    /// # 业务约束
    /// 软删除记录视为已存在，避免把管理员删除的预定义角色重新创建出来。
    pub(in crate::iam) async fn seed_role_if_absent(
        self: &Arc<Self>,
        id: &str,
        data: RoleData,
        permissions: Vec<Permission>,
    ) -> Result<bool> {
        if self
            .db
            .roles()
            .find_by_id_including_deleted(id, &mut NoTransaction)
            .await?
            .is_some()
        {
            return Ok(false);
        }

        match self
            .create_role_with_id(id.to_string(), data, permissions, None, None)
            .await
        {
            Ok(_) => Ok(true),
            Err(Error::ConflictError(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// 仅当预定义角色权限仍与旧种子完全一致时升级到新种子。
    ///
    /// 管理员已经增删过权限时保持原样；多实例并发升级产生冲突后会重新读取，若另一
    /// 实例已完成相同升级则按幂等成功处理。
    ///
    /// # 参数
    /// * `role_id` - 预定义角色 ID
    /// * `previous` - 可安全识别的旧默认权限快照
    /// * `desired` - 当前推荐权限
    ///
    /// # 返回值
    /// 完成整集替换返回 `true`；角色不存在、权限已改或并发后已对齐时返回 `false`。
    ///
    /// # 错误
    /// MongoDB 或 Casbin policy 写入失败，且冲突后权限仍未对齐时返回错误。
    ///
    /// # 业务约束
    /// 只匹配精确旧快照，用于权限替换（删旧加新）；自定义角色交给缺失权限补齐。
    pub(in crate::iam) async fn upgrade_seeded_role_permissions_if_exact(
        self: &Arc<Self>,
        role_id: &str,
        previous: Vec<Permission>,
        desired: Vec<Permission>,
    ) -> Result<bool> {
        if !self.active_role_exists(role_id).await? {
            return Ok(false);
        }
        let previous = PermissionSet::new(previous);
        let desired_set = PermissionSet::new(desired.clone());
        if self.direct_role_permissions(role_id).await? != previous {
            return Ok(false);
        }
        self.commit_seeded_role_permissions(role_id, desired, |latest| latest == &desired_set)
            .await
    }

    /// 为已存在的预定义角色补齐当前种子中尚未覆盖的权限。
    ///
    /// # 参数
    /// * `role_id` - 预定义角色 ID
    /// * `desired` - 当前推荐权限
    ///
    /// # 返回值
    /// 实际追加了缺失权限返回 `true`；角色不存在、已覆盖或并发后已覆盖返回 `false`。
    ///
    /// # 错误
    /// MongoDB 或 Casbin policy 写入失败，且冲突后仍未覆盖推荐权限时返回错误。
    ///
    /// # 业务约束
    /// 只追加缺失权限，不删除管理员额外授予的权限，也不改名称、启停与 system 标记。
    pub(in crate::iam) async fn ensure_missing_seeded_role_permissions(
        self: &Arc<Self>,
        role_id: &str,
        desired: Vec<Permission>,
    ) -> Result<bool> {
        if !self.active_role_exists(role_id).await? {
            return Ok(false);
        }
        let current = self.direct_role_permissions(role_id).await?;
        let desired = PermissionSet::new(desired);
        let Some(merged) = current.with_missing(&desired) else {
            return Ok(false);
        };
        self.commit_seeded_role_permissions(role_id, merged.into_vec(), |latest| latest.covers(&desired))
            .await
    }

    /// 判断未删除的预定义角色是否存在。
    ///
    /// # 参数
    /// * `role_id` - 角色 ID
    ///
    /// # 返回值
    /// 角色存在且未软删除时返回 `true`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    ///
    /// # 业务约束
    /// 软删除角色不补权限，避免把已下线岗位重新写回 policy。
    async fn active_role_exists(&self, role_id: &str) -> Result<bool> {
        Ok(self
            .db
            .roles()
            .find_by_id(role_id, &mut NoTransaction)
            .await?
            .is_some())
    }

    /// 提交预定义角色权限替换，并把并发冲突收敛为幂等结果。
    ///
    /// # 参数
    /// * `role_id` - 角色 ID
    /// * `permissions` - 即将写入的完整权限集合
    /// * `conflict_settled` - 冲突后根据最新直接权限判断是否已对齐
    ///
    /// # 返回值
    /// 本次写入成功返回 `true`；冲突后权限已满足目标返回 `false`。
    ///
    /// # 错误
    /// 非冲突写入失败，或冲突后权限仍未对齐时返回错误。
    ///
    /// # 业务约束
    /// 内建种子写入不带操作人审计；并发冲突不得覆盖另一实例已提交的结果。
    async fn commit_seeded_role_permissions(
        self: &Arc<Self>,
        role_id: &str,
        permissions: Vec<Permission>,
        conflict_settled: impl Fn(&PermissionSet) -> bool,
    ) -> Result<bool> {
        match self
            .replace_role_permissions(role_id, permissions, None, None)
            .await
        {
            Ok(_) => Ok(true),
            Err(error @ Error::ConflictError(_)) => {
                if conflict_settled(&self.direct_role_permissions(role_id).await?) {
                    Ok(false)
                } else {
                    Err(error)
                }
            }
            Err(error) => Err(error),
        }
    }
}
