//! 供应商列表的一致授权快照；范围与业务版本跨页携带。

use application_core::{AuditActor, EmptyScopeVersion, OwnershipPage, ScopeVersion};
use erp_identity::AccountDirectoryService;
use erp_permissions::supplier;
use persistence_core::{Executor, Transactional};
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
    /// 分页结果与归属口径。
    #[serde(flatten)]
    pub data: OwnershipPage<SupplierView>,

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

    context: SupplierResolvedScope,
}

impl SupplierService {
    /// 在快照事务内执行供应商列表授权、筛选、分页和事实装配。后续页须携带当前范围版本；交付前重读版本并
    /// 复验凭证，范围变化、筛选非法或读库失败时不返回列表。
    pub async fn supplier_list(
        &self,
        params: &SupplierListParams,
        actor: &AuditActor,
    ) -> Result<SupplierListView> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;

        let service = self.clone();
        let actor = actor.clone();
        let expected_version = params.scope_version.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.list_stage(&query, &actor, expected_version.as_deref(), executor).await
                })
            })
            .await
    }

    // 用同一快照执行供应商分页。先读取并核对客户端版本，再不装配地重读范围，防止用已经变化的授权或业务范
    // 围交付结果；非快照执行器拒绝。
    async fn list_stage(
        &self,
        query: &SupplierListQuery,
        actor: &AuditActor,
        expected_version: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<SupplierListView> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("供应商列表必须使用快照事务".into()));
        }

        let snapshot = self.run_list_snapshot(query, actor, true, executor).await?;
        ensure_scope_version(expected_version, &snapshot.context.scope_version)?;

        let current = self.run_list_snapshot(query, actor, false, executor).await?;
        ensure_stable_snapshot(&snapshot.context.scope_version, &current.context.scope_version)?;
        Ok(to_list_view(snapshot))
    }

    // 在当前执行器编译列表权限、解析筛选并计算范围指纹。首次调用读取并装配分页；只为复验调用时不读取页内
    // 容，仅验证凭证。
    async fn run_list_snapshot(
        &self,
        query: &SupplierListQuery,
        actor: &AuditActor,
        hydrate: bool,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSnapshot> {
        let mut context = self.data_scope.resolve(actor, &supplier::LIST, executor).await?;
        context.ensure_current(executor)?;
        let input = build_list_input(query, &context, self.party.as_ref(), executor).await?;
        fingerprint_scope(&self.db, &mut context, &input, executor).await?;

        if !hydrate {
            context.validate(executor).await?;
            return Ok(SupplierSnapshot {
                items: Vec::new(),
                total: 0,
                page: input.page,
                page_size: input.page_size,
                context,
            });
        }

        let bundle = self.db.supplier().load_supplier_list_bundle(&input, executor).await?;
        hydrate_snapshot(HydrateArgs {
            bundle,
            party: self.party.as_ref(),
            accounts: &self.accounts,
            context,
            page: input.page,
            page_size: input.page_size,
            executor,
        })
        .await
    }
}

// 构造可被 HTTP 边界识别的范围变化冲突。
// 返回 `ConflictError`。
pub(super) fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}

// 后续页必须携带当前范围版本。
// 第一页，或后续页带有非空版本时返回 `Ok(())`。
// `page` 大于 1 且版本缺失或为空字符串时返回范围变化冲突。
pub(super) fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    ScopeVersion::new(version, EmptyScopeVersion::Missing)
        .require_for_page(page)
        .map_err(|error| data_scope_changed(error.detail()))
}

// 客户端回传的范围版本必须与当前快照一致。
// 未回传或与 `actual` 相同时返回 `Ok(())`。
// 回传值与 `actual` 不同时返回范围变化冲突。
// `expected`：客户端版本；`None` 表示未回传。
// `actual`：当前快照版本。
pub(super) fn ensure_scope_version(expected: Option<&str>, actual: &str) -> Result<()> {
    ScopeVersion::new(expected, EmptyScopeVersion::Provided)
        .verify(actual)
        .map_err(|error| data_scope_changed(error.detail()))
}

// 同一快照内两次读取的范围版本必须一致。
// 两次版本相同时返回 `Ok(())`。
// 版本不同时返回范围变化冲突。
pub(super) fn ensure_stable_snapshot(first: &str, second: &str) -> Result<()> {
    ScopeVersion::new(Some(first), EmptyScopeVersion::Provided)
        .verify(second)
        .map_err(|_| data_scope_changed("数据范围或供应商资料已变化，请刷新"))
}

