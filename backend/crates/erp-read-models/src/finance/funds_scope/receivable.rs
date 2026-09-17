//! 应收子账范围查询。

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::repository::ReceivableExt;
use erp_sales::repository::SalesOrderExt;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::authorization::*;
use super::rows::*;
use crate::{Error, Result};

/// M07 应收子账范围查询：负责销售与登记经办人分别查询，同一事务内解析与取数。
impl FundsAccess {
    /// 分页查询应收往来子账范围行。
    pub async fn receivable_account_list_scoped(
        &self,
        params: &erp_finance::dto::receivable::ReceivableAccountListParams,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedReceivableAccountRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot =
            self.checked_receivable_accounts(params, &query, actor, params.scope_version.as_deref()).await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析应收子账详情动作；不可见与不存在统一为 NotFound。
    pub async fn receivable_account_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<FundsScopedResult<ScopedReceivableAccountRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (access, authorization) =
                        this.resolve(&actor, "receivable_account", "detail", executor).await?;
                    let account = this
                        .db
                        .receivable_accounts()
                        .find_by_id(&id, executor)
                        .await
                        .map_err(Error::from)?
                        .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
                    let order = this
                        .db
                        .sales_orders()
                        .find_by_id(account.sales_order_id.as_ref(), executor)
                        .await
                        .map_err(Error::from)?
                        .ok_or_else(|| Error::NotFound("应收账户来源销售单不存在".into()))?;
                    let facts = FundsLinkedFacts {
                        owner_user_id: Some(order.sales_owner_user_id.clone()),
                        business_org_unit_id: Some(order.business_org_unit_id.clone()),
                        operator_user_ids: vec![account.stable.created_by.clone()],
                        secondary_operator_user_ids: Vec::new(),
                        linked_document_id: order.base.id.clone(),
                        linked_document_version: order.base.version,
                    };
                    if !Self::allows(&access, &facts)? {
                        return Err(Error::NotFound("应收往来子账不存在".into()));
                    }
                    let whole = authorization.whole();
                    let visible = this.receivable_visible_share(&account.base.id, executor).await?;
                    let row = ScopedReceivableAccountRow {
                        id: account.base.id.clone(),
                        sales_order_id: account.sales_order_id.to_string(),
                        account_seq: account.account_seq,
                        status: account.stable.status(),
                        created_at: account.base.created_at,
                        visible_settled_share: visible,
                        gross_total: whole_amount(whole, account.gross_total),
                        settled_total: whole_amount(whole, account.settled_total),
                        open_total: whole_amount(whole, account.open_total),
                        permission_limited: !whole,
                        sales_owner_user_id: Some(order.sales_owner_user_id.clone()),
                        business_org_unit_id: Some(order.business_org_unit_id.clone()),
                    };
                    let parts = vec![facts.version_part()];
                    let version = scope_version(&authorization.context, &parts);
                    Ok(FundsScopedResult {
                        data: row,
                        scope_version: version,
                        policy_version: authorization.context.policy_version,
                        organization_version: authorization.context.organizations.version,
                        as_of: authorization.context.as_of.as_utc().to_rfc3339(),
                        empty_reason: None,
                        scope_summary: "应收子账按关联销售当前负责人与登记经办人授权；部分授权仅返获授权份额",
                        ownership_basis: "current_sales_owner_and_register_operator",
                    })
                })
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    pub(super) async fn checked_receivable_accounts(
        &self,
        params: &erp_finance::dto::receivable::ReceivableAccountListParams,
        query: &erp_finance::dto::receivable::ReceivableAccountListQuery,
        actor: &AuditActor,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedReceivableAccountRow>> {
        let snapshot = self.snapshot_receivable_accounts(params, query, actor).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current = self.snapshot_receivable_accounts(params, query, actor).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    pub(super) async fn snapshot_receivable_accounts(
        &self,
        params: &erp_finance::dto::receivable::ReceivableAccountListParams,
        query: &erp_finance::dto::receivable::ReceivableAccountListQuery,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedReceivableAccountRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { this.load_receivable_accounts(&params, &query, &actor, executor).await },
                )
            })
            .await
    }

    /// 隐藏来源条件拒绝，空范围保持空集；完整装载并裁剪后才允许分页。
    pub(super) async fn load_receivable_accounts(
        &self,
        params: &erp_finance::dto::receivable::ReceivableAccountListParams,
        query: &erp_finance::dto::receivable::ReceivableAccountListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedReceivableAccountRow>> {
        let (access, authorization) = self.resolve(actor, "receivable_account", "list", executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "应收子账无可见范围"));
        }
        let expanded_orgs = match (&query.org_unit_ids, query.include_descendants) {
            (Some(ids), _) => {
                let list = ids.as_slice().to_vec();
                let expanded = self
                    .expand_org_units(&list, query.include_descendants.unwrap_or(false), executor)
                    .await?;
                Some(expanded.into_iter().collect::<Vec<_>>())
            },
            (None, _) => None,
        };
        let authorized_ids = self.authorized_sales_ids(&authorization, executor).await?;
        let authorized_set = authorized_ids.map(|ids| ids.into_iter().collect::<BTreeSet<_>>());
        let keyword_ids = crate::finance::search::keyword_ids(
            &self.db,
            query.q.as_deref(),
            erp_finance::repository::keyword::FinanceSearchTarget::Receivable,
        )
        .await?;
        let filter = erp_finance::repository::ReceivableAccountFilter {
            keyword_ids,
            keyword: None,
            keyword_sales_order_ids: Vec::new(),
            keyword_party_ids: Vec::new(),
            account_id: query.account_id.clone(),
            customer_id: query.customer_id.clone(),
            counterparty_party_id: query.counterparty_party_id.clone(),
            status: query.status,
            sales_order_id: query.sales_order_id.clone(),
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, erp_finance::dto::receivable::SortDir::Asc),
        };
        let candidates = self
            .db
            .receivable_accounts()
            .search_receivable_accounts(&filter, executor)
            .await
            .map_err(Error::from)?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("应收查询超过上限，请收窄组织或负责人条件".into()));
        }
        let sales_ids = candidates
            .items
            .iter()
            .map(|row| row.sales_order_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut orders_by_id = std::collections::HashMap::new();
        for chunk in sales_ids.chunks(500) {
            let ids = chunk.iter().map(erp_core::ids::SalesOrderId::new).collect::<Vec<_>>();
            for order in
                self.db.sales_orders().find_orders_by_ids(&ids, executor).await.map_err(Error::from)?
            {
                orders_by_id.insert(order.base.id.clone(), order);
            }
        }
        let condition = FundsLinkedCondition {
            owner_user_ids: query.sales_owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            operator_user_ids: query.operator_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            secondary_operator_user_ids: None,
            org_unit_ids: expanded_orgs,
        };
        let mut filtered = Vec::new();
        for row in candidates.items {
            let Some(order) = orders_by_id.get(&row.sales_order_id) else {
                continue;
            };
            if let Some(allowed) = &authorized_set
                && !allowed.contains(&order.base.id)
            {
                continue;
            }
            let facts = FundsLinkedFacts {
                owner_user_id: Some(order.sales_owner_user_id.clone()),
                business_org_unit_id: Some(order.business_org_unit_id.clone()),
                operator_user_ids: vec![row.stable.created_by.clone()],
                secondary_operator_user_ids: Vec::new(),
                linked_document_id: order.base.id.clone(),
                linked_document_version: order.base.version,
            };
            if !Self::allows(&access, &facts)? {
                continue;
            }
            if !matches_linked_condition(&facts, &condition) {
                continue;
            }
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            order.base.id.hash(&mut fingerprint);
            order.base.version.hash(&mut fingerprint);
            filtered.push((row, order.base.id.clone(), order.base.version));
        }
        let total = filtered.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = query.paging.page_size.max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(filtered.len());
        let whole = authorization.whole();
        let mut items = Vec::new();
        if start < filtered.len() {
            for (row, _, _) in filtered[start..end].iter() {
                let visible = self.receivable_visible_share(&row.id, executor).await?;
                items.push(ScopedReceivableAccountRow {
                    id: row.id.clone(),
                    sales_order_id: row.sales_order_id.clone(),
                    account_seq: row.account_seq,
                    status: row.stable.status(),
                    created_at: row.created_at,
                    visible_settled_share: visible,
                    gross_total: whole_amount(whole, row.gross_total),
                    settled_total: whole_amount(whole, row.settled_total),
                    open_total: whole_amount(whole, row.open_total),
                    permission_limited: !whole,
                    sales_owner_user_id: orders_by_id
                        .get(&row.sales_order_id)
                        .map(|order| order.sales_owner_user_id.clone()),
                    business_org_unit_id: orders_by_id
                        .get(&row.sales_order_id)
                        .map(|order| order.business_org_unit_id.clone()),
                });
            }
        }
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        for (row, order_id, _) in filtered.iter() {
            let visible = self.receivable_visible_share(&row.id, executor).await?;
            triples.push((row.id.clone(), visible, Some(order_id.clone())));
            whole_sum = whole_sum.checked_add(row.gross_total);
        }
        let owner_of = orders_by_id
            .iter()
            .map(|(id, order)| (id.clone(), order.sales_owner_user_id.clone()))
            .collect::<HashMap<_, _>>();
        let summary = build_summary(&triples, &owner_of, whole_amount(whole, whole_sum), &version, !whole)?;
        let owner_options = self.owner_options_sales(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "应收子账按关联销售当前负责人与登记经办人授权；部分授权仅返获授权份额",
            ownership_basis: "current_sales_owner_and_register_operator",
        })
    }

    /// 计算子账获授权核销份额；未分配与未授权份额不计入，禁止差额推导。
    pub(super) async fn receivable_visible_share(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        use erp_core::ids::{ReceivableAccountId, ReceivableEntryId};
        let entries = self
            .db
            .receivable_entries()
            .find_entries_by_accounts(
                std::slice::from_ref(&ReceivableAccountId::new(account_id.to_string())),
                executor,
            )
            .await
            .map_err(Error::from)?;
        let entry_ids =
            entries.iter().map(|entry| ReceivableEntryId::new(entry.base.id.clone())).collect::<Vec<_>>();
        if entry_ids.is_empty() {
            return Ok(erp_finance::service::receivable::mapping::zero_amount());
        }
        let allocations = self
            .db
            .receipt_allocations()
            .find_allocations_by_entries(&entry_ids, executor)
            .await
            .map_err(Error::from)?;
        let mut net = erp_finance::service::receivable::mapping::zero_amount();
        for allocation in allocations {
            match allocation.allocation_action {
                erp_finance::entity::receivable::AllocationAction::Apply => {
                    net = net.checked_add(allocation.allocated_amount);
                },
                erp_finance::entity::receivable::AllocationAction::Reverse => {
                    net = net.checked_sub(allocation.allocated_amount);
                },
            }
        }
        Ok(net)
    }
}
