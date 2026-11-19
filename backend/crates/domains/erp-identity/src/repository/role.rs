use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::Role;

/// 为角色集合提供资格取证和内建角色修复所需的查询。
#[allow(async_fn_in_trait)]
pub trait RoleRepositoryExt {
    /// 用调用方执行器读取所有未删除且启用的角色，数据库错误直接返回。
    async fn list_enabled(&self, executor: &mut dyn Executor) -> Result<Vec<Role>>;

    /// 用调用方执行器按 ID 批量读取未删除且启用的角色，结果按 ID 排序，查询错误直接返回。
    async fn enabled_roles(
        &self,
        role_ids: &[impl AsRef<str> + Sync],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Role>>;

    /// 核验目标账号已绑定角色的安全属性时，按 ID 读取未删除角色并按 ID 排序。停用角色仍受系统角色保护，不能过滤掉；查询错误直接返回。
    async fn roles_by_ids(
        &self,
        role_ids: &[impl AsRef<str> + Sync],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Role>>;

    /// 仅供初始化内建角色时修复历史软删除：按 ID 读取角色且包含已删除记录，缺失返回 `None`，数据库错误直接返回。
    async fn find_by_id_including_deleted(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Role>>;
}

impl RoleRepositoryExt for Repository<'_, Role> {
    // 在数据库同时筛掉已删除及停用角色。
    async fn list_enabled(&self, executor: &mut dyn Executor) -> Result<Vec<Role>> {
        self.find_many(
            doc! {
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "disabled": false,
            },
            executor,
        )
        .await
    }

    // 批量取证只保留启用角色，复用固定的按 ID 排序查询。
    async fn enabled_roles(
        &self,
        role_ids: &[impl AsRef<str> + Sync],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Role>> {
        find_roles_by_ids(self, role_ids, true, executor).await
    }

    // 校验已有角色保护时保留停用角色，仍排除软删除记录。
    async fn roles_by_ids(
        &self,
        role_ids: &[impl AsRef<str> + Sync],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Role>> {
        find_roles_by_ids(self, role_ids, false, executor).await
    }

    // 直接查原集合，避免通用软删除过滤遮蔽待修复角色。
    async fn find_by_id_including_deleted(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Role>> {
        mongo_ops::find_one(&self.collection(), doc! { "id": id }, executor).await
    }
}

// 共用按稳定 ID 批量查找及排序逻辑；空列表不查库，仅在 `only_enabled` 时排除停用角色，查询错误直接返回。
async fn find_roles_by_ids(
    repo: &Repository<'_, Role>,
    role_ids: &[impl AsRef<str> + Sync],
    only_enabled: bool,
    executor: &mut dyn Executor,
) -> Result<Vec<Role>> {
    if role_ids.is_empty() {
        return Ok(Vec::new());
    }

    let ids = role_ids.iter().map(AsRef::as_ref).collect::<Vec<_>>();
    let mut filter = doc! { "id": { "$in": ids } };
    if only_enabled {
        filter.insert("disabled", false);
    }
    let mut roles = repo.find_many(filter, executor).await?;
    roles.sort_by(|left, right| left.base.id.cmp(&right.base.id));
    Ok(roles)
}
