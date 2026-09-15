//! 授权、成本、分配和销售责任事实在同一事务读取，交付前独立事务重验。
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use erp_finance::entity::cost::CostAllocation;
use erp_finance::repository::CostExt;
use erp_finance::repository::cost::read_scope::{ALLOCATION_LIMIT, ENTRY_LIMIT};
use erp_finance::repository::cost::{CostEntryFilter, CostEntryRow};
use erp_finance::service::cost::{cost_entry_filter, cost_entry_row_view};
use erp_identity::Permission;
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use persistence_core::{Executor, Transactional};

use super::*;
use crate::sales_center::access::SalesAccess;

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
    cost: SalesReadScope,
    context: AuthorizedDataScope,
    fingerprint: DefaultHasher,
}
impl CostAuthorization {
    fn whole(&self) -> bool {
        self.sales.is_company() && self.cost.is_company()
    }
    fn empty(&self) -> bool {
        self.sales.is_empty() || self.cost.is_empty()
    }
}
impl Snapshot {
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
    pub fn result<T>(self, data: T) -> CostReadResult<T> {
        CostReadResult {
            data,
            scope_version: self.version,
            policy_version: self.policy,
            organization_version: self.organization,
            as_of: self.as_of,
            empty_reason: self.no_scope.then_some("no_scope"),
            scope_summary: "当前成本权限与关联销售权限交集；受限整笔金额不返回",
            ownership_basis: "current_sales_allocation",
        }
    }
    /// 在分页前裁剪完整候选；保留分配本身的创建时间，不能用成本创建时间替代。
    fn project(
        &mut self,
        candidates: Vec<CostEntryRow>,
        allocations: Vec<CostAllocation>,
        orders: &BTreeSet<String>,
        access: &mut CostAuthorization,
    ) -> Result<()> {
        let mut grouped = HashMap::<String, Vec<_>>::new();
        for line in allocations {
            self.allocation_created_at.insert(line.base.id.clone(), line.base.created_at);
            line.base.id.hash(&mut access.fingerprint);
            line.base.version.hash(&mut access.fingerprint);
            grouped.entry(line.cost_entry_id.to_string()).or_default().push(line);
        }
        for row in candidates {
            row.id.hash(&mut access.fingerprint);
            row.version.hash(&mut access.fingerprint);
            let allocations = grouped.remove(&row.id).unwrap_or_default();
            if let Some(view) = cost_entry_row_view(row, allocations).restrict(access.whole(), orders)? {
                self.rows.push(view);
            }
        }
        Ok(())
    }
}
impl CostReadModel {
    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    pub(super) async fn checked(
        &self,
        params: &CostEntryListParams,
        id: Option<&str>,
        resource: &str,
        action: &str,
        actor: &AuditActor,
        expected: Option<&str>,
    ) -> Result<Snapshot> {
        let snapshot = self.snapshot(params, id, resource, action, actor).await?;
        if expected.is_some_and(|value| value != snapshot.version) {
            return Err(changed());
        }
        let current = self.snapshot(params, id, resource, action, actor).await?;
        if current.version != snapshot.version {
            return Err(changed());
        }
        Ok(snapshot)
    }
    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    async fn snapshot(
        &self,
        params: &CostEntryListParams,
        id: Option<&str>,
        resource: &str,
        action: &str,
        actor: &AuditActor,
    ) -> Result<Snapshot> {
        let filter = cost_entry_filter(params)?;
        let this = self.clone();
        let actor = actor.clone();
        let id = id.map(str::to_owned);
        let resource = resource.to_owned();
        let action = action.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = this.authorize(&actor, &resource, &action, executor).await?;
                    this.load_snapshot(&filter, id.as_deref(), access, executor).await
                })
            })
            .await
    }
    /// 收入和目标成本动作必须由同一合格角色集合证明，两个资源范围仍分别解析。
    async fn authorize(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<CostAuthorization> {
        let resolver = SalesAccess::new(self.db.clone(), self.rbac.clone());
        let (sales_context, sales) = resolver
            .resolve(actor, "list", &[Permission::parse(format!("{resource}:{action}"))?], executor)
            .await?;
        let (context, cost) = resolver
            .resolve_resource(actor, resource, action, &[Permission::parse("sales_order:list")?], executor)
            .await?;
        let mut fingerprint = DefaultHasher::new();
        sales_context.scope_version.hash(&mut fingerprint);
        context.scope_version.hash(&mut fingerprint);
        Ok(CostAuthorization { sales, cost, context, fingerprint })
    }
    /// 隐藏来源条件拒绝，空范围保持空集；完整装载并裁剪后才允许分页。
    async fn load_snapshot(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        mut access: CostAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Snapshot> {
        let mut snapshot = Snapshot::new(&access);
        if !access.empty() {
            if !access.whole() && filter.source_document_id.is_some() {
                return Err(Error::ValidationError("当前只能读取成本分配，不能按整笔来源单据筛选".into()));
            }
            let candidates = self.cost_candidates(filter, id, executor).await?;
            let allocations = self.candidate_allocations(&candidates, executor).await?;
            let orders = self.authorized_orders(&allocations, &mut access, executor).await?;
            snapshot.project(candidates, allocations, &orders, &mut access)?;
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
    /// 在销售与成本范围交集中装载当前销售责任，业务版本加入跨页凭据。
    async fn authorized_orders(
        &self,
        allocations: &[CostAllocation],
        access: &mut CostAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        let ids = allocations
            .iter()
            .filter_map(|line| line.sales_order_id.as_ref().map(ToString::to_string))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut orders = BTreeSet::new();
        for chunk in ids.chunks(500) {
            let mut current =
                self.db.sales_orders().scope_orders(chunk, &access.sales, &access.cost, executor).await?;
            current.sort_by(|a, b| a.base.id.cmp(&b.base.id));
            for order in current {
                order.base.id.hash(&mut access.fingerprint);
                order.base.version.hash(&mut access.fingerprint);
                orders.insert(order.base.id);
            }
        }
        Ok(orders)
    }
}
fn changed() -> Error {
    Error::ConflictError("DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新".into())
}
