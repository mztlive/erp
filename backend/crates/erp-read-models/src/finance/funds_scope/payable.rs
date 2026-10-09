//! 应付子账范围查询。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::ids::PayableAccountId;
use erp_finance::dto::payable::PayableEntryView;
use erp_finance::entity::payable::PayableAccount;
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableAccountRow, PayableExt};
use erp_procurement::PurchaseAccess;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::super::payable::mapping::{payment_recipient_view, resolve_optional_payment_recipient_for_read};
use super::authorization::*;
use super::payable_source::source_key;
use super::repository::accounts::{AccountPageRow, AccountSummaryRow};
use super::rows::*;
use crate::{Error, Result};

impl FundsAccess {
    /// 分页查询应付往来子账范围行：来源采购单当前采购负责人查询。
    ///
    /// # 参数
    /// * `params` - 应付子账列表请求。
    /// * `actor` - 已认证操作人。
    /// * `purchase_access` - 采购访问器；本链路解析时不读取它。
    ///
    /// # 返回
    /// 最终授权应付子账页及同口径汇总。
    ///
    /// # 错误
    /// 参数校验、范围版本、授权或读取失败时返回对应错误。
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
    ///
    /// # 参数
    /// * `id` - 应付往来子账主键。
    /// * `actor` - 已认证操作人。
    /// * `purchase_access` - 采购访问器。
    ///
    /// # 返回
    /// 当前详情动作允许的应付子账行。
    ///
    /// # 错误
    /// 授权或读取失败时返回对应错误；子账不存在或来源责任不允许时返回 `NotFound`。
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
    ///
    /// # 参数
    /// * `params` - 原始列表请求。
    /// * `query` - 已规范化的列表查询。
    /// * `actor` - 已认证操作人。
    /// * `purchase_access` - 采购访问器。
    /// * `expected` - 调用方回传的范围版本；首页可为 `None`。
    ///
    /// # 返回
    /// 两次版本一致时返回第一次应付子账页。
    ///
    /// # 错误
    /// 首次版本失配或复核版本变化时返回范围变化冲突错误；授权或读取失败时返回对应错误。
    pub(super) async fn checked_payable_accounts(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        checked_revalidated(
            expected,
            || self.snapshot_payable_accounts(params, query, actor, purchase_access),
            || self.revalidate_payable_accounts(query, actor, purchase_access),
        )
        .await
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    ///
    /// # 参数
    /// * `params` - 原始列表请求。
    /// * `query` - 已规范化的列表查询。
    /// * `actor` - 已认证操作人。
    /// * `purchase_access` - 采购访问器。
    ///
    /// # 返回
    /// 同一事务内形成的授权应付子账页。
    ///
    /// # 错误
    /// 事务、授权或读取失败时返回对应错误。
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

    /// 分页查询应付子账范围行：采购来源按当前采购负责人，结算来源按当前对账负责人。
    ///
    /// # 参数
    /// * `params` - 原始列表请求，用于比对范围版本。
    /// * `query` - 已规范化的列表查询。
    /// * `actor` - 已认证操作人。
    /// * `purchase_access` - 采购访问器。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 无可见范围时返回空页；否则返回数据库分页、计数与同口径汇总。
    ///
    /// # 错误
    /// 授权、聚合、版本不一致或当前页名称读取失败时返回对应错误。
    pub(super) async fn load_payable_accounts(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        let (_access, authorization) =
            self.resolve_with_purchase(actor, "payable_account", "list", purchase_access, executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "应付子账无可见范围"));
        }
        let condition = self.payable_account_condition(query, executor).await?;
        let snapshot =
            self.payable_database_snapshot(query, &authorization, &condition, true, executor).await?;
        let version = snapshot.version(&authorization)?;
        ensure_version(params.scope_version.as_deref(), &version)?;
        let items = self.payable_page_items(&snapshot.items, executor).await?;
        let summary = payable_summary(&snapshot.summary, &version)?;
        Ok(FundsScopedPage {
            items,
            total: snapshot.total(),
            summary,
            page: query.paging.page,
            page_size: query.paging.page_size,
            scope_version: version,
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "应付子账继承采购或供应商结算的真实来源边界",
            ownership_basis: "linked_purchase_owner",
        })
    }

    /// 独立新事务只重验资格、真实来源与全部匹配行版本。
    async fn revalidate_payable_accounts(
        &self,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<String> {
        let this = self.clone();
        let query = query.clone();
        let actor = actor.clone();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (_, authorization) = this
                        .resolve_with_purchase(&actor, "payable_account", "list", &purchase_access, executor)
                        .await?;
                    if authorization.empty() {
                        return Ok(authorization.context.scope_version);
                    }
                    let condition = this.payable_account_condition(&query, executor).await?;
                    this.payable_database_snapshot(&query, &authorization, &condition, false, executor)
                        .await?
                        .version(&authorization)
                })
            })
            .await
    }

    /// 应付子账关联筛选条件：采购负责人与组织分别精确匹配，只收窄授权结果。
    ///
    /// # 参数
    /// * `query` - 已规范化的应付列表查询。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 采购负责人与展开组织的精确条件；经办人条件为空。
    ///
    /// # 错误
    /// 组织展开失败时返回对应错误。
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

    /// 只为数据库当前页补齐供应商名称和真实来源单号。
    async fn payable_page_items(
        &self,
        page: &[AccountPageRow<PayableAccountRow>],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ScopedPayableAccountRow>> {
        let ids = page.iter().map(|item| item.row.supplier_id.clone()).collect::<Vec<_>>();
        let names = self.supplier_legal_names(&ids, executor).await?;
        let mut items = page
            .iter()
            .map(|item| {
                let source = &item.source;
                let fact = LinkedPurchaseFact {
                    owner_user_id: source.owner_user_id.clone(),
                    business_org_unit_id: source.business_org_unit_id.clone(),
                    version: source.version,
                    document_no: source.document_no.clone(),
                };
                let mut row = cut_payable_account_row(&item.row, Some(&fact), true);
                row.supplier_name = names.get(&item.row.supplier_id).cloned();
                row
            })
            .collect::<Vec<_>>();
        self.fill_payment_guidance(&mut items, executor).await?;
        Ok(items)
    }

    /// 应付子账详情同一事务内解析、取数与裁剪；版本绑定来源采购单。
    ///
    /// # 参数
    /// * `id` - 应付往来子账主键。
    /// * `actor` - 已认证操作人。
    /// * `purchase_access` - 采购访问器。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 整单可读的应付子账行，含分录、供应商名称和收款方。
    ///
    /// # 错误
    /// 授权或读取失败时返回对应错误；子账不存在或来源责任不允许时返回 `NotFound`。
    pub(super) async fn load_payable_account_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedPayableAccountRow>> {
        let (_access, authorization) =
            self.resolve_with_purchase(actor, "payable_account", "detail", purchase_access, executor).await?;
        let accounts = self
            .db
            .payable_accounts()
            .find_accounts_by_ids(&[PayableAccountId::new(id.to_string())], executor)
            .await?;
        let account =
            accounts.into_iter().next().ok_or_else(|| Error::NotFound("应付往来子账不存在".into()))?;
        let key = source_key(account.source_type, &account.source_document_id);
        let fact = self
            .purchase_fact_map(std::slice::from_ref(&key), executor)
            .await?
            .remove(&key)
            .filter(|fact| authorization.payable_fact_allowed(&key, fact))
            .ok_or_else(|| Error::NotFound("应付往来子账不存在".into()))?;
        let fact = Some(fact);
        let row_facts = FundsLinkedFacts {
            owner_user_id: fact.as_ref().and_then(|order| order.owner_user_id.clone()),
            business_org_unit_id: fact.as_ref().map(|order| order.business_org_unit_id.clone()),
            operator_user_ids: Vec::new(),
            secondary_operator_user_ids: Vec::new(),
            linked_document_id: account.source_document_id.clone(),
            linked_document_version: fact.as_ref().map(|order| order.version).unwrap_or(0),
        };
        let data = self.payable_detail_data(&account, fact.as_ref(), executor).await?;
        let parts = vec![format!("{}:{}", account.base.id, account.base.version), row_facts.version_part()];
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "应付子账继承采购或供应商结算的真实来源边界",
            ownership_basis: "linked_purchase_owner",
        })
    }

    /// 在已验证来源资格的同一快照内装配应付详情。
    async fn payable_detail_data(
        &self,
        account: &PayableAccount,
        fact: Option<&LinkedPurchaseFact>,
        executor: &mut dyn Executor,
    ) -> Result<ScopedPayableAccountRow> {
        let mut data = cut_payable_account_row(
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
            fact,
            true,
        );
        data.entries = self.payable_detail_entries(account, executor).await?;
        self.fill_payment_guidance(std::slice::from_mut(&mut data), executor).await?;
        data.supplier_name = self.supplier_name_of(&account.supplier_id, executor).await?;
        data.payment_recipient =
            resolve_optional_payment_recipient_for_read(&self.db, &account.supplier_id, executor)
                .await?
                .as_ref()
                .map(payment_recipient_view);
        Ok(data)
    }

    /// 读取已授权子账的分录，保持原始分录身份供付款核销。
    async fn payable_detail_entries(
        &self,
        account: &PayableAccount,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntryView>> {
        Ok(self
            .db
            .payable_entries()
            .find_entries_by_accounts(&[PayableAccountId::new(account.base.id.clone())], executor)
            .await?
            .into_iter()
            .map(|entry| PayableEntryView {
                id: entry.base.id,
                entry_type: entry.entry_type,
                direction: entry.direction,
                amount: entry.amount,
                due_date: entry.due_date,
                source_document_id: entry.source_document_id.clone(),
                source_document_no: None,
                source_sequence: entry.source_sequence,
                posted_at: entry.posted_at,
            })
            .collect())
    }
}

