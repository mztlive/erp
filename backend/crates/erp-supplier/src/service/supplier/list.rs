//! 供应商列表查询编排。

use std::collections::HashMap;

use erp_core::common::time::BusinessDate;
use persistence_core::NoTransaction;
use validator::Validate;

use super::SupplierService;
use super::list_view::{SupplierViewAssembleInput, assemble_supplier_views, commercial_party_ids};
use crate::dto::supplier::{
    PageView, SortDir, SupplierListParams, SupplierListQuery, SupplierQualificationHealth, SupplierView,
};
use crate::entity::supplier::SupplierCommercialProfileRevision;
use crate::error::Result;
use crate::ports::PartyFactsPort;
use crate::repository::{SupplierExt, SupplierListSearchInput};

impl SupplierService {
    /// 分页查询供应商角色列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn supplier_list(&self, params: &SupplierListParams) -> Result<PageView<SupplierView>> {
        params.validate()?;
        let query = params.normalized()?;
        let as_of = BusinessDate::today();
        let keyword_party_ids = match query.keyword.as_deref() {
            Some(keyword) => {
                Some(self.party.matching_current_party_ids_by_name(keyword, &mut NoTransaction).await?)
            },
            None => None,
        };
        let input = supplier_list_search_input(&query, as_of.to_string(), keyword_party_ids);
        let bundle = self.db.supplier().load_supplier_list_bundle(&input, &mut NoTransaction).await?;
        let total = bundle.page.total;
        let (parties, revisions) =
            self.party.list_with_current_revisions(&bundle.party_ids, &mut NoTransaction).await?;
        let entity_names = load_entity_names(self.party.as_ref(), &bundle.profiles).await?;
        let items = assemble_supplier_views(SupplierViewAssembleInput {
            rows: bundle.page.items,
            parties,
            revisions,
            profiles: bundle.profiles,
            capabilities: bundle.capabilities,
            qualifications: bundle.qualifications,
            entity_names,
            as_of,
        });

        Ok(PageView { items, total, page: input.page, page_size: input.page_size })
    }
}

/// 供应商列表业务查询参数的仓储搜索输入组织（保留在 Service）。
///
/// # 参数
/// * `query` - 已校验的供应商列表业务筛选条件
/// * `as_of` - 当前业务日字符串
/// * `keyword_party_ids` - 关键词命中的主体 ID，由 PartyFactsPort 预先解析
///
/// # 返回
/// 返回仓储侧列表事实束搜索输入。
fn supplier_list_search_input(
    query: &SupplierListQuery,
    as_of: String,
    keyword_party_ids: Option<Vec<erp_core::ids::PartyId>>,
) -> SupplierListSearchInput {
    type HealthFilter = crate::repository::SupplierQualificationHealthFilter;
    let qualification_health = match query.qualification_health {
        None => None,
        Some(SupplierQualificationHealth::Unverified) => Some(HealthFilter::Unverified),
        Some(SupplierQualificationHealth::Valid) => Some(HealthFilter::Valid),
        Some(SupplierQualificationHealth::Expiring30) => Some(HealthFilter::Expiring30),
        Some(SupplierQualificationHealth::Expired) => Some(HealthFilter::Expired),
        Some(SupplierQualificationHealth::NotRegistered) => Some(HealthFilter::NotRegistered),
    };
    SupplierListSearchInput {
        keyword: query.keyword.clone(),
        party_id: query.party_id.clone(),
        keyword_party_ids,
        status: query.status,
        capability_codes: query.capability_codes.clone(),
        qualification_types: query.qualification_types.clone(),
        qualification_health,
        as_of,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

/// 批量读取当前页签约/付款主体的法定名称。
///
/// # 参数
/// * `party` - 主体只读事实端口
/// * `profiles` - 当前页商务资料
///
/// # 返回
/// 返回主体 ID 到法定名称的映射；无主体引用时不访问端口。
///
/// # 错误
/// 主体查询失败时返回仓储或映射错误。
async fn load_entity_names(
    party: &dyn PartyFactsPort,
    profiles: &[SupplierCommercialProfileRevision],
) -> Result<HashMap<String, String>> {
    let ids = commercial_party_ids(profiles);
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    party.current_legal_names_by_party_ids(&ids, &mut NoTransaction).await
}
