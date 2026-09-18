use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::Role;

/// 角色集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait RoleRepositoryExt {
    /// 判断指定角色 ID 是否对应未软删除记录。
    ///
    /// 本方法不把 `disabled` 解释为不存在；只用于种子过程区分已软删除身份，
    /// 并通过 `_id` 窄投影在首条命中后停止。
    ///
    /// # 参数
    /// * `id` - 角色 ID
    /// * `executor` - 调用方事务或非事务执行器
    ///
    /// # 返回值
    /// 角色存在且未软删除时返回 `true`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn exists_active_by_id(&self, id: &str, executor: &mut dyn Executor) -> Result<bool>;

    /// 查询全部未删除且启用的角色。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回全部未删除且启用的角色。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn list_enabled(&self, executor: &mut dyn Executor) -> Result<Vec<Role>>;

    /// 查询一组存在且启用的角色。
    ///
    /// # 参数
    /// * `role_ids` - 待校验的角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回存在且启用的角色，并按角色 ID 排序。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn enabled_roles(&self, role_ids: &[String], executor: &mut dyn Executor) -> Result<Vec<Role>>;

    /// 查询一组未删除角色，不要求角色处于启用状态。
    ///
    /// 该查询用于校验目标账号当前已绑定角色的安全属性；已停用角色仍然属于
    /// 目标账号现有权限边界，不能因停用而绕过系统角色保护。
    ///
    /// # 参数
    /// * `role_ids` - 待查询的角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回存在的未删除角色，并按角色 ID 排序。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn roles_by_ids(&self, role_ids: &[String], executor: &mut dyn Executor) -> Result<Vec<Role>>;

    /// 根据 ID 查询角色，包含已软删除记录。
    ///
    /// 该查询仅供内建角色初始化修复历史软删除数据。
    ///
    /// # 参数
    /// * `id` - 角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回匹配的角色记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_by_id_including_deleted(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Role>>;
}

impl RoleRepositoryExt for Repository<'_, Role> {
    async fn exists_active_by_id(&self, id: &str, executor: &mut dyn Executor) -> Result<bool> {
        self.exists(doc! { "id": id }, executor).await
    }

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

    async fn enabled_roles(&self, role_ids: &[String], executor: &mut dyn Executor) -> Result<Vec<Role>> {
        find_roles_by_ids(self, role_ids, true, executor).await
    }

    async fn roles_by_ids(&self, role_ids: &[String], executor: &mut dyn Executor) -> Result<Vec<Role>> {
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

/// 按稳定 ID 批量加载未删除角色并按角色 ID 排序。
///
/// # 参数
/// * `role_ids` - 待查询的角色 ID；为空时直接返回空集合
/// * `only_enabled` - 为 `true` 时只返回未停用角色
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回值
/// 返回存在的未删除角色，并按角色 ID 排序。
///
/// # 错误
/// 当 MongoDB 查询失败时返回错误。
async fn find_roles_by_ids(
    repo: &Repository<'_, Role>,
    role_ids: &[String],
    only_enabled: bool,
    executor: &mut dyn Executor,
) -> Result<Vec<Role>> {
    if role_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut filter = doc! { "id": { "$in": role_ids } };
    if only_enabled {
        filter.insert("disabled", false);
    }
    let mut roles = repo.find_many(filter, executor).await?;
    roles.sort_by(|left, right| left.base.id.cmp(&right.base.id));
    Ok(roles)
}
