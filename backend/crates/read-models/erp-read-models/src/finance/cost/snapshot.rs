//! 授权、成本、分配和销售责任事实在同一事务读取，交付前独立事务重验。
mod versions;

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::hash::{Hash, Hasher};

use erp_finance::dto::cost::CostAllocationListQuery;
use erp_finance::entity::cost::CostAllocation;
use erp_finance::entity::read_coverage::whole_document_readable;
use erp_finance::repository::CostExt;
use erp_finance::repository::cost::read_scope::{ALLOCATION_LIMIT, ENTRY_LIMIT};
use erp_finance::repository::cost::{CostEntryFilter, CostEntryRow};
use erp_finance::repository::prelude::*;
use erp_finance::service::cost::{cost_entry_filter, cost_entry_row_view};
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_sales::repository::sales_order::scope::SalesReadScope;
use persistence_core::{Executor, Transactional};
use versions::VersionFacts;

use super::*;
use crate::finance::funds_scope::ledger_readable;
use crate::sales_center::access::sales_scope;

#[derive(Clone, Default)]
pub(super) struct CostSelection {
    id: Option<String>,
    allocations: Option<CostAllocationListQuery>,
}

impl CostSelection {
    /// 独立详情沿原成本身份约束读取。
    ///
    /// # 参数
    /// * `id` - 成本身份。
    /// # 返回
    /// 返回仅按该身份筛选的读取选择。
    /// # 错误
    /// 无；构造不执行 I/O。
    pub fn entry(id: &str) -> Self {
        Self { id: Some(id.to_owned()), allocations: None }
    }

    /// 分配联合条件用于缩小金额投影；版本仍覆盖原完整成本范围。
    ///
    /// # 参数
    /// * `query` - 已规范化的成本分配条件。
    /// # 返回
    /// 返回包含原成本身份条件和联合分配条件的读取选择。
    /// # 错误
    /// 无；构造不执行 I/O。
    pub fn allocations(query: CostAllocationListQuery) -> Self {
        Self { id: query.cost_entry_id.as_ref().map(ToString::to_string), allocations: Some(query) }
    }
}

pub(super) struct Snapshot {
    pub rows: Vec<ScopedCostEntryView>,
    pub allocation_created_at: HashMap<String, u64>,
    no_scope: bool,
    version: String,
    policy: u64,
    organization: u64,
    as_of: String,
}

struct CostAuthorization {
    sales: SalesReadScope,
    ledger_read: bool,
    context: AuthorizedDataScope,
    fingerprint: DefaultHasher,
}
impl CostAuthorization {
    /// 无销售来源范围且无整账职责时无需访问业务集合。
    fn empty(&self) -> bool {
        !self.ledger_read && self.sales.is_empty()
    }

