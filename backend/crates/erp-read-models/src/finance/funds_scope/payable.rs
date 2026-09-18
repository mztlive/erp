//! 应付子账范围查询。

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableAccountFilter, PayableAccountRow, PayableExt};
use erp_procurement::PurchaseAccess;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::allocation::purchase_whole;
use super::authorization::*;
use super::rows::*;
use crate::{Error, Result};

impl FundsAccess {
    /// 分页查询应付往来子账范围行：来源采购单当前采购负责人查询。
    pub async fn payable_account_list_scoped(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self
            .checked_payable_accounts(params, &query, actor, purchase_access, params.scope_version.as_deref())
            .await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析应付子账详情动作；不可见与不存在统一为 NotFound。
    pub async fn payable_account_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedResult<ScopedPayableAccountRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_payable_account_detail(&id, &actor, &purchase_access, executor).await
                })
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    pub(super) async fn checked_payable_accounts(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        let snapshot = self.snapshot_payable_accounts(params, query, actor, purchase_access).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current = self.snapshot_payable_accounts(params, query, actor, purchase_access).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    pub(super) async fn snapshot_payable_accounts(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
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
                    this.load_payable_accounts(&params, &query, &actor, &purchase_access, executor).await
                })
            })
            .await
    }

    /// 分页查询应付子账范围行：采购来源按当前采购负责人，非采购来源按空事实。
    pub(super) async fn load_payable_accounts(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        use erp_finance::entity::payable::PayableSourceType;
        let (access, authorization) =
            self.resolve_with_purchase(actor, "payable_account", "list", purchase_access, executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "应付子账无可见范围"));
        }
        let keyword_ids = crate::finance::search::keyword_ids(
            &self.db,
            query.q.as_deref(),
            erp_finance::repository::keyword::FinanceSearchTarget::Payable,
        )
        .await?;
        let filter = PayableAccountFilter {
            source_document_id: query.source_document_id.clone(),
            keyword_ids,
            supplier_id: query.supplier_id.clone(),
            source_type: query.source_type,
            status: query.status,
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, application_core::SortDir::Asc),
        };
        let candidates = self.db.payable_accounts().search_payable_accounts(&filter, executor).await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("应付查询超过上限，请收窄组织或负责人条件".into()));
        }
        let po_ids = candidates
            .items
            .iter()
            .filter(|row| row.source_type == PayableSourceType::PurchaseOrder)
            .map(|row| row.source_document_id.clone())
            .collect::<Vec<_>>();
        let purchase_facts = self.purchase_fact_map(&po_ids, executor).await?;
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let condition = self.payable_account_condition(query, executor).await?;
        let mut decided = Vec::new();
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for row in candidates.items {
            let fact = (row.source_type == PayableSourceType::PurchaseOrder)
                .then(|| purchase_facts.get(&row.source_document_id))
                .flatten();
            if fact.is_some()
                && let Some(allowed) = &allowed
                && !allowed.contains(&row.source_document_id)
            {
                continue;
            }
            let row_facts = FundsLinkedFacts {
                owner_user_id: fact.and_then(|order| order.owner_user_id.clone()),
                business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
                operator_user_ids: Vec::new(),
                secondary_operator_user_ids: Vec::new(),
                linked_document_id: row.source_document_id.clone(),
                linked_document_version: fact.map(|order| order.version).unwrap_or(0),
            };
            if !Self::allows(&access, &row_facts)? {
                continue;
            }
            if !matches_linked_condition(&row_facts, &condition) {
                continue;
            }
            let whole = match fact {
                Some(_) => purchase_whole(&authorization),
                None => authorization.funds.is_company(),
            };
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            row.source_document_id.hash(&mut fingerprint);
            fact.map(|order| order.version).unwrap_or(0).hash(&mut fingerprint);
            decided.push((row, fact.cloned(), whole));
        }
        self.finish_payable_accounts(params, query, decided, authorization, fingerprint, executor).await
    }

    /// 应付子账关联筛选条件：采购负责人与组织分别精确匹配，只收窄授权结果。
    pub(super) async fn payable_account_condition(
        &self,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
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
            operator_user_ids: None,
            secondary_operator_user_ids: None,
            org_unit_ids,
        })
    }

    /// 应付子账候选分页裁剪与汇总装配；明细、汇总与导出复用同一已决集合。
    pub(super) async fn finish_payable_accounts(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        decided: Vec<(PayableAccountRow, Option<LinkedPurchaseFact>, bool)>,
        authorization: FundsAuthorization,
        fingerprint: std::collections::hash_map::DefaultHasher,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        let total = decided.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = u64::from(query.paging.page_size).max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let mut items = Vec::new();
        if start < decided.len() {
            for (row, fact, whole) in decided[start..end].iter() {
                items.push(cut_payable_account_row(row, fact.as_ref(), *whole));
            }
        }
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        let mut all_whole = true;
        for (row, fact, whole) in decided.iter() {
            all_whole &= *whole;
            let order = fact.as_ref().map(|_| row.source_document_id.clone());
            triples.push((row.id.clone(), row.settled_total, order));
            if *whole {
                whole_sum = whole_sum.checked_add(row.gross_total);
            }
        }
        let owner_of = decided
            .iter()
            .filter_map(|(row, fact, _)| {
                fact.as_ref()
                    .and_then(|order| order.owner_user_id.clone())
                    .map(|owner| (row.source_document_id.clone(), owner))
            })
            .collect::<HashMap<_, _>>();
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary =
            build_summary(&triples, &owner_of, whole_amount(all_whole, whole_sum), &version, !all_whole)?;
        let owner_options = self.owner_options_purchase(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size: page_size as u32,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "应付子账按来源采购单当前采购负责人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_purchase_owner",
        })
    }

    /// 应付子账详情同一事务内解析、取数与裁剪；版本绑定来源采购单。
    pub(super) async fn load_payable_account_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedPayableAccountRow>> {
        use erp_core::ids::PayableAccountId;
        use erp_finance::entity::payable::PayableSourceType;
        let (access, authorization) =
            self.resolve_with_purchase(actor, "payable_account", "detail", purchase_access, executor).await?;
        let accounts = self
            .db
            .payable_accounts()
            .find_accounts_by_ids(&[PayableAccountId::new(id.to_string())], executor)
            .await?;
        let account =
            accounts.into_iter().next().ok_or_else(|| Error::NotFound("应付往来子账不存在".into()))?;
        let fact = if account.source_type == PayableSourceType::PurchaseOrder {
            self.purchase_fact_map(std::slice::from_ref(&account.source_document_id), executor)
                .await?
                .remove(&account.source_document_id)
        } else {
            None
        };
        if fact.is_some()
            && let Some(scope) = authorization.purchase_scope.as_ref()
        {
            let allowed = self.authorized_purchase_ids(purchase_access, scope, executor).await?;
            if allowed.is_some_and(|list| !list.contains(&account.source_document_id)) {
                return Err(Error::NotFound("应付往来子账不存在".into()));
            }
        }
        let row_facts = FundsLinkedFacts {
            owner_user_id: fact.as_ref().and_then(|order| order.owner_user_id.clone()),
            business_org_unit_id: fact.as_ref().map(|order| order.business_org_unit_id.clone()),
            operator_user_ids: Vec::new(),
            secondary_operator_user_ids: Vec::new(),
            linked_document_id: account.source_document_id.clone(),
            linked_document_version: fact.as_ref().map(|order| order.version).unwrap_or(0),
        };
        if !Self::allows(&access, &row_facts)? {
            return Err(Error::NotFound("应付往来子账不存在".into()));
        }
        let whole = match fact.as_ref() {
            Some(_) => purchase_whole(&authorization),
            None => authorization.funds.is_company(),
        };
        let data = cut_payable_account_row(
            &PayableAccountRow {
                id: account.base.id.clone(),
                stable: account.stable.clone(),
                source_document_id: account.source_document_id.clone(),
                supplier_id: account.supplier_id.to_string(),
                source_type: account.source_type,
                gross_total: account.gross_total,
                settled_total: account.settled_total,
                open_total: account.open_total,
                invoiceable_total: account.invoiceable_total,
                invoiced_total: account.invoiced_total,
                open_invoiceable_total: account.open_invoiceable_total,
                version: account.base.version,
                created_at: account.base.created_at,
            },
            fact.as_ref(),
            whole,
        );
        let parts = vec![format!("{}:{}", account.base.id, account.base.version), row_facts.version_part()];
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "应付子账按来源采购单当前采购负责人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_purchase_owner",
        })
    }
}

/// 应付子账单行裁剪；整单金额仅整单资格返回，否则为 null。
pub(super) fn cut_payable_account_row(
    row: &PayableAccountRow,
    fact: Option<&LinkedPurchaseFact>,
    whole: bool,
) -> ScopedPayableAccountRow {
    ScopedPayableAccountRow {
        id: row.id.clone(),
        source_document_id: row.source_document_id.clone(),
        source_type: row.source_type,
        supplier_id: row.supplier_id.clone(),
        status: row.stable.status(),
        created_at: row.created_at,
        visible_settled_share: row.settled_total,
        gross_total: whole_amount(whole, row.gross_total),
        settled_total: whole_amount(whole, row.settled_total),
        open_total: whole_amount(whole, row.open_total),
        permission_limited: !whole,
        procurement_owner_user_id: fact.and_then(|order| order.owner_user_id.clone()),
        business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
    }
}
