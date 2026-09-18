//! 域 D06 `access_control` 仓储：permission 定义与 user_role 绑定查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::{permission_projection, sort_doc};
use crate::entity::access_control::{Permission, UserRole};

/// 权限定义列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionRow {
    /// 实体主键。
    pub id: String,
    /// 权限资源。
    pub resource: String,
    /// 权限动作。
    pub action: String,
    /// 展示名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 系统内建权限标记。
    pub system: bool,
    /// 停用标记。
    pub disabled: bool,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 权限定义列表筛选条件。
#[derive(Debug, Clone)]
pub struct PermissionFilter {
    /// 权限资源（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub resource: Option<String>,
    /// 停用标记；`None` 表示不筛选。
    pub disabled: Option<bool>,
    /// 是否仅系统内建；`None` 表示不筛选。
    pub system: Option<bool>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for PermissionFilter {
    /// 返回首页空筛选（`page: 1`，`page_size: 20`）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回筛选为空、降序的首页过滤条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            resource: None,
            disabled: None,
            system: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for PermissionFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "resource", self.resource.as_deref());
        if let Some(disabled) = self.disabled {
            filter.insert("disabled", disabled);
        }
        if let Some(system) = self.system {
            filter.insert("system", system);
        }
        filter
    }
}

impl Pagination for PermissionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 权限定义集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait PermissionRepositoryExt {
    /// 分页检索权限定义列表（投影查询，权限目录）。
    ///
    /// 只返回 [`PermissionRow`] 所需的目录字段，不加载整文档；`resource` 按
    /// 字面量忽略大小写模糊匹配（复用 `repository::regex_filter`），停用/系统
    /// 标记精确匹配覆盖 `idx_permissions_disabled`。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    async fn search_permissions(
        &self,
        filter: &PermissionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PermissionRow>>;
}

impl PermissionRepositoryExt for Repository<'_, Permission> {
    async fn search_permissions(
        &self,
        filter: &PermissionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PermissionRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(permission_projection())
            .build();
        let collection = self.collection().clone_with_type::<PermissionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }
}

/// 用户角色绑定集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait UserRoleRepositoryExt {
    /// 按用户批量取回全部角色绑定（W19：按当前、未来、已过期分开只读展示）。
    ///
    /// # 参数
    /// * `user_id` - 用户 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按生效时间倒序排列的绑定记录（含已撤权历史）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn list_by_user(&self, user_id: &str, executor: &mut dyn Executor) -> Result<Vec<UserRole>>;
}

impl UserRoleRepositoryExt for Repository<'_, UserRole> {
    async fn list_by_user(&self, user_id: &str, executor: &mut dyn Executor) -> Result<Vec<UserRole>> {
        self.find_many_sorted(doc! { "user_id": user_id }, doc! { "effective_from": -1 }, executor).await
    }
}