fn to_list_view(snapshot: SupplierSnapshot) -> SupplierListView {
    SupplierListView {
        policy_version: snapshot.context.policy_version(),
        organization_version: snapshot.context.organization_version(),
        as_of: snapshot.context.as_of().as_utc().to_rfc3339(),
        scope_version: snapshot.context.scope_version,
        empty_reason: None,
        scope_summary: "供应商按岗位权限共享；维护人及能力负责人筛选为额外收窄",

        data: OwnershipPage {
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

// 依据名称关键词解析主体；业务日取当天。
async fn build_list_input(
    query: &SupplierListQuery,
    scope: &SupplierReadScope,
    party: &dyn crate::ports::PartyFactsPort,
    executor: &mut dyn persistence_core::Executor,
) -> Result<SupplierListSearchInput> {
    let keyword_party_ids = match query.keyword.as_deref() {
        Some(keyword) => Some(party.matching_current_party_ids_by_name(keyword, executor).await?),
        None => None,
    };
    Ok(supplier_list_search_input(
        query,
        erp_core::common::time::BusinessDate::today().to_string(),
        keyword_party_ids,
        scope.clone(),
    ))
}

// 用当前筛选下的身份版本重算范围指纹；超过一万条时拒绝。
async fn fingerprint_scope(
    db: &mongodb::Database,
    context: &mut SupplierResolvedScope,
    input: &SupplierListSearchInput,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let versions = db.supplier_accounts().query_versions(&account_filter(input), executor).await?;
    if versions.len() > 10_000 {
        return Err(Error::ValidationError("供应商查询超过上限，请收窄搜索或负责人条件".into()));
    }
    context.scope_version = fingerprint_versions(&context.scope_version, &versions);
    Ok(())
}

// 用确定性哈希计算供应商身份版本指纹并拼接到基线版本。
// `DefaultHasher` 跨进程不保证稳定（与 erp-customer-004 同类问题）；本函数使用 FNV-1a 64 位并固化字段顺
// 序，`query_versions` 已按 `id` 排序。
// 返回 `{base}:{hex指纹}` 格式的范围版本。
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
        page: input.page,
        page_size: input.page_size,
        sort_by: input.sort_by.clone(),
        sort_ascending: input.sort_ascending,
    }
}

struct HydrateArgs<'a> {
    bundle: crate::repository::SupplierListBundle,
    party: &'a dyn crate::ports::PartyFactsPort,
    accounts: &'a AccountDirectoryService,
    context: SupplierResolvedScope,
    page: u64,
    page_size: u32,
    executor: &'a mut dyn persistence_core::Executor,
}

// 在同一执行器内水合主体、维护人名称与能力资质，再装配当前页。
async fn hydrate_snapshot(args: HydrateArgs<'_>) -> Result<SupplierSnapshot> {
    let HydrateArgs { bundle, party, accounts, context, page, page_size, executor } = args;
    let total = bundle.page.total;

    let (parties, revisions) = party.list_with_current_revisions(&bundle.party_ids, executor).await?;
    let entity_names = load_entity_names(party, &bundle.profiles, executor).await?;
    let maintainer_ids = unique_ids(bundle.page.items.iter().map(|row| row.maintainer_user_id.clone()));
    let names = accounts.names(&maintainer_ids, executor).await?;

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

    context.validate(executor).await?;
    Ok(SupplierSnapshot { items, total, page, page_size, context })
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

    #[tokio::test]
    async fn list_rejects_non_snapshot_and_missing_binding_before_business_reads() {
        use std::sync::Arc;

        use erp_core::AccountKind;
        use erp_identity::testing::unbound_authorization;
        use mongodb::Client;
        use persistence_core::ReadExecutor;
        use persistence_core::testing::SnapshotExecutor;

        use crate::SupplierAccess;
        use crate::ports::EmptyPartyFacts;

        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("supplier_list_snapshot");
        let service = SupplierService::new(
            db.clone(),
            Arc::new(EmptyPartyFacts),
            SupplierAccess::new(db, unbound_authorization()),
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let query = serde_json::from_value::<SupplierListParams>(serde_json::json!({}))
            .unwrap()
            .normalized()
            .unwrap();

        let result = service.list_stage(&query, &actor, None, &mut ReadExecutor::default()).await;
        assert!(matches!(
            result,
            Err(Error::ValidationError(message)) if message == "供应商列表必须使用快照事务"
        ));

        let result = service.list_stage(&query, &actor, None, &mut SnapshotExecutor::default()).await;
        assert!(matches!(
            result,
            Err(Error::ValidationError(message)) if message == "缺少查询 Mapping 绑定"
        ));
    }

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
        }))
        .unwrap();
        let normalized = query.normalized().unwrap();
        assert_eq!(normalized.owner_user_ids.unwrap().as_slice(), &["buyer-a".to_string()]);
        assert_eq!(normalized.capability_owner_user_ids.unwrap().as_slice(), &["cap-b".to_string()]);

        assert!(serde_json::from_value::<SupplierListParams>(serde_json::json!({"owner": "张三"})).is_err());
    }
}
