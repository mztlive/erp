//! 预定义角色启动种子与缺失权限补齐。

use std::sync::Arc;

use persistence_core::NoTransaction;

use super::RbacService;
use crate::entity::access_control::{DataScope, DataScopeData, DataScopeId, DataScopeSubjectType};
use crate::entity::{Permission, PermissionSet, RoleData};
use crate::error::{Error, Result};
use crate::repository::prelude::*;
use crate::service::access_control::consumers::validate_binding;
use crate::{AccessControlExt, MongoCasbinAdapter};

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
    pub async fn seed_role_if_absent(
        self: &Arc<Self>,
        id: &str,
        data: RoleData,
        permissions: Vec<Permission>,
    ) -> Result<bool> {
        if self.db.roles().find_by_id_including_deleted(id, &mut NoTransaction).await?.is_some() {
            return Ok(false);
        }

        match self.create_role_with_id(id.to_string(), data, permissions, None, None).await {
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
    pub async fn upgrade_seeded_role_permissions_if_exact(
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
        self.commit_seeded_role_permissions(role_id, desired, |latest| latest == &desired_set).await
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
    pub async fn ensure_missing_seeded_role_permissions(
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

    /// 按资源原子登记首次授权清单，已有任意配置（含软删除）时不再初始化。
    ///
    /// # 错误
    /// 查询、唯一冲突或事务失败返回原错误，禁止失败后扩大为 Company。
    pub async fn seed_data_scope_manifest(
        self: &Arc<Self>,
        role_id: &str,
        resource: &str,
        definitions: Vec<DataScopeData>,
    ) -> Result<()> {
        validate_scope_manifest(role_id, resource, &definitions)?;
        if !self.active_role_exists(role_id).await? {
            return Ok(());
        }
        let existing =
            self.db.data_scopes().has_subject_resource_history(role_id, resource, &mut NoTransaction).await?;
        if existing {
            return Ok(());
        }
        let scopes = definitions
            .into_iter()
            .map(|data| {
                DataScope::new(
                    DataScopeId::new(format!("s2:{}:{}:{}", role_id, resource, data.scope_type.as_str())),
                    data,
                )
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let db = self.db.clone();
        use persistence_core::Transactional;
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let Some(first) = scopes.first() else {
                        return Ok::<(), Error>(());
                    };
                    if !db.roles().exists_active_by_id(&first.subject_id, executor).await? {
                        return Ok(());
                    }
                    if db
                        .data_scopes()
                        .has_subject_resource_history(&first.subject_id, &first.binding.resource, executor)
                        .await?
                    {
                        return Ok(());
                    }
                    for scope in &scopes {
                        db.data_scopes().create(scope, executor).await?;
                    }
                    MongoCasbinAdapter::new(db.clone()).bump_policy_revision(executor).await?;
                    Ok::<(), Error>(())
                })
            })
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
        Ok(self.db.roles().exists_active_by_id(role_id, &mut NoTransaction).await?)
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
        match self.replace_role_permissions(role_id, permissions, None, None).await {
            Ok(_) => Ok(true),
            Err(error @ Error::ConflictError(_)) => {
                if conflict_settled(&self.direct_role_permissions(role_id).await?) {
                    Ok(false)
                } else {
                    Err(error)
                }
            },
            Err(error) => Err(error),
        }
    }
}

/// 初始化载荷须在任何数据库读写之前证明消费者准入与主体一致性。
fn validate_scope_manifest(role: &str, resource: &str, definitions: &[DataScopeData]) -> Result<()> {
    if definitions.is_empty() {
        return Err(Error::ValidationError("初始化范围清单不能为空".into()));
    }
    for data in definitions {
        if data.subject_type != DataScopeSubjectType::Role
            || data.subject_id != role
            || data.binding.resource != resource
        {
            return Err(Error::ValidationError("初始化范围主体或资源与清单不一致".into()));
        }
        data.binding.validate(data.scope_type, &data.scope_targets)?;
        validate_binding(&data.binding)?;
    }
    Ok(())
}

#[cfg(test)]
mod scope_manifest_tests {
    use super::*;
    use crate::access_control::{DataScopeType, ScopeBinding, ScopeDimension};

    fn data() -> DataScopeData {
        DataScopeData {
            subject_type: DataScopeSubjectType::Role,
            subject_id: "role-sales".into(),
            scope_type: DataScopeType::Company,
            scope_targets: vec![],
            binding: ScopeBinding {
                schema_version: 2,
                resource: "customer".into(),
                actions: vec!["list".into()],
                target_dimension: ScopeDimension::InternalOrg,
                target_mode: None,
                include_descendants: None,
                enabled: true,
            },
        }
    }

    #[test]
    fn seed_rejects_unwired_mismatched_and_empty_manifests_before_io() {
        let good = data();
        assert!(validate_scope_manifest("role-sales", "customer", std::slice::from_ref(&good)).is_ok());
        assert!(validate_scope_manifest("role-other", "customer", std::slice::from_ref(&good)).is_err());
        assert!(validate_scope_manifest("role-sales", "contract", std::slice::from_ref(&good)).is_err());
        assert!(validate_scope_manifest("role-sales", "customer", &[]).is_err());
        let mut bad = good.clone();
        bad.binding.actions.push("manage".into());
        assert!(validate_scope_manifest("role-sales", "customer", &[bad]).is_err());
        let mut bad = good.clone();
        bad.binding.resource = "work_item".into();
        assert!(validate_scope_manifest("role-sales", "work_item", &[bad]).is_err());
        let mut bad = good;
        bad.binding.target_dimension = ScopeDimension::Warehouse;
        assert!(validate_scope_manifest("role-sales", "customer", &[bad]).is_err());
    }
}
