//! 供应商列表查询编排。

use std::collections::HashMap;

use super::list_view::commercial_party_ids;
use crate::dto::supplier::{SortDir, SupplierListQuery, SupplierQualificationHealth};
use crate::entity::supplier::SupplierCommercialProfileRevision;
use crate::error::Result;
use crate::ports::PartyFactsPort;
use crate::repository::SupplierListSearchInput;

/// 供应商列表业务查询参数的仓储搜索输入组织（保留在 Service）。
///
/// DTO→仓储的健康映射经 [`SupplierQualificationHealth::to_repository_filter`]
/// 单一转换入口（erp-supplier-008），调用处只剩一句转换。
///
/// # 参数
/// * `query` - 已校验的供应商列表业务筛选条件
/// * `as_of` - 当前业务日字符串
/// * `keyword_party_ids` - 关键词命中的主体 ID，由 PartyFactsPort 预先解析
///
/// # 返回
/// 返回仓储侧列表事实束搜索输入。
pub(super) fn supplier_list_search_input(
    query: &SupplierListQuery,
    as_of: String,
    keyword_party_ids: Option<Vec<erp_core::ids::PartyId>>,
) -> SupplierListSearchInput {
    let qualification_health =
        query.qualification_health.map(SupplierQualificationHealth::to_repository_filter);
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
        authorized_scope: crate::repository::scope::SupplierReadScope::default(),
        maintainer_user_ids: query.owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
        business_org_unit_ids: None,
        capability_owner_user_ids: query
            .capability_owner_user_ids
            .as_ref()
            .map(|ids| ids.as_slice().to_vec())
            .unwrap_or_default(),
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
pub(super) async fn load_entity_names(
    party: &dyn PartyFactsPort,
    profiles: &[SupplierCommercialProfileRevision],
    executor: &mut dyn persistence_core::Executor,
) -> Result<HashMap<String, String>> {
    let ids = commercial_party_ids(profiles);
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    party.current_legal_names_by_party_ids(&ids, executor).await
}