    /// 部分来源范围禁止探测整笔来源引用，初读与返回前重验使用相同规则。
    fn require_filter(&self, filter: &CostEntryFilter) -> Result<()> {
        if !self.empty() && !self.ledger_read && filter.source_document_id.is_some() {
            return Err(Error::ValidationError("当前只能读取成本分配，不能按整笔来源单据筛选".into()));
        }
        Ok(())
    }
}
impl Snapshot {
    /// 记录授权元信息，金额和范围指纹由同事务读取完成。
    fn new(access: &CostAuthorization) -> Self {
        Self {
            rows: vec![],
            allocation_created_at: HashMap::new(),
            no_scope: access.empty(),
            version: String::new(),
            policy: access.context.policy_version,
            organization: access.context.organizations.version,
            as_of: access.context.as_of.as_utc().to_rfc3339(),
        }
    }
    /// 为当前结果附加已经复验的范围凭据。
    ///
    /// # 参数
    /// * `data` - 首次快照生成的当前页面或详情。
    /// # 返回
    /// 返回数据与范围元信息。
    /// # 错误
    /// 无；版本复验由调用方在交付前完成。
    pub fn result<T>(self, data: T) -> CostReadResult<T> {
        CostReadResult {
            data,
            scope_version: self.version,
            policy_version: self.policy,
            organization_version: self.organization,
            as_of: self.as_of,
            empty_reason: self.no_scope.then_some("no_scope"),
            scope_summary: "按关联销售范围读取成本份额；整笔及未分配成本另需财务整账读取资格",
            ownership_basis: "current_sales_allocation",
        }
    }
    /// 在分页前裁剪完整候选；保留分配本身的创建时间，不能用成本创建时间替代。
    fn project(
        &mut self,
        candidates: Vec<CostEntryRow>,
        allocations: Vec<CostAllocation>,
        orders: &BTreeSet<String>,
        ledger_read: bool,
    ) -> Result<()> {
        let mut grouped = HashMap::<String, Vec<_>>::new();
        for line in allocations {
            self.allocation_created_at.insert(line.base.id.clone(), line.base.created_at);
            grouped.entry(line.cost_entry_id.to_string()).or_default().push(line);
        }
        let visible_sources = orders.iter().cloned().collect::<Vec<_>>();
        for row in candidates {
            let allocations = grouped.remove(&row.id).unwrap_or_default();
            let whole = whole_document_readable(
                ledger_read,
                allocations
                    .iter()
                    .filter_map(|line| line.sales_order_id.as_ref())
                    .map(|id| Some(id.as_ref())),
                &visible_sources,
            );
            if let Some(view) = cost_entry_row_view(row, allocations).restrict(whole, orders)? {
                self.rows.push(view);
            }
        }
        Ok(())
    }
}
impl CostReadModel {
    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    ///
    /// # 参数
    /// 原业务条件、读取选择、资源动作、操作人及可选跨页版本。
    /// # 返回
    /// 返回轻量版本重验通过的首次金额快照。
    /// # 错误
    /// 无权限、范围变化、非法参数、候选超限或持久化失败时拒绝。
    pub(super) async fn checked(
        &self,
        params: &CostEntryListParams,
        selection: &CostSelection,
        resource: &str,
        action: &str,
        actor: &AuditActor,
        expected: Option<&str>,
    ) -> Result<Snapshot> {
        let snapshot = self.snapshot(params, selection, resource, action, actor).await?;
        checked_snapshot(snapshot, expected, || {
            self.current_version(params, selection, resource, action, actor)
        })
        .await
    }
    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    async fn snapshot(
        &self,
        params: &CostEntryListParams,
        selection: &CostSelection,
        resource: &str,
        action: &str,
        actor: &AuditActor,
    ) -> Result<Snapshot> {
        let filter = cost_entry_filter(params)?;
        let this = self.clone();
        let actor = actor.clone();
        let selection = selection.clone();
        let resource = resource.to_owned();
        let action = action.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = this.authorize(&actor, &resource, &action, executor).await?;
                    this.load_snapshot(&filter, &selection, access, executor).await
                })
            })
            .await
    }
    /// 证明目标成本动作并继承销售来源边界；整账职责独立重验。
    async fn authorize(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<CostAuthorization> {
        let resolver = DataScopeService::new(self.db.clone(), self.rbac.clone());
        let context =
            resolver.resolve_source_scope(actor, resource, action, "sales_order", "list", executor).await?;
        let sales = sales_scope(&context, actor.id(), &[], Vec::new());
        let ledger_read = ledger_readable(&self.db, &self.rbac, actor, executor).await?;
        let mut fingerprint = DefaultHasher::new();
        context.scope_version.hash(&mut fingerprint);
        ledger_read.hash(&mut fingerprint);
        Ok(CostAuthorization { sales, ledger_read, context, fingerprint })
    }
    /// 隐藏来源条件拒绝，空范围保持空集；完整装载并裁剪后才允许分页。
    async fn load_snapshot(
        &self,
        filter: &CostEntryFilter,
        selection: &CostSelection,
        mut access: CostAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Snapshot> {
        let mut snapshot = Snapshot::new(&access);
        access.require_filter(filter)?;
        if !access.empty() {
            let (candidates, allocations, versions) =
                self.candidate_facts(filter, selection, executor).await?;
            let orders = self.authorized_orders(&versions, &mut access, executor).await?;
            versions.hash(&mut access.fingerprint);
            snapshot.project(candidates, allocations, &orders, access.ledger_read)?;
        }
        snapshot.version = format!("{:x}", access.fingerprint.finish());
        Ok(snapshot)
    }
    /// 候选成本超过上限必须整体拒绝，不把截断集合当成完整统计。
    async fn cost_candidates(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntryRow>> {
        let candidates = self.db.cost_entries().scope_candidates(filter, id, executor).await?;
        if candidates.len() > ENTRY_LIMIT {
            return Err(Error::ValidationError("成本查询超过上限，请指定供应商或成本类型".into()));
        }
        Ok(candidates)
    }

    /// 分配只给匹配成本装载金额；完整原候选仍以轻量事实绑定范围版本。
    async fn candidate_facts(
        &self,
        filter: &CostEntryFilter,
        selection: &CostSelection,
        executor: &mut dyn Executor,
    ) -> Result<(Vec<CostEntryRow>, Vec<CostAllocation>, VersionFacts)> {
        let Some(query) = &selection.allocations else {
            let entries = self.cost_candidates(filter, selection.id.as_deref(), executor).await?;
            let allocations = self.candidate_allocations(&entries, executor).await?;
            let versions = VersionFacts::from_rows(&entries, &allocations);
            return Ok((entries, allocations, versions));
        };
        let versions = self.candidate_versions(filter, selection.id.as_deref(), executor).await?;
        let ids = versions.entries.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let matching = self
            .db
            .cost_allocations()
            .scope_matching_allocations(
                query.cost_entry_id.as_ref().map(|id| id.as_ref()),
                query.sales_order_id.as_ref().map(|id| id.as_ref()),
                &ids,
                executor,
            )
            .await?;
        let matched_ids = matching
            .into_iter()
            .map(|row| row.cost_entry_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut entries = Vec::new();
        for chunk in matched_ids.chunks(500) {
            entries.extend(self.db.cost_entries().scope_entries_by_ids(filter, chunk, executor).await?);
        }
        let allocations = self.candidate_allocations(&entries, executor).await?;
        Ok((entries, allocations, versions))
    }
    /// 批量装载全部分配并跨批次累计上限，避免部分结果漏掉资金份额。
    async fn candidate_allocations(
        &self,
        candidates: &[CostEntryRow],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocation>> {
        let ids = candidates.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let mut allocations = Vec::new();
        for chunk in ids.chunks(500) {
            allocations.extend(self.db.cost_allocations().scope_allocations(chunk, executor).await?);
            if allocations.len() > ALLOCATION_LIMIT {
                return Err(Error::ValidationError("成本分配超过查询上限，请收窄业务条件".into()));
            }
        }
        Ok(allocations)
    }
}
/// 先验证跨页凭据再启动轻量重验；成功只交付首次金额快照。
async fn checked_snapshot<F, Fut>(snapshot: Snapshot, expected: Option<&str>, current: F) -> Result<Snapshot>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<String>>,
{
    if expected.is_some_and(|value| value != snapshot.version) {
        return Err(changed());
    }
    if current().await? != snapshot.version {
        return Err(changed());
    }
    Ok(snapshot)
}

/// 保持成本范围变化的既有冲突分类及提示。
fn changed() -> Error {
    crate::support::data_scope_changed("数据范围已变化，请从第一页刷新")
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{CostEntryId, SalesOrderId};
    use erp_core::money::Amount;
    use erp_finance::dto::cost::{PageParams, SortDir};
    use erp_finance::entity::cost::{CostScope, CostStage, CostType};

    use super::*;

    pub(super) fn entry(id: &str) -> CostEntryRow {
        CostEntryRow {
            id: id.into(),
            cost_type: CostType::Other,
            cost_stage: CostStage::Actual,
            cost_scope: CostScope::NonVoucherFulfillment,
            cost_basis: None,
            supplier_id: None,
            gross_amount: "100".parse().unwrap(),
            net_amount: "100".parse().unwrap(),
            tax_amount: Amount::zero(),
            tax_inclusion: false,
            input_tax_rate: "0".parse().unwrap(),
            occurred_at: Instant::from_unix_secs(1),
            source_fact_type: "manual".into(),
            source_document_id: "private-source".into(),
            source_line_id: "private-line".into(),
            source_version: "1".into(),
            version: 1,
            created_at: 1,
        }
    }

    pub(super) fn allocation(id: &str, sales: &str, amount: &str) -> CostAllocation {
        let mut base = BaseModel::fake();
        base.id = id.into();
        base.created_at = if id == "a" { 20 } else { 10 };
        CostAllocation {
            base,
            cost_entry_id: CostEntryId::new("cost-a"),
            sales_order_id: Some(SalesOrderId::new(sales)),
            sales_order_line_id: None,
            allocated_gross_amount: amount.parse().unwrap(),
            allocated_net_amount: amount.parse().unwrap(),
            rounding_residual_flag: false,
        }
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            rows: vec![],
            allocation_created_at: HashMap::new(),
            no_scope: false,
            version: String::new(),
            policy: 1,
            organization: 1,
            as_of: String::new(),
        }
    }

    #[test]
    fn matched_cost_keeps_sibling_allocations_for_partial_and_whole_readability() {
        let lines = vec![allocation("a", "sales-a", "60"), allocation("b", "sales-b", "40")];
        let orders = BTreeSet::from(["sales-a".into()]);
        let mut partial = snapshot();
        partial.project(vec![entry("cost-a")], lines.clone(), &orders, true).unwrap();
        assert_eq!(partial.rows.len(), 1);
        assert_eq!(partial.rows[0].scope_net_amount, "60".parse().unwrap());
        assert!(!partial.rows[0].whole_document_access);
        assert!(partial.rows[0].net_amount.is_none());
        assert!(partial.rows[0].source_document_id.is_none());
        assert_eq!(partial.allocation_created_at["a"], 20);

        let orders = BTreeSet::from(["sales-a".into(), "sales-b".into()]);
        let mut whole = snapshot();
        whole.project(vec![entry("cost-a")], lines, &orders, true).unwrap();
        assert!(whole.rows[0].whole_document_access);
        assert_eq!(whole.rows[0].scope_net_amount, "100".parse().unwrap());
        assert_eq!(whole.rows[0].allocations.len(), 2);
    }

    #[test]
    fn no_scope_and_unallocated_cost_keep_explicit_ledger_requirement() {
        let mut denied = snapshot();
        denied.project(vec![entry("unallocated")], vec![], &BTreeSet::new(), false).unwrap();
        assert!(denied.rows.is_empty());
        let mut allowed = snapshot();
        allowed.project(vec![entry("unallocated")], vec![], &BTreeSet::new(), true).unwrap();
        assert_eq!(allowed.rows.len(), 1);
        assert!(allowed.rows[0].whole_document_access);
        assert_eq!(allowed.rows[0].scope_net_amount, Amount::zero());
        assert_eq!(allowed.rows[0].net_amount, Some("100".parse().unwrap()));
    }

    #[test]
    fn narrowed_cost_projection_keeps_allocation_filter_and_persisted_sort_results() {
        let mut projected = snapshot();
        let lines = vec![allocation("a", "sales-a", "60"), allocation("b", "sales-b", "40")];
        projected.project(vec![entry("cost-a")], lines, &BTreeSet::from(["sales-a".into()]), false).unwrap();
        let query = CostAllocationListQuery {
            sales_order_id: Some(SalesOrderId::new("sales-a")),
            cost_entry_id: Some(CostEntryId::new("cost-a")),
            paging: PageParams { page: 1, page_size: 1, sort_by: "created_at", sort_dir: SortDir::Asc },
        };
        let rows = projected.rows.into_iter().flat_map(|row| row.allocations).collect();
        let page = paging::allocations(rows, &projected.allocation_created_at, &query).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "a");
        assert_eq!(page.items[0].allocated_net_amount, "60".parse().unwrap());
    }

    #[tokio::test]
    async fn expected_version_conflict_skips_revalidation_and_second_change_is_rejected() {
        let calls = Cell::new(0);
        let mut first = snapshot();
        first.version = "v2".into();
        let result = checked_snapshot(first, Some("v1"), || {
            calls.set(calls.get() + 1);
            async { Ok("v2".into()) }
        })
        .await;
        assert!(matches!(result, Err(Error::ConflictError(_))));
        assert_eq!(calls.get(), 0);
        let mut first = snapshot();
        first.version = "v1".into();
        let result = checked_snapshot(first, Some("v1"), || async { Ok("v2".into()) }).await;
        assert!(matches!(result, Err(Error::ConflictError(_))));
    }

    #[tokio::test]
    async fn lightweight_revalidation_returns_first_amount_projection_and_propagates_failure() {
        let mut first = snapshot();
        first.version = "v1".into();
        first
            .project(
                vec![entry("cost-a")],
                vec![allocation("a", "sales-a", "60")],
                &BTreeSet::from(["sales-a".into()]),
                false,
            )
            .unwrap();
        let result = checked_snapshot(first, Some("v1"), || async { Ok("v1".into()) }).await.unwrap();
        assert_eq!(result.rows[0].scope_net_amount, "60".parse().unwrap());
        assert_eq!(result.version, "v1");
        let failure = checked_snapshot(snapshot(), None, || async {
            Err(Error::ValidationError("authorization unavailable".into()))
        })
        .await;
        assert!(
            matches!(failure, Err(Error::ValidationError(message)) if message == "authorization unavailable")
        );
    }
}
