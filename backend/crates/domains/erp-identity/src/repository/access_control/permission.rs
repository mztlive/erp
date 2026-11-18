//! 域 D06 `access_control` 仓储：permission 定义与 user_role 绑定查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter, mongo_ops,
};

use super::{permission_projection, sort_doc};
use crate::dto::PermissionView;
use crate::entity::access_control::paging::PageParams;
use crate::entity::access_control::{PermissionDefinition, UserRole};

/// 权限定义列表筛选条件。
#[derive(Debug, Clone)]
pub struct PermissionFilter {
    /// 权限资源（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub resource: Option<String>,
    /// 停用标记；`None` 表示不筛选。
    pub disabled: Option<bool>,
    /// 是否仅系统内建；`None` 表示不筛选。
    pub system: Option<bool>,
    /// 已验证的分页与排序。
    pub paging: PageParams,
}

impl Default for PermissionFilter {
    // 返回首页空筛选（`page: 1`，`page_size: 20`）。
    // 返回筛选为空、降序的首页过滤条件。
    fn default() -> Self {
        Self { resource: None, disabled: None, system: None, paging: PageParams::default() }
    }
}

impl QueryFilter for PermissionFilter {
    // 转换为 MongoDB 查询条件（自动追加未删除过滤）。
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
    // 返回页码与单页条数。
    fn page_and_size(&self) -> (u64, u64) {
        (self.paging.page, u64::from(self.paging.page_size))
    }
}

/// 权限定义集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait PermissionRepositoryExt {
    /// 分页检索权限定义列表（投影查询，权限目录）。
    ///
    /// 只返回 [`PermissionView`] 所需的目录字段，不加载整文档；`resource` 按
    /// 字面量忽略大小写模糊匹配（复用 `insert_literal_regex_filter`），停用/系统
    /// 标记精确匹配覆盖 `idx_permissions_disabled`。
    ///
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    ///
    /// `executor`：数据访问执行器，由 Service 决定是否位于事务中。
    async fn search_permissions(
        &self,
        filter: &PermissionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PermissionView>>;
}

impl PermissionRepositoryExt for Repository<'_, PermissionDefinition> {
    async fn search_permissions(
        &self,
        filter: &PermissionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PermissionView>> {
        let options = FindOptions::builder()
            .sort(sort_doc(&filter.paging))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(permission_projection())
            .build();

        let collection = self.collection().clone_with_type::<PermissionView>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }
}

/// 用户角色绑定集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait UserRoleRepositoryExt {
    /// 按用户批量取回全部角色绑定（W19：按当前、未来、已过期分开只读展示）。
    /// 返回按生效时间倒序排列的绑定记录（含已撤权历史）。
    ///
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// `executor`：数据访问执行器，由 Service 决定是否位于事务中。
    async fn list_by_user(&self, user_id: &str, executor: &mut dyn Executor) -> Result<Vec<UserRole>>;
}

impl UserRoleRepositoryExt for Repository<'_, UserRole> {
    async fn list_by_user(&self, user_id: &str, executor: &mut dyn Executor) -> Result<Vec<UserRole>> {
        self.find_many_sorted(doc! { "user_id": user_id }, doc! { "effective_from": -1 }, executor).await
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::PermissionId;
    use mongodb::bson;

    use crate::entity::access_control::{PermissionData, PermissionDefinition};
    // 验证 BSON 序列化与反序列化保留既有字段，不新增嵌套权限字段。
    #[test]
    fn entity_roundtrips_through_bson() {
        let permission = PermissionDefinition::new(
            PermissionId::new("perm-1"),
            PermissionData::new(" SalesOrder", "APPROVE", " 销售单审批 ").with_description(" 审批销售单 "),
        )
        .unwrap();
        let document = bson::serialize_to_document(&permission).unwrap();
        assert_eq!(document.get_str("resource").unwrap(), "salesorder");
        assert_eq!(document.get_str("action").unwrap(), "approve");
        assert!(!document.contains_key("permission"));

        let roundtrip: PermissionDefinition = bson::deserialize_from_document(document).unwrap();
        assert_eq!(roundtrip, permission);
    }
}
