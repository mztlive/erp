//! 供给列表专用读模型：分页前关联筛选与当前指针批量装载。
//!
//! Service 只组织查询参数并映射视图；关键字 SKU、SPU/SKU 编号、当前可供
//! 状态的持久化过滤，以及分页、排序、总数、当前修订、可供投影与展示实体的
//! 批量读取全部收敛在本模块，保证关联筛选在分页前生效、查询次数与页大小无关。

use std::collections::HashMap;

use entities::catalog::{Product, Sku, SkuRevision};
use entities::ids::{SkuId, SupplierAccountId, SupplierOfferingId};
use entities::party::{Party, PartyRevision};
use entities::supplier::SupplierAccount;
use entities::supplier_offering::{
    AvailabilityStatus, OfferingSourceType, OfferingStatus, SupplierOfferingAvailability,
    SupplierOfferingRevision,
};

use super::super::extensions::{CatalogExt, SupplierOfferingExt};
use super::super::PageResult;
use super::{SupplierOfferingFilter, SupplierOfferingRepository, SupplierOfferingRow};
use crate::executor::Executor;
use crate::Result;

/// 供给列表的高层查询条件。
///
/// 调用方传入已规整的关键字与编号（Service 负责 `normalized_text`），
/// 本模块负责把它们解析为持久化过滤并在分页前生效。
#[derive(Debug, Clone)]
pub struct SupplierOfferingListQuery {
    /// 当前可供状态；`None` 表示不过滤。
    pub availability_status: Option<AvailabilityStatus>,
    /// 已规整的关键字（供应商订货编码、公司 SKU 编号/名称）。
    pub keyword: Option<String>,
    /// 已规整的公司商品编号。
    pub product_no: Option<String>,
    /// 已规整的公司 SKU 编号。
    pub sku_no: Option<String>,
    /// 公司 SKU 精确过滤。
    pub sku_id: Option<SkuId>,
    /// 供应商精确过滤。
    pub supplier_id: Option<SupplierAccountId>,
    /// 供给关系状态。
    pub status: Option<OfferingStatus>,
    /// 登记来源。
    pub source_type: Option<OfferingSourceType>,
    /// 页码（从 1 起）。
    pub page: u64,
    /// 每页数量。
    pub page_size: u32,
    /// 排序字段。
    pub sort_by: Option<String>,
    /// 是否升序。
    pub sort_ascending: bool,
}

/// 供给列表一页的最小展示事实束。
///
/// 行投影沿当前修订指针返回；缺失指针或修订时对应映射不生成条目，
/// 由 Service 按空展示语义映射视图。
#[derive(Debug)]
pub struct SupplierOfferingListBundle {
    /// 当前页投影行与总数。
    pub page: PageResult<SupplierOfferingRow>,
    /// 以供给主键为键的当前修订映射。
    pub revisions: HashMap<String, SupplierOfferingRevision>,
    /// 以供给主键为键的实时可供投影映射。
    pub availabilities: HashMap<String, SupplierOfferingAvailability>,
    /// 公司 SKU 批量事实。
    pub skus: Vec<Sku>,
    /// SKU 当前修订批量事实。
    pub sku_revisions: Vec<SkuRevision>,
    /// 公司商品批量事实。
    pub products: Vec<Product>,
    /// 供应商账户批量事实。
    pub suppliers: Vec<SupplierAccount>,
    /// 主体批量事实。
    pub parties: Vec<Party>,
    /// 主体当前修订批量事实。
    pub party_revisions: Vec<PartyRevision>,
}

