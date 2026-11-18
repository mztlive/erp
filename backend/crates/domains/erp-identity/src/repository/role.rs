use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::Role;

/// 角色集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait RoleRepositoryExt {
    /// 查询全部未删除且启用的角色。
    /// 返回全部未删除且启用的角色。
    ///
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// `executor`：数据访问执行器，由 Service 决定是否位于事务中。
    async fn list_enabled(&self, executor: &mut dyn Executor) -> Result<Vec<Role>>;

    /// 查询一组存在且启用的角色。
    /// 返回存在且启用的角色，并按角色 ID 排序。
    ///
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// `executor`：数据访问执行器，由 Service 决定是否位于事务中。
    async fn enabled_roles(
        &self,
        role_ids: &[impl AsRef<str> + Sync],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Role>>;

    /// 查询一组未删除角色，不要求角色处于启用状态。
    ///
    /// 该查询用于校验目标账号当前已绑定角色的安全属性；已停用角色仍然属于
    /// 目标账号现有权限边界，不能因停用而绕过系统角色保护。
    ///
    /// 返回存在的未删除角色，并按角色 ID 排序。
    ///
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// `executor`：数据访问执行器，由 Service 决定是否位于事务中。
    async fn roles_by_ids(
        &self,
        role_ids: &[impl AsRef<str> + Sync],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Role>>;

    /// 根据 ID 查询角色，包含已软删除记录。
    ///
    /// 该查询仅供内建角色初始化修复历史软删除数据。
    ///
    /// 返回匹配的角色记录，包含已软删除记录；不存在时返回 `None`。
    ///
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// `executor`：数据访问执行器，由 Service 决定是否位于事务中。
    async fn find_by_id_including_deleted(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Role>>;
}

impl RoleRepositoryExt for Repository<'_, Role> {
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

    async fn enabled_roles(
        &self,
        role_ids: &[impl AsRef<str> + Sync],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Role>> {
        find_roles_by_ids(self, role_ids, true, executor).await
    }

    async fn roles_by_ids(
        &self,
        role_ids: &[impl AsRef<str> + Sync],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Role>> {
        find_roles_by_ids(self, role_ids, false, executor).await
    }

    async fn find_by_id_including_deleted(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Role>> {
        mongo_ops::find_one(&self.collection(), doc! { "id": id }, executor).await
    }
}

// 按稳定 ID 批量加载未删除角色并按角色 ID 排序。
// 返回存在的未删除角色，并按角色 ID 排序。
//
// 当 MongoDB 查询失败时返回错误。
//
// `role_ids`：待查询的角色 ID；为空时直接返回空集合；`only_enabled`：为 `true` 时只返回未停用角色。
// `executor`：数据访问执行器，由 Service 决定是否位于事务中。
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
