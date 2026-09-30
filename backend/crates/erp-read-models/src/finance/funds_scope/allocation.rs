//! 进项发票分配与采购关联范围查询。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::entity::payable::AllocationAction as PayableAllocationAction;
use erp_finance::ports::funds_scope::FundsResolvedScope;
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_procurement::PurchaseAccess;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::authorization::*;
use super::payable_source::source_key;
use super::rows::*;
use crate::{Error, Result};

/// 进项发票分配的授权裁剪单元。
pub(super) struct PurchaseInvoiceLink {
    /// 分配主键。
    pub(super) id: String,
    /// 正反动作后的含税方向金额。
    pub(super) signed: Amount,
    /// 归属采购单；结算来源使用带类型关联键；缺失来源拒绝。
    pub(super) order: LinkedOrderId,
    /// 响应视图。
    pub(super) view: erp_finance::dto::payable::PurchaseInvoiceAllocationView,
}

impl FundsAccess {
    /// 销售与采购双关联同时证明；发票双方向查询使用，单方向复用 resolve 即可。
    pub async fn resolve_dual(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<(FundsResolvedScope, FundsAuthorization)> {
        let _ = purchase_access;
        self.resolve_sources(actor, resource, action, true, true, executor).await
    }

    /// 进项发票分配按应付子账反查采购单；结算来源保留自身责任边界。
    pub(super) async fn purchase_invoice_matched_links(
        &self,
        invoice_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<PurchaseInvoiceLink>>> {
        use erp_core::ids::{InvoiceId, PayableAccountId};
        let keys = invoice_ids.iter().map(|id| InvoiceId::new(id.clone())).collect::<Vec<_>>();
        let allocations =
            self.db.purchase_invoice_allocations().find_allocations_by_invoices(&keys, executor).await?;
        let account_keys = allocations
            .iter()
            .map(|item| PayableAccountId::new(item.payable_account_id.to_string()))
            .collect::<Vec<_>>();
        let accounts = self.db.payable_accounts().find_accounts_by_ids(&account_keys, executor).await?;
        let account_order = accounts
            .into_iter()
            .map(|account| {
                let order = Some(source_key(account.source_type, &account.source_document_id));
                (account.base.id.clone(), order)
            })
            .collect::<HashMap<_, _>>();
        let mut links: HashMap<String, Vec<PurchaseInvoiceLink>> = HashMap::new();
        for item in allocations {
            let signed = match item.allocation_action {
                PayableAllocationAction::Apply => item.allocated_gross_amount,
                PayableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_gross_amount),
            };
            let order = account_order.get(&item.payable_account_id.to_string()).cloned().flatten();
            let view = erp_finance::dto::payable::PurchaseInvoiceAllocationView {
                id: item.base.id.clone(),
                invoice_id: item.invoice_id.to_string(),
                allocation_seq: item.allocation_seq,
                allocation_action: item.allocation_action,
                payable_account_id: item.payable_account_id.to_string(),
                allocated_gross_amount: item.allocated_gross_amount,
                allocated_net_amount: item.allocated_net_amount,
                allocated_tax_amount: item.allocated_tax_amount,
                reverses_allocation_id: item.reverses_allocation_id.as_ref().map(|id| id.to_string()),
            };
            let id = item.base.id.clone();
            let invoice = item.invoice_id.to_string();
            links.entry(invoice).or_default().push(PurchaseInvoiceLink { id, signed, order, view });
        }
        Ok(links)
    }
}
use erp_finance::entity::payable::PurchaseInvoiceAllocation;
use erp_finance::repository::PurchaseInvoiceAllocationFilter;

/// 进项发票分配的范围装配行：归属采购单、收票经办人与整单资格。
pub(super) struct ScopedPurchaseAllocation {
    /// 分配实体。
    pub(super) item: PurchaseInvoiceAllocation,
    /// 归属采购单；结算来源使用带类型关联键；缺失来源拒绝。
    pub(super) order: LinkedOrderId,
    /// 方向金额（正反动作已记符号）。
    pub(super) signed: Amount,
    /// 进项发票号码。
    pub(super) invoice_no: Option<String>,
    /// 装载时授权快照决定的本行整单读取资格。
    pub(super) whole_flag: bool,
    /// 归属采购单当前负责人；无归属时为空，份额计入未分配。
    pub(super) owner_name: Option<String>,
    /// 当前真实来源版本，责任交接须使分页凭据失效。
    pub(super) source_version: u64,
}

impl FundsAccess {
    /// 分页查询进项发票分配范围行：采购负责人与收票经办人分别查询（仅列表）。
    pub async fn purchase_invoice_allocation_list_scoped(
        &self,
        params: &erp_finance::dto::payable::PurchaseInvoiceAllocationListParams,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self
            .checked_purchase_invoice_allocations(
                params,
                &query,
                actor,
                purchase_access,
                params.scope_version.as_deref(),
            )
            .await?;
        Ok(snapshot)
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    pub(super) async fn checked_purchase_invoice_allocations(
        &self,
        params: &erp_finance::dto::payable::PurchaseInvoiceAllocationListParams,
        query: &erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        checked_twice(expected, || {
            self.snapshot_purchase_invoice_allocations(params, query, actor, purchase_access)
        })
        .await
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    pub(super) async fn snapshot_purchase_invoice_allocations(
        &self,
        params: &erp_finance::dto::payable::PurchaseInvoiceAllocationListParams,
        query: &erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_purchase_invoice_allocations(
                        &params,
                        &query,
                        &actor,
                        &purchase_access,
                        executor,
                    )
                    .await
                })
            })
            .await
    }

    /// 分页查询进项发票分配范围行：分配按应付子账反查采购单，收票经办取发票登记人。
    pub(super) async fn load_purchase_invoice_allocations(
        &self,
        params: &erp_finance::dto::payable::PurchaseInvoiceAllocationListParams,
        query: &erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        let (access, authorization) = self
            .resolve_with_purchase(actor, "purchase_invoice_allocation", "list", purchase_access, executor)
            .await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "进项发票分配无可见范围"));
        }
        let filter = PurchaseInvoiceAllocationFilter {
            payable_account_id: query.payable_account_id.clone(),
            invoice_id: None,
            page: 1,
            page_size: 10_000,
            sort_ascending: false,
        };
        let candidates = self
            .db
            .purchase_invoice_allocations()
            .search_purchase_invoice_allocations(&filter, executor)
            .await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("收票查询超过上限，请收窄组织或负责人条件".into()));
        }
        let assembled = self
            .assemble_purchase_invoice_allocations(
                query,
                candidates.items,
                &access,
                &authorization,
                purchase_access,
                executor,
            )
            .await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        let mut all_whole = true;
        let mut owner_of = HashMap::new();
        for scoped in assembled.iter() {
            scoped.hash_source(&mut fingerprint);
            triples.push((scoped.item.base.id.clone(), scoped.signed, scoped.order.clone()));
            if scoped.whole() {
                whole_sum = whole_sum.checked_add(scoped.signed);
            } else {
                all_whole = false;
            }
        }
        for (order, owner) in
            assembled.iter().filter_map(|scoped| scoped.order.clone().map(|id| (id, scoped.owner())))
        {
            if let Some(owner) = owner {
                owner_of.insert(order, owner);
            }
        }
        let total = assembled.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = u64::from(query.paging.page_size).max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(assembled.len());
        let mut items = Vec::new();
        if start < assembled.len() {
            for scoped in assembled[start..end].iter() {
                let whole = scoped.whole();
                items.push(ScopedPurchaseInvoiceAllocationRow {
                    id: scoped.item.base.id.clone(),
                    invoice_id: scoped.item.invoice_id.to_string(),
                    invoice_no: scoped.invoice_no.clone(),
                    payable_account_id: scoped.item.payable_account_id.to_string(),
                    created_at: scoped.item.base.created_at,
                    visible_allocated_amount: scoped.signed,
                    allocated_gross_amount: whole_amount(whole, scoped.signed),
                    permission_limited: !whole,
                });
            }
        }
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary =
            build_summary(&triples, &owner_of, whole_amount(all_whole, whole_sum), &version, !all_whole)?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            page,
            page_size: page_size as u32,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "收票分配继承应付子账的采购或结算来源边界，仅返回获授权分配",
            ownership_basis: "linked_purchase_owner_and_invoice_operator",
        })
    }

    /// 进项发票分配候选逐行判定可见性与筛选；授权集合外的份额不进入行与汇总。
    pub(super) async fn assemble_purchase_invoice_allocations(
        &self,
        query: &erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery,
        rows: Vec<PurchaseInvoiceAllocation>,
        _access: &FundsResolvedScope,
        authorization: &FundsAuthorization,
        _purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ScopedPurchaseAllocation>> {
        use erp_core::ids::{InvoiceId, PayableAccountId};
        let account_keys = rows
            .iter()
            .map(|item| PayableAccountId::new(item.payable_account_id.to_string()))
            .collect::<Vec<_>>();
        let accounts = self.db.payable_accounts().find_accounts_by_ids(&account_keys, executor).await?;
        let account_order = accounts
            .into_iter()
            .map(|account| {
                let order = Some(source_key(account.source_type, &account.source_document_id));
                (account.base.id.clone(), order)
            })
            .collect::<HashMap<_, _>>();
        let invoice_keys =
            rows.iter().map(|item| InvoiceId::new(item.invoice_id.to_string())).collect::<Vec<_>>();
        let invoice_ids = invoice_keys.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        let invoices = self.db.invoices().find_invoices_by_ids(&invoice_ids, executor).await?;
        let invoice_operator = invoices
            .into_iter()
            .map(|invoice| {
                (invoice.base.id.clone(), (invoice.stable.created_by.clone(), invoice.invoice_no.clone()))
            })
            .collect::<HashMap<_, _>>();
        let po_ids = account_order.values().filter_map(|order| order.clone()).collect::<Vec<_>>();
        let purchase_facts = self.purchase_fact_map(&po_ids, executor).await?;
        let allowed = authorization.payable_ids(&purchase_facts);
        let condition = self.purchase_invoice_condition(query, executor).await?;
        let mut decided = Vec::new();
        for item in rows {
            let order = account_order.get(&item.payable_account_id.to_string()).cloned().flatten();
            let (operator, invoice_no) = invoice_operator
                .get(&item.invoice_id.to_string())
                .cloned()
                .map(|(operator, no)| (Some(operator), Some(no)))
                .unwrap_or((None, None));
            if let (Some(order), Some(allowed)) = (order.as_ref(), allowed.as_ref())
                && !allowed.contains(order)
            {
                continue;
            }
            let fact = order.as_ref().and_then(|id| purchase_facts.get(id));
            let row_facts = FundsLinkedFacts {
                owner_user_id: fact.and_then(|order| order.owner_user_id.clone()),
                business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
                operator_user_ids: operator.clone().into_iter().collect(),
                secondary_operator_user_ids: Vec::new(),
                linked_document_id: item.payable_account_id.to_string(),
                linked_document_version: item.base.version,
            };
            if fact.is_none() {
                continue;
            }
            if !matches_linked_condition(&row_facts, &condition) {
                continue;
            }
            let signed = match item.allocation_action {
                PayableAllocationAction::Apply => item.allocated_gross_amount,
                PayableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_gross_amount),
            };
            let whole = true;
            decided.push(ScopedPurchaseAllocation {
                item,
                order,
                signed,
                invoice_no,
                whole_flag: whole,
                owner_name: fact.and_then(|order| order.owner_user_id.clone()),
                source_version: fact.map(|source| source.version).unwrap_or(0),
            });
        }
        Ok(decided)
    }

    /// 收票分配关联筛选条件：采购负责人、收票经办人与组织分别精确匹配。
    pub(super) async fn purchase_invoice_condition(
        &self,
        query: &erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = match &query.org_unit_ids {
            Some(ids) => {
                let list = ids.as_slice().to_vec();
                let expanded = self
                    .expand_org_units(&list, query.include_descendants.unwrap_or(false), executor)
                    .await?;
                Some(expanded.into_iter().collect::<Vec<_>>())
            },
            None => None,
        };
        Ok(FundsLinkedCondition {
            owner_user_ids: query.procurement_owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            operator_user_ids: query.operator_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            secondary_operator_user_ids: None,
            org_unit_ids,
        })
    }
}

impl ScopedPurchaseAllocation {
    /// 分配和来源版本共同绑定分页凭据，责任交接会使旧快照失效。
    fn hash_source(&self, fingerprint: &mut impl Hasher) {
        self.item.base.id.hash(fingerprint);
        self.item.base.version.hash(fingerprint);
        self.order.hash(fingerprint);
        self.source_version.hash(fingerprint);
    }

    /// 本行整单读取资格由调用时授权快照决定，装载后不再重算。
    pub(super) fn whole(&self) -> bool {
        self.whole_flag
    }

    /// 归属采购单当前负责人；结算单来源与缺失时为空，份额计入未分配。
    pub(super) fn owner(&self) -> Option<String> {
        self.owner_name.clone()
    }
}