/// 全范围窄金额摘要按原列表顺序折叠，保持负责人归组与整单口径。
fn payable_summary(rows: &[AccountSummaryRow], version: &str) -> Result<FundsSummaryView> {
    let mut triples = Vec::with_capacity(rows.len());
    let mut whole_sum = zero_amount();
    let mut owner_of = HashMap::new();
    for row in rows {
        triples.push((row.id.clone(), row.settled_total, Some(row.order_id.clone())));
        whole_sum = whole_sum.checked_add(row.gross_total);
        if let Some(owner) = &row.owner_user_id {
            owner_of.insert(row.order_id.clone(), owner.clone());
        }
    }
    build_summary(&triples, &owner_of, Some(whole_sum), version, false)
}

/// 应付子账单行裁剪；整单金额仅整单资格返回，否则为 null。
///
/// # 参数
/// * `row` - 应付子账行。
/// * `fact` - 真实来源责任；缺失时负责人和单号为空。
/// * `whole` - 是否返回整单金额。
///
/// # 返回
/// 可见已结份额始终取 `settled_total`；`whole` 为 false 时整单金额为 `None`。
///
/// # 错误
/// 不返回错误。
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
        open_invoiceable_total: whole_amount(whole, row.open_invoiceable_total),
        invoiced_total: whole_amount(whole, row.invoiced_total),
        permission_limited: !whole,
        procurement_owner_user_id: fact.and_then(|order| order.owner_user_id.clone()),
        business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
        payment_recipient: None,
        payment_guidance: None,
        entries: Vec::new(),
        source_document_no: fact.and_then(|order| {
            let number = order.document_no.trim();
            if number.is_empty() { None } else { Some(number.to_string()) }
        }),
        supplier_name: None,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::stable::StableBase;
    use erp_core::money::Amount;
    use erp_finance::entity::payable::{PayableAccountStatus, PayableSourceType};

    use super::*;

    fn amount(text: &str) -> Amount {
        Amount::from_str(text).expect("测试金额")
    }

    fn sample_row() -> PayableAccountRow {
        PayableAccountRow {
            id: "pa-1".into(),
            stable: StableBase::new(PayableAccountStatus::Open, "buyer"),
            source_document_id: "po-1".into(),
            supplier_id: "sup-1".into(),
            source_type: PayableSourceType::PurchaseOrder,
            gross_total: amount("100.00"),
            settled_total: amount("20.00"),
            open_total: amount("80.00"),
            invoiceable_total: amount("100.00"),
            invoiced_total: amount("15.00"),
            open_invoiceable_total: amount("85.00"),
            version: 3,
            created_at: 1,
        }
    }

    /// 整单资格必须带回真实可收票余额；部分授权为 null，不能写成零。
    #[test]
    fn whole_access_returns_open_invoiceable_total_and_partial_returns_null() {
        let fact = LinkedPurchaseFact {
            owner_user_id: Some("buyer-1".into()),
            business_org_unit_id: "org-1".into(),
            version: 2,
            document_no: "PO-1".into(),
        };
        let whole = cut_payable_account_row(&sample_row(), Some(&fact), true);
        assert_eq!(whole.open_invoiceable_total, Some(amount("85.00")));
        assert_eq!(whole.invoiced_total, Some(amount("15.00")));
        assert_eq!(whole.open_total, Some(amount("80.00")));
        assert!(!whole.permission_limited);
        let whole_json = serde_json::to_value(&whole).expect("整单行必须可序列化");
        assert_eq!(whole_json["open_invoiceable_total"], "85.00");
        assert_eq!(whole_json["invoiced_total"], "15.00");

        let partial = cut_payable_account_row(&sample_row(), Some(&fact), false);
        assert_eq!(partial.open_invoiceable_total, None);
        assert_eq!(partial.invoiced_total, None);
        assert_eq!(partial.open_total, None);
        assert!(partial.permission_limited);
        let partial_json = serde_json::to_value(&partial).expect("部分授权行必须可序列化");
        assert!(partial_json.get("open_invoiceable_total").is_some());
        assert!(partial_json["open_invoiceable_total"].is_null());
        assert!(partial_json.get("invoiced_total").is_some());
        assert!(partial_json["invoiced_total"].is_null());
    }

    /// 窄摘要与原列表汇总同口径；没有负责人时保留未知归属并保证可加总。
    #[test]
    fn database_account_summary_keeps_full_totals_and_unknown_ownership() {
        let rows = vec![
            AccountSummaryRow {
                id: "a".into(),
                order_id: "po-a".into(),
                owner_user_id: Some("buyer".into()),
                gross_total: amount("100.01"),
                settled_total: amount("60.01"),
            },
            AccountSummaryRow {
                id: "b".into(),
                order_id: "settlement-b".into(),
                owner_user_id: None,
                gross_total: amount("40.02"),
                settled_total: amount("20.02"),
            },
        ];
        let summary = payable_summary(&rows, "v").unwrap();
        assert_eq!(summary.whole_total, Some(amount("140.03")));
        assert_eq!(summary.grouped.len(), 1);
        assert_eq!(summary.grouped[0].visible_share, amount("60.01"));
        assert_eq!(summary.unassigned, amount("20.02"));
        assert_eq!(summary.scope_version, "v");
        assert!(!summary.permission_limited);
        assert_eq!(payable_summary(&[], "empty").unwrap().whole_total, Some(Amount::zero()));
    }
}
