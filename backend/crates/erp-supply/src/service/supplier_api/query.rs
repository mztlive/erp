//! 连接与能力本域分页，不读取供应商主数据或后台任务。
use persistence_core::NoTransaction;
use validator::Validate;

use super::SupplierApiService;
use crate::Result;
use crate::dto::supplier_api::*;
use crate::repository::SupplierApiExt;
use crate::repository::prelude::*;
type SupplierApiConnectionFilter = <mongodb::Database as SupplierApiExt>::SupplierApiConnectionFilter;
type SupplierApiCapabilityFilter = <mongodb::Database as SupplierApiExt>::SupplierApiCapabilityFilter;
impl SupplierApiService {
    /// 分页查询连接列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// * `keyword_supplier_ids` - 读取模型按关键词解析的完整供应商身份，与精确条件取交集
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn connection_list(
        &self,
        params: &SupplierApiConnectionListParams,
        keyword_supplier_ids: Vec<erp_core::ids::SupplierAccountId>,
    ) -> Result<PageView<SupplierApiConnectionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SupplierApiConnectionFilter {
            q: query.q,
            keyword_supplier_ids,
            supplier_id: query.supplier_id,
            connection_code: query.connection_code,
            environment: query.environment,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_api_connections()
            .search_supplier_api_connections(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierApiConnectionView {
                id: row.id,
                supplier_id: row.supplier_id,
                connection_code: row.connection_code,
                environment: row.environment,
                status: row.status,
                rate_limit_policy: None,
                last_health_at: row.last_health_at.map(|at| at as u64),
                last_health_result: row.last_health_result,
                safe_references: SafeReferencesView {
                    endpoint: SafeReferenceView {
                        state: if row.endpoint_reference_bound { "BOUND" } else { "MISSING" },
                        alias: None,
                        version: None,
                        visible: false,
                    },
                    credential: SafeReferenceView {
                        state: if row.credential_reference_bound { "BOUND" } else { "MISSING" },
                        alias: None,
                        version: None,
                        visible: false,
                    },
                },
                technical_config_version: row.technical_config_version,
                allowed_actions: Vec::new(),
                action_blockers: Vec::new(),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }
    /// 分页查询连接能力列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`connection_id`/`capability_code`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn capability_list(
        &self,
        params: &SupplierApiCapabilityListParams,
    ) -> Result<PageView<SupplierApiCapabilityView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SupplierApiCapabilityFilter {
            connection_id: query.connection_id,
            capability_code: query.capability_code,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_api_capabilities()
            .search_supplier_api_capabilities(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierApiCapabilityView {
                id: row.id,
                connection_id: row.connection_id,
                capability_code: row.capability_code,
                status: row.status,
                version: row.version,
                created_at: row.created_at,
                constraint_summary: None,
                business_requirement: None,
                business_confirmation_version: None,
                technically_verified: false,
                verified_at: None,
                allowed_actions: Vec::new(),
                action_blockers: Vec::new(),
            })
            .collect();

        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }
}