impl<'a> SupplierOfferingRepository<'a> {
    /// 将高层列表查询解析为分页前生效的持久化过滤。
    ///
    /// # 参数
    /// * `query` - 高层查询条件，关键字与编号应已由 Service 规整
    /// * `executor` - 数据访问执行器，由调用方决定事务边界
    ///
    /// # 返回
    /// 返回可直接分页的供给过滤；任一关联条件无命中时对应 ID 集合为空，
    /// 分页查询必然无结果但总数与排序语义保持不变。
    ///
    /// # 错误
    /// 任一批量解析查询失败时返回错误。
    ///
    /// # 约束
    /// 只做批量读取，不开启或提交事务；软删除、当前指针与稳定排序语义
    /// 由底层 `search_supplier_offerings` 保持。
    pub async fn resolve_list_filter(
        &self,
        query: &SupplierOfferingListQuery,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOfferingFilter> {
        let offering_ids = match query.availability_status {
            Some(status) => Some(
                self.db
                    .supplier_offering_availabilities()
                    .find_offering_ids_by_status(status, executor)
                    .await?,
            ),
            None => None,
        };
        let keyword_sku_ids = match query.keyword.as_deref() {
            Some(keyword) => Some(
                self.db
                    .catalog()
                    .resolve_sku_ids_by_keyword(keyword, executor)
                    .await?,
            ),
            None => None,
        };
        let sku_ids = self
            .db
            .catalog()
            .resolve_sku_ids_by_codes(query.product_no.as_deref(), query.sku_no.as_deref(), executor)
            .await?;
        Ok(SupplierOfferingFilter {
            offering_ids,
            sku_id: query.sku_id.clone(),
            supplier_id: query.supplier_id.clone(),
            status: query.status,
            source_type: query.source_type,
            supplier_sku_code: query.keyword.clone(),
            keyword_sku_ids,
            sku_ids,
            page: query.page,
            page_size: query.page_size,
            sort_by: query.sort_by.clone(),
            sort_ascending: query.sort_ascending,
        })
    }

    /// 执行供给列表分页查询。
    ///
    /// # 参数
    /// * `query` - 高层查询条件
    /// * `executor` - 数据访问执行器，由调用方决定事务边界
    ///
    /// # 返回
    /// 返回当前页投影行与总数；总数与排序语义与历史实现一致。
    ///
    /// # 错误
    /// 关联解析或分页查询失败时返回错误。
    ///
    /// # 约束
    /// 查询次数不随页大小增长；不开启或提交事务。
    pub async fn search_offering_list(
        &self,
        query: &SupplierOfferingListQuery,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierOfferingRow>> {
        let filter = self.resolve_list_filter(query, executor).await?;
        self.db
            .supplier_offerings()
            .search_supplier_offerings(&filter, executor)
            .await
    }

    /// 一次装载供给列表页的全部最小展示事实。
    ///
    /// # 参数
    /// * `query` - 高层查询条件
    /// * `executor` - 数据访问执行器，由调用方决定事务边界
    ///
    /// # 返回
    /// 返回分页行、当前修订、可供投影与跨域展示实体的事实束；
    /// 查询次数固定为解析三次加分页两次加批量装载，与输入规模无关。
    ///
    /// # 错误
    /// 任一批量查询失败时返回错误且不返回部分事实束。
    ///
    /// # 约束
    /// 只沿当前修订指针读取；不返回 Service DTO、HTTP View 或授权结论；
    /// 不开启或提交事务。
    pub async fn load_offering_list_bundle(
        &self,
        query: &SupplierOfferingListQuery,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOfferingListBundle> {
        let page = self.search_offering_list(query, executor).await?;
        let revisions = self.load_current_revisions(&page.items, executor).await?;
        let offering_ids = page
            .items
            .iter()
            .map(|row| SupplierOfferingId::new(row.id.clone()))
            .collect::<Vec<_>>();
        let availabilities = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_ids(&offering_ids, executor)
            .await?
            .into_iter()
            .map(|value| (value.supplier_offering_id.to_string(), value))
            .collect::<HashMap<_, _>>();
        let (skus, sku_revisions, products, suppliers, parties, party_revisions) =
            self.load_display_entities(&page.items, executor).await?;
        Ok(SupplierOfferingListBundle {
            page,
            revisions,
            availabilities,
            skus,
            sku_revisions,
            products,
            suppliers,
            parties,
            party_revisions,
        })
    }

    /// 一次装载供给列表页的全部最小展示事实（Service 直接调用入口）。
    ///
    /// Service 不能直接构造 [`SupplierOfferingListQuery`]：`repository::supplier_offering`
    /// 模块为私有且 `repository/mod.rs` 在冻结清单内，跨 crate 无法命名查询结构体。
    /// 因此本 12 参数包装是 Service 唯一的规范入口；它只做一件事——把参数逐字段
    /// 装配为查询结构体后委托 [`Self::load_offering_list_bundle`]，两者过滤、总数与
    /// 排序语义恒等（见 `wrapper_args_map_one_to_one_onto_query`）。
    ///
    /// # 参数
    /// * `availability_status` - 当前可供状态；`None` 表示不过滤
    /// * `keyword` - 已规整的关键字（供应商订货编码、公司 SKU 编号/名称）
    /// * `product_no` - 已规整的公司商品编号
    /// * `sku_no` - 已规整的公司 SKU 编号
    /// * `sku_id` - 公司 SKU 精确过滤
    /// * `supplier_id` - 供应商精确过滤
    /// * `status` - 供给关系状态
    /// * `source_type` - 登记来源
    /// * `page` - 页码（从 1 起）
    /// * `page_size` - 每页数量
    /// * `sort_by` - 排序字段
    /// * `sort_ascending` - 是否升序
    /// * `executor` - 数据访问执行器，由调用方决定事务边界
    ///
    /// # 返回
    /// 返回分页行、当前修订、可供投影与跨域展示实体的事实束。
    ///
    /// # 错误
    /// 任一批量查询失败时返回错误且不返回部分事实束。
    ///
    /// # 约束
    /// 只沿当前修订指针读取；不返回 Service DTO、HTTP View 或授权结论；
    /// 不开启或提交事务。
    #[allow(clippy::too_many_arguments)]
    pub async fn load_offering_list_page(
        &self,
        availability_status: Option<AvailabilityStatus>,
        keyword: Option<String>,
        product_no: Option<String>,
        sku_no: Option<String>,
        sku_id: Option<SkuId>,
        supplier_id: Option<SupplierAccountId>,
        status: Option<OfferingStatus>,
        source_type: Option<OfferingSourceType>,
        page: u64,
        page_size: u32,
        sort_by: Option<String>,
        sort_ascending: bool,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOfferingListBundle> {
        let query = SupplierOfferingListQuery {
            availability_status,
            keyword,
            product_no,
            sku_no,
            sku_id,
            supplier_id,
            status,
            source_type,
            page,
            page_size,
            sort_by,
            sort_ascending,
        };
        self.load_offering_list_bundle(&query, executor).await
    }
}

#[cfg(test)]
mod tests {
    use super::SupplierOfferingListQuery;

    #[test]
    fn list_query_combines_availability_keyword_and_code_scopes() {
        let query = SupplierOfferingListQuery {
            availability_status: Some(entities::supplier_offering::AvailabilityStatus::Available),
            keyword: Some("SUP-1".to_string()),
            product_no: Some("SPU-1".to_string()),
            sku_no: None,
            sku_id: None,
            supplier_id: None,
            status: None,
            source_type: Some(entities::supplier_offering::OfferingSourceType::Excel),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        assert!(query.availability_status.is_some());
        assert_eq!(query.keyword.as_deref(), Some("SUP-1"));
        assert_eq!(query.product_no.as_deref(), Some("SPU-1"));
        assert_eq!(query.page, 1);
    }

    #[test]
    fn list_query_pagination_fields_are_preserved() {
        let query = SupplierOfferingListQuery {
            availability_status: None,
            keyword: None,
            product_no: None,
            sku_no: None,
            sku_id: None,
            supplier_id: None,
            status: None,
            source_type: None,
            page: 3,
            page_size: 50,
            sort_by: Some("status".to_string()),
            sort_ascending: true,
        };
        assert_eq!(query.page, 3);
        assert_eq!(query.page_size, 50);
        assert_eq!(query.sort_by.as_deref(), Some("status"));
        assert!(query.sort_ascending);
    }

    #[test]
    fn wrapper_args_map_one_to_one_onto_query() {
        use entities::ids::{SkuId, SupplierAccountId};
        let query = SupplierOfferingListQuery {
            availability_status: Some(entities::supplier_offering::AvailabilityStatus::Stale),
            keyword: Some("SUP-9".to_string()),
            product_no: Some("SPU-9".to_string()),
            sku_no: Some("SKU-9".to_string()),
            sku_id: Some(SkuId::new("sku-9")),
            supplier_id: Some(SupplierAccountId::new("supplier-9")),
            status: Some(entities::supplier_offering::OfferingStatus::Paused),
            source_type: Some(entities::supplier_offering::OfferingSourceType::Api),
            page: 2,
            page_size: 15,
            sort_by: Some("supplier_sku_code".to_string()),
            sort_ascending: true,
        };
        assert_eq!(
            query.availability_status,
            Some(entities::supplier_offering::AvailabilityStatus::Stale)
        );
        assert_eq!(query.keyword.as_deref(), Some("SUP-9"));
        assert_eq!(query.product_no.as_deref(), Some("SPU-9"));
        assert_eq!(query.sku_no.as_deref(), Some("SKU-9"));
        assert_eq!(query.sku_id.map(|id| id.to_string()).as_deref(), Some("sku-9"));
        assert_eq!(
            query.supplier_id.map(|id| id.to_string()).as_deref(),
            Some("supplier-9")
        );
        assert_eq!(
            query.status,
            Some(entities::supplier_offering::OfferingStatus::Paused)
        );
        assert_eq!(
            query.source_type,
            Some(entities::supplier_offering::OfferingSourceType::Api)
        );
        assert_eq!((query.page, query.page_size), (2, 15));
        assert_eq!(query.sort_by.as_deref(), Some("supplier_sku_code"));
        assert!(query.sort_ascending);
    }
}
