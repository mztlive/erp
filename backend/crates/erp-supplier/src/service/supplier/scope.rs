//! 供应商列表的一致授权快照；范围与业务版本跨页携带。

use application_core::{AuditActor, FilterOption, FilteredPage};
use persistence_core::Transactional;
use serde::Serialize;
use validator::Validate;

use super::SupplierService;
use super::list::{load_entity_names, supplier_list_search_input};
use super::list_view::{SupplierViewAssembleInput, assemble_supplier_views};
use crate::dto::supplier::{SupplierListParams, SupplierListQuery, SupplierView};
use crate::error::{Error, Result};
use crate::ports::SupplierResolvedScope;
use crate::repository::prelude::*;
use crate::repository::scope::SupplierReadScope;
use crate::repository::{SupplierExt, SupplierListSearchInput};

/// 列表响应保持现有字段并声明独立的授权时点及版本。
#[derive(Serialize)]
pub struct SupplierListView {
    /// 分页结果、维护人候选与归属口径。
    #[serde(flatten)]
    pub data: FilteredPage<SupplierView>,
    /// 能力负责人候选，只收窄不授予可见权。
    pub capability_owner_options: Vec<FilterOption>,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`。
    pub empty_reason: Option<&'static str>,
    /// 当前供应商范围口径摘要。
    pub scope_summary: &'static str,
}

/// 同一事务内的列表快照。
struct SupplierSnapshot {
    items: Vec<SupplierView>,
    total: i64,
    page: u64,
    page_size: u32,
    owner_options: Vec<FilterOption>,
    capability_owner_options: Vec<FilterOption>,
    context: SupplierResolvedScope,
    no_scope: bool,
}

impl SupplierService {
    /// 分页查询授权范围内的供应商列表。
    ///
    /// # 参数
    /// * `params` - 原始查询
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的列表。
    ///
    /// # 错误
    /// 缺范围版本的后续页、范围变化、筛选非法或仓储失败时拒绝。
    pub async fn supplier_list(
        &self,
        params: &SupplierListParams,
        actor: &AuditActor,
    ) -> Result<SupplierListView> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self.list_snapshot(&query, actor).await?;
        ensure_scope_version(params.scope_version.as_deref(), &snapshot.context.scope_version)?;
        let current = self.list_version_snapshot(&query, actor).await?;
        ensure_stable_snapshot(&snapshot.context.scope_version, &current.context.scope_version)?;
        Ok(to_list_view(snapshot))
    }

    /// 授权、总数、候选与业务身份版本全部在同一个事务读取。
    async fn list_snapshot(&self, query: &SupplierListQuery, actor: &AuditActor) -> Result<SupplierSnapshot> {
        self.run_list_snapshot(query, actor, true).await
    }

    /// 轻量版本复核快照（erp-supplier-004）。
    ///
    /// 与 [`Self::list_snapshot`] 共用授权、过滤与版本指纹逻辑，
    /// 跳过列表束装配、候选与行水合；调用方仅用 `context.scope_version`
    /// 做稳定性比对，快照语义保持不变。
    async fn list_version_snapshot(
        &self,
        query: &SupplierListQuery,
        actor: &AuditActor,
    ) -> Result<SupplierSnapshot> {
        self.run_list_snapshot(query, actor, false).await
    }

    /// 执行列表事务体（erp-supplier-004）。
    ///
    /// `hydrate` 为假时跳过列表束、水合与候选装配，仅重算版本指纹供复核使用。
    async fn run_list_snapshot(
        &self,
        query: &SupplierListQuery,
        actor: &AuditActor,
        hydrate: bool,
    ) -> Result<SupplierSnapshot> {
        let db = self.db.clone();
        let access = self.access();
        let data_scope = self.data_scope.clone();
        let party = self.party.clone();
        let accounts = self.accounts.clone();
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (mut context, scope) = access.resolve(&actor, "list", executor).await?;
                    let no_scope = !context.has_scope_rules();
                    let input =
                        build_list_input(&query, &scope, data_scope.as_ref(), party.as_ref(), executor)
                            .await?;
                    fingerprint_scope(&db, &mut context, &input, executor).await?;
                    if !hydrate {
                        return Ok(SupplierSnapshot {
                            items: Vec::new(),
                            total: 0,
                            page: input.page,
                            page_size: input.page_size,
                            owner_options: Vec::new(),
                            capability_owner_options: Vec::new(),
                            context,
                            no_scope,
                        });
                    }
                    let bundle = db.supplier().load_supplier_list_bundle(&input, executor).await?;
                    hydrate_snapshot(HydrateArgs {
                        bundle,
                        party: party.as_ref(),
                        accounts: accounts.as_ref(),
                        context,
                        no_scope,
                        page: input.page,
                        page_size: input.page_size,
                        executor,
                    })
                    .await
                })
            })
            .await
    }
}

/// 构造可被 HTTP 边界识别的范围变化冲突。
pub(super) fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}

/// 后续页必须携带当前范围版本。
pub(super) fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    if page > 1 && version.is_none_or(str::is_empty) {
        return Err(data_scope_changed("请从第一页刷新后继续查询"));
    }
    Ok(())
}

/// 客户端回传的范围版本必须与当前快照一致。
pub(super) fn ensure_scope_version(expected: Option<&str>, actual: &str) -> Result<()> {
    if expected.is_some_and(|value| value != actual) {
        return Err(data_scope_changed("数据范围已变化，请从第一页刷新"));
    }
    Ok(())
}

/// 同一查询内两次快照的范围版本必须一致。
pub(super) fn ensure_stable_snapshot(first: &str, second: &str) -> Result<()> {
    if first != second {
        return Err(data_scope_changed("数据范围或供应商资料已变化，请刷新"));
    }
    Ok(())
}

fn to_list_view(snapshot: SupplierSnapshot) -> SupplierListView {
    SupplierListView {
        scope_version: snapshot.context.scope_version,
        policy_version: snapshot.context.policy_version,
        organization_version: snapshot.context.organization_version,
        as_of: snapshot.context.as_of.as_utc().to_rfc3339(),
        empty_reason: snapshot.no_scope.then_some("no_scope"),
        scope_summary: "供应商整体维护人及其业务组织范围",
        capability_owner_options: snapshot.capability_owner_options,
        data: FilteredPage {
            owner_options: snapshot.owner_options,
            ownership_basis: "supplier_maintainer",
            page: application_core::PageView {
                items: snapshot.items,
                total: snapshot.total,
                page: snapshot.page,
                page_size: snapshot.page_size,
            },
        },
    }
}

async fn build_list_input(
    query: &SupplierListQuery,
    scope: &SupplierReadScope,
    data_scope: &dyn crate::ports::SupplierDataScopePort,
    party: &dyn crate::ports::PartyFactsPort,
    executor: &mut dyn persistence_core::Executor,
) -> Result<SupplierListSearchInput> {
    let org_ids = expand_org_filter(
        data_scope,
        query.org_unit_ids.as_ref().map(|ids| ids.as_slice()),
        query.include_descendants,
        executor,
    )
    .await?;
    let keyword_party_ids = match query.keyword.as_deref() {
        Some(keyword) => Some(party.matching_current_party_ids_by_name(keyword, executor).await?),
        None => None,
    };
    let mut input = supplier_list_search_input(
        query,
        erp_core::common::time::BusinessDate::today().to_string(),
        keyword_party_ids,
    );
    input.authorized_scope = scope.clone();
    input.business_org_unit_ids = org_ids;
    Ok(input)
}

async fn fingerprint_scope(
    db: &mongodb::Database,
    context: &mut SupplierResolvedScope,
    input: &SupplierListSearchInput,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let versions = db.supplier_accounts().query_versions(&account_filter(input), executor).await?;
    if versions.len() > 10_000 {
        return Err(Error::ValidationError("供应商查询超过上限，请收窄组织或负责人条件".into()));
    }
    context.scope_version = fingerprint_versions(&context.scope_version, &versions);
    Ok(())
}

/// 用确定性哈希计算供应商身份版本指纹并拼接到基线版本。
///
/// `DefaultHasher` 跨进程不保证稳定（与 erp-customer-004 同类问题）；
/// 本函数使用 FNV-1a 64 位并固化字段顺序，`query_versions` 已按 `id` 排序。
///
/// # 参数
/// * `base` - 基线范围版本
/// * `versions` - 供应商身份与版本集合
///
/// # 返回
/// 返回 `{base}:{hex指纹}` 格式的范围版本。
fn fingerprint_versions(base: &str, versions: &[crate::repository::SupplierVersion]) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for version in versions {
        for byte in version.id.as_bytes().iter().chain(&version.version.to_le_bytes()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{base}:{hash:016x}")
}

fn account_filter(input: &SupplierListSearchInput) -> crate::repository::SupplierAccountFilter {
    crate::repository::SupplierAccountFilter {
        keyword: input.keyword.clone(),
        party_id: input.party_id.clone(),
        party_ids: input.keyword_party_ids.clone(),
        status: input.status,
        supplier_ids: None,
        excluded_supplier_ids: None,
        authorized_scope: input.authorized_scope.clone(),
        maintainer_user_ids: input.maintainer_user_ids.clone(),
        business_org_unit_ids: input.business_org_unit_ids.clone(),
        page: input.page,
        page_size: input.page_size,
        sort_by: input.sort_by.clone(),
        sort_ascending: input.sort_ascending,
    }
}

struct HydrateArgs<'a> {
    bundle: crate::repository::SupplierListBundle,
    party: &'a dyn crate::ports::PartyFactsPort,
    accounts: &'a dyn crate::ports::AccountFactPort,
    context: SupplierResolvedScope,
    no_scope: bool,
    page: u64,
    page_size: u32,
    executor: &'a mut dyn persistence_core::Executor,
}

async fn hydrate_snapshot(args: HydrateArgs<'_>) -> Result<SupplierSnapshot> {
    let HydrateArgs { bundle, party, accounts, context, no_scope, page, page_size, executor } = args;
    let total = bundle.page.total;
    let (parties, revisions) = party.list_with_current_revisions(&bundle.party_ids, executor).await?;
    let entity_names = load_entity_names(party, &bundle.profiles, executor).await?;
    let maintainer_ids = unique_ids(bundle.page.items.iter().map(|row| row.maintainer_user_id.clone()));
    let capability_owners = unique_ids(bundle.capabilities.iter().map(|item| item.owner_user_id.clone()));
    let owner_options = accounts.filter_options(&maintainer_ids).await?;
    let capability_owner_options = accounts.filter_options(&capability_owners).await?;
    let names = accounts.names_by_ids(&maintainer_ids).await?;
    let items = assemble_supplier_views(SupplierViewAssembleInput {
        rows: bundle.page.items,
        parties,
        revisions,
        profiles: bundle.profiles,
        capabilities: bundle.capabilities,
        qualifications: bundle.qualifications,
        entity_names,
        maintainer_names: names,
        as_of: erp_core::common::time::BusinessDate::today(),
    });
    Ok(SupplierSnapshot {
        no_scope,
        items,
        total,
        page,
        page_size,
        owner_options,
        capability_owner_options,
        context,
    })
}

async fn expand_org_filter(
    data_scope: &dyn crate::ports::SupplierDataScopePort,
    org_ids: Option<&[String]>,
    include_descendants: Option<bool>,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<Vec<String>>> {
    let Some(org_ids) = org_ids else {
        if include_descendants == Some(true) {
            return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        return Ok(None);
    };
    if include_descendants == Some(true) && org_ids.is_empty() {
        return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
    }
    let expanded =
        data_scope.expand_org_units(org_ids, include_descendants.unwrap_or(false), executor).await?;
    Ok(Some(expanded.into_iter().collect()))
}

fn unique_ids(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut values: Vec<String> = ids.into_iter().filter(|id| !id.is_empty()).collect();
    values.sort();
    values.dedup();
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_pages_without_scope_version_are_rejected() {
        assert!(ensure_page(1, None).is_ok());
        match ensure_page(2, None) {
            Err(Error::ConflictError(message)) => assert!(message.starts_with("DATA_SCOPE_CHANGED：")),
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        assert!(ensure_scope_version(Some("v1"), "v1").is_ok());
        assert!(ensure_scope_version(Some("v1"), "v2").is_err());
        assert!(ensure_stable_snapshot("v1", "v1").is_ok());
        assert!(ensure_stable_snapshot("v1", "v2").is_err());
    }

    #[test]
    fn owner_and_capability_filters_stay_independent() {
        let query: SupplierListParams = serde_json::from_value(serde_json::json!({
            "owner_user_ids": "buyer-a",
            "capability_owner_user_ids": "cap-b",
            "org_unit_ids": "org-1"
        }))
        .unwrap();
        let normalized = query.normalized().unwrap();
        assert_eq!(normalized.owner_user_ids.unwrap().as_slice(), &["buyer-a".to_string()]);
        assert_eq!(normalized.capability_owner_user_ids.unwrap().as_slice(), &["cap-b".to_string()]);
        assert_eq!(normalized.org_unit_ids.unwrap().as_slice(), &["org-1".to_string()]);
        assert!(serde_json::from_value::<SupplierListParams>(serde_json::json!({"owner": "张三"})).is_err());
    }
}
