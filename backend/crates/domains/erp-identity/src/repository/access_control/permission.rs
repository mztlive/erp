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

/// 权限目录的资源、状态筛选及分页参数。
#[derive(Debug, Clone, Default)]
pub struct PermissionFilter {
    /// 资源代码按不区分大小写的字面量匹配；未指定则不限制。
    pub resource: Option<String>,
    /// 精确筛选权限是否停用；未指定则不限制。
    pub disabled: Option<bool>,
    /// 精确筛选权限是否为系统内建；未指定则不限制。
    pub system: Option<bool>,
    /// 上层已校验的分页和排序参数。
    pub paging: PageParams,
}

impl QueryFilter for PermissionFilter {
    // 组合字面量资源条件和精确状态条件，同时排除已删除定义。
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
    // 将目录页码和页大小转换成通用分页接口需要的类型。
    fn page_and_size(&self) -> (u64, u64) {
        (self.paging.page, u64::from(self.paging.page_size))
    }
}

/// 为权限定义集合提供权限目录的投影查询。
#[allow(async_fn_in_trait)]
pub trait PermissionRepositoryExt {
    /// 用调用方执行器分页读取目录投影及同条件总数，不读取完整定义。
    /// 资源忽略大小写并按字面量匹配；停用和内建状态精确匹配，使用 `idx_permissions_disabled` 对应字段。查询、游标或计数错误直接返回。
    async fn search_permissions(
        &self,
        filter: &PermissionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PermissionView>>;
}

impl PermissionRepositoryExt for Repository<'_, PermissionDefinition> {
    // 在数据库按同一筛选读取 `PermissionView` 投影及匹配总数。
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

/// 为用户角色绑定集合提供管理端历史查询。
#[allow(async_fn_in_trait)]
pub trait UserRoleRepositoryExt {
    /// 用调用方执行器读取账号未删除的角色绑定，按生效时间倒序，保留撤权历史以区分当前、待生效和已失效关系。查询或游标错误直接返回。
    async fn list_by_user(&self, user_id: &str, executor: &mut dyn Executor) -> Result<Vec<UserRole>>;
}

impl UserRoleRepositoryExt for Repository<'_, UserRole> {
    // 按用户 ID 限定绑定记录并按生效时间倒序，不预先过滤撤权或过期状态。
    async fn list_by_user(&self, user_id: &str, executor: &mut dyn Executor) -> Result<Vec<UserRole>> {
        self.find_many_sorted(doc! { "user_id": user_id }, doc! { "effective_from": -1 }, executor).await
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::PermissionId;
    use mongodb::bson;

    use crate::entity::access_control::{PermissionData, PermissionDefinition};
    // 验证权限资源与动作按规范化的顶层 BSON 字段持久化，且可无损反序列化。
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
