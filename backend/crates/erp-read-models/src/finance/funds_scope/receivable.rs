//! 应收子账范围查询。

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::repository::ReceivableExt;
use erp_sales::repository::SalesOrderExt;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::authorization::*;
use super::rows::*;
use crate::{Error, Result};

mod facts;
mod list;

use list::{receivable_page_rows, receivable_summary};

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
                    let whole = true;
                    let visible = this.receivable_visible_share(&account.base.id, executor).await?;
                    let mut row = ScopedReceivableAccountRow {
                        id: account.base.id.clone(),
                        sales_order_id: account.sales_order_id.to_string(),
                        sales_order_no: order.order_no.clone(),
                        account_seq: account.account_seq,
                        status: account.stable.status(),
                        created_at: account.base.created_at,
                        version: account.base.version,
                        customer_id: account.customer_id.to_string(),
                        customer_name: None,
                        counterparty_party_id: account.counterparty_party_id.to_string(),
                        counterparty_party_name: None,
                        visible_settled_share: visible,
                        gross_total: whole_amount(whole, account.gross_total),
                        settled_total: whole_amount(whole, account.settled_total),
                        open_total: whole_amount(whole, account.open_total),
                        open_invoiceable_total: whole_amount(whole, account.open_invoiceable_total),
                        permission_limited: !whole,
                        sales_owner_user_id: Some(order.sales_owner_user_id.clone()),
                        business_org_unit_id: Some(order.business_org_unit_id.clone()),
                        entries: Vec::new(),
                    };
                    this.finish_receivable_display(std::slice::from_mut(&mut row), executor).await?;
                    let parts = vec![facts.version_part()];
                    let version = scope_version(&authorization.context, &parts);
                    Ok(FundsScopedResult {
                        data: row,
                        scope_version: version,
                        policy_version: authorization.context.policy_version,
                        organization_version: authorization.context.organizations.version,
                        as_of: authorization.context.as_of.as_utc().to_rfc3339(),
                        empty_reason: None,
                        scope_summary: "应收子账继承所属销售来源边界",
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
        checked_revalidated(
            expected,
            || self.snapshot_receivable_accounts(params, query, actor),
            || self.revalidate_receivable_accounts(query, actor),
        )
        .await
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

    /// 来源授权和业务条件进入数据库，再按最终匹配结果分页与计数。
    pub(super) async fn load_receivable_accounts(
        &self,
        params: &erp_finance::dto::receivable::ReceivableAccountListParams,
        query: &erp_finance::dto::receivable::ReceivableAccountListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedReceivableAccountRow>> {
        let (_access, authorization) = self.resolve(actor, "receivable_account", "list", executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "应收子账无可见范围"));
        }
        let condition = self.receivable_condition(query, executor).await?;
        let snapshot =
            self.receivable_database_snapshot(query, &authorization, &condition, true, executor).await?;
        let version = snapshot.version(&authorization)?;
        ensure_version(params.scope_version.as_deref(), &version)?;
        let ids = snapshot.summary.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let display_ids = snapshot.items.iter().map(|item| item.row.id.clone()).collect();
        let mut facts = self.receivable_facts(&ids, &display_ids, executor).await?;
        let mut items = receivable_page_rows(&snapshot.items, &mut facts);
        self.finish_receivable_names(&mut items, executor).await?;
        let summary = receivable_summary(&snapshot.summary, &facts, &version)?;
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
            scope_summary: "应收子账继承所属销售来源边界",
            ownership_basis: "current_sales_owner_and_register_operator",
        })
    }

    /// 独立新事务重读动作、授权和完整匹配版本，不再生成页面或金额汇总。
    async fn revalidate_receivable_accounts(
        &self,
        query: &erp_finance::dto::receivable::ReceivableAccountListQuery,
        actor: &AuditActor,
    ) -> Result<String> {
        let this = self.clone();
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (_, authorization) =
                        this.resolve(&actor, "receivable_account", "list", executor).await?;
                    if authorization.empty() {
                        return Ok(authorization.context.scope_version);
                    }
                    let condition = this.receivable_condition(&query, executor).await?;
                    this.receivable_database_snapshot(&query, &authorization, &condition, false, executor)
                        .await?
                        .version(&authorization)
                })
            })
            .await
    }

    /// 计算子账获授权核销份额；未分配与未授权份额不计入，禁止差额推导。
    pub(super) async fn receivable_visible_share(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        let facts = self.receivable_facts(&[account_id.to_string()], &Default::default(), executor).await?;
        Ok(facts.share(account_id))
    }
}
