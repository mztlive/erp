use super::Repository;
use crate::errors::Result;
use entities::Role;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::{bson::doc, ClientSession};

impl<'a> Repository<'a, Role> {
    /// 查询全部未删除且启用的角色。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn list_enabled(&self) -> Result<Vec<Role>> {
        self.find_many(doc! {
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "disabled": false,
        })
        .await
    }

    /// 查询一组存在且启用的角色。
    ///
    /// # 参数
    /// * `role_ids` - 待校验的角色 ID
    ///
    /// # 返回值
    /// 返回存在且启用的角色，并按角色 ID 排序。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn enabled_roles(&self, role_ids: &[String]) -> Result<Vec<Role>> {
        if role_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut roles = self
            .find_many(doc! {
                "id": { "$in": role_ids },
                "disabled": false,
            })
            .await?;
        roles.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(roles)
    }

    /// 查询一组未删除角色，不要求角色处于启用状态。
    ///
    /// 该查询用于校验目标账号当前已绑定角色的安全属性；已停用角色仍然属于
    /// 目标账号现有权限边界，不能因停用而绕过系统角色保护。
    ///
    /// # 参数
    /// * `role_ids` - 待查询的角色 ID
    ///
    /// # 返回值
    /// 返回存在的未删除角色，并按角色 ID 排序。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn roles_by_ids(&self, role_ids: &[String]) -> Result<Vec<Role>> {
        if role_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut roles = self.find_many(doc! { "id": { "$in": role_ids } }).await?;
        roles.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(roles)
    }

    /// 根据 ID 查询角色，包含已软删除记录。
    ///
    /// 该查询仅供内建角色初始化修复历史软删除数据。
    ///
    /// # 参数
    /// * `id` - 角色 ID
    ///
    /// # 返回值
    /// 返回匹配的角色记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id_including_deleted(&self, id: &str) -> Result<Option<Role>> {
        let role = self
            .database()
            .collection::<Role>(self.collection_name())
            .find_one(doc! { "id": id })
            .await?;
        Ok(role)
    }

    /// 在调用方事务中查询一组存在且启用的角色。
    ///
    /// # 参数
    /// * `role_ids` - 待校验的角色 ID
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 返回值
    /// 返回同一事务快照内存在且启用的角色，并按角色 ID 排序。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn enabled_roles_with_session(
        &self,
        role_ids: &[String],
        session: &mut ClientSession,
    ) -> Result<Vec<Role>> {
        if role_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut roles = self
            .find_many_with_session(
                doc! {
                    "id": { "$in": role_ids },
                    "disabled": false,
                },
                session,
            )
            .await?;
        roles.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(roles)
    }
}
