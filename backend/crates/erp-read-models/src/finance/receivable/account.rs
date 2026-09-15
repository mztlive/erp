//! 应收往来子账列表、详情与创建编排。

use std::collections::{HashMap, HashSet};

use erp_core::ids::{ReceivableAccountId, ReceivableEntryId, SalesOrderId};
use erp_core::money::Amount;
use erp_finance::dto::receivable::{
    PageView, ReceivableAccountListParams, ReceivableAccountSummaryView, SortDir,
};
use erp_finance::entity::receivable::{EntryDirection, ReceivableEntry};
use erp_finance::repository::{ReceivableAccountFilter, ReceivableExt};
use erp_sales::repository::SalesOrderExt;
use persistence_core::NoTransaction;
use validator::Validate;

use super::ReceivableReadService;
use super::snapshot::{invoice_fact_views, load_receivable_snapshot, receipt_fact_views, zero_amount};
use crate::finance::dto::ReceivableAccountView;
use crate::{Error, Result};

impl ReceivableReadService {
    /// 分页查询应收往来子账列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`customer_id`/`counterparty_party_id`/`status`/
    ///   `sales_order_id`/`review_status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn receivable_account_list(
        &self,
        params: &ReceivableAccountListParams,
    ) -> Result<PageView<ReceivableAccountSummaryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let keyword_ids = crate::finance::search::keyword_ids(
            &self.db,
            query.q.as_deref(),
            erp_finance::repository::keyword::FinanceSearchTarget::Receivable,
        )
        .await?;
        let filter = ReceivableAccountFilter {
            keyword_ids,
            keyword: None,
            keyword_sales_order_ids: Vec::new(),
            keyword_party_ids: Vec::new(),
            account_id: query.account_id,
            customer_id: query.customer_id,
            counterparty_party_id: query.counterparty_party_id,
            status: query.status,
            sales_order_id: query.sales_order_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page =
            self.db.receivable_accounts().search_receivable_accounts(&filter, &mut NoTransaction).await?;
        let account_ids =
            page.items.iter().map(|row| ReceivableAccountId::new(row.id.clone())).collect::<Vec<_>>();
        let mut entries_by_account = HashMap::<String, Vec<ReceivableEntry>>::new();
        let entries =
            self.db.receivable_entries().find_entries_by_accounts(&account_ids, &mut NoTransaction).await?;
        let decrease_entry_ids = entries
            .iter()
            .filter(|entry| entry.direction == EntryDirection::Decrease)
            .map(|entry| ReceivableEntryId::new(entry.base.id.clone()))
            .collect::<Vec<_>>();
        let mut offset_by_increase = HashMap::<String, Amount>::new();
        for offset in self
            .db
            .receivable_entry_offsets()
            .find_offsets_by_decreases(&decrease_entry_ids, &mut NoTransaction)
            .await?
        {
            let total =
                offset_by_increase.entry(offset.increase_entry_id.to_string()).or_insert_with(zero_amount);
            *total = total.checked_add(offset.offset_amount);
        }
        for entry in entries {
            entries_by_account.entry(entry.receivable_account_id.to_string()).or_default().push(entry);
        }
        for entries in entries_by_account.values_mut() {
            entries.sort_unstable_by_key(|entry| entry.source_sequence);
        }

        let sales_order_ids = page
            .items
            .iter()
            .map(|row| SalesOrderId::new(row.sales_order_id.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let sales_orders =
            self.db.sales_orders().find_orders_by_ids(&sales_order_ids, &mut NoTransaction).await?;
        let revision_ids = sales_orders
            .iter()
            .map(|order| {
                order
                    .stable
                    .current_revision_id
                    .clone()
                    .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前正式版本".to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        let revisions =
            self.db.sales_order_revisions().find_revisions_by_ids(&revision_ids, &mut NoTransaction).await?;
        let sales_order_by_id =
            sales_orders.into_iter().map(|order| (order.base.id.clone(), order)).collect::<HashMap<_, _>>();
        let revision_by_id = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();

        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let order = sales_order_by_id
                .get(&row.sales_order_id)
                .ok_or_else(|| Error::NotFound("应收账户来源销售单不存在".to_string()))?;
            let revision_id = order
                .stable
                .current_revision_id
                .as_ref()
                .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前正式版本".to_string()))?;
            let revision = revision_by_id
                .get(revision_id)
                .ok_or_else(|| Error::NotFound("来源销售单当前正式版本不存在".to_string()))?;
            let entries = entries_by_account
                .remove(&row.id)
                .unwrap_or_default()
                .into_iter()
                .map(|entry| erp_finance::dto::receivable::ReceivableEntryView {
                    offset_total: offset_by_increase.get(&entry.base.id).copied().unwrap_or_else(zero_amount),
                    id: entry.base.id,
                    entry_type: entry.entry_type,
                    direction: entry.direction,
                    amount: entry.amount,
                    due_date: entry.due_date,
                    source_document_id: entry.source_document_id,
                    source_sequence: entry.source_sequence,
                    posted_at: entry.posted_at,
                })
                .collect();
            views.push(ReceivableAccountSummaryView {
                id: row.id,
                sales_order_id: row.sales_order_id,
                sales_order_no: order.order_no.clone(),
                account_seq: row.account_seq,
                customer_id: row.customer_id,
                customer_name: revision.customer_snapshot.customer_name.clone(),
                counterparty_party_id: row.counterparty_party_id,
                counterparty_party_name: revision
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                gross_total: row.gross_total,
                settled_total: row.settled_total,
                open_total: row.open_total,
                invoiceable_total: row.invoiceable_total,
                invoiced_total: row.invoiced_total,
                open_invoiceable_total: row.open_invoiceable_total,
                status: row.stable.status(),
                version: row.version,
                created_at: row.created_at,
                entries,
            });
        }
        Ok(PageView { items: views, total: page.total, page: filter.page, page_size: filter.page_size })
    }
    /// 查询应收往来子账详情（子账 + 分录 + 抵销 + 复核链）。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    ///
    /// # 返回
    /// 返回完整台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn receivable_account_detail(&self, id: &str) -> Result<ReceivableAccountView> {
        self.receivable_account_view(id.to_string()).await
    }
    /// 装配应收往来子账详情视图。
    ///
    /// # 参数
    /// * `id` - 子账 ID
    ///
    /// # 返回
    /// 返回完整台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    pub async fn receivable_account_view(&self, id: String) -> Result<ReceivableAccountView> {
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        let snapshot = load_receivable_snapshot(&self.db, &account, &mut NoTransaction).await?;
        let offsets = snapshot
            .entries
            .iter()
            .filter(|entry| entry.direction == EntryDirection::Decrease)
            .map(|entry| entry.base.id.clone().into())
            .collect::<Vec<ReceivableEntryId>>();
        let mut offset_map: std::collections::HashMap<String, Amount> = std::collections::HashMap::new();
        for offset in
            self.db.receivable_entry_offsets().find_offsets_by_decreases(&offsets, &mut NoTransaction).await?
        {
            let key = offset.increase_entry_id.to_string();
            let total = offset_map.entry(key).or_insert_with(zero_amount);
            *total = total.checked_add(offset.offset_amount);
        }
        let entry_views = snapshot
            .entries
            .iter()
            .map(|entry| {
                let offset_total = offset_map.get(&entry.base.id).copied().unwrap_or_else(zero_amount);
                erp_finance::dto::receivable::ReceivableEntryView {
                    id: entry.base.id.clone(),
                    entry_type: entry.entry_type,
                    direction: entry.direction,
                    amount: entry.amount,
                    due_date: entry.due_date,
                    source_document_id: entry.source_document_id.clone(),
                    source_sequence: entry.source_sequence,
                    posted_at: entry.posted_at,
                    offset_total,
                }
            })
            .collect();
        let receipt_facts = receipt_fact_views(&snapshot);
        let invoice_facts = invoice_fact_views(&snapshot);

        Ok(ReceivableAccountView {
            id: account.base.id.clone(),
            sales_order_id: account.sales_order_id.to_string(),
            sales_order_no: snapshot.sales_order_no.clone(),
            sales_order_revision_no: snapshot.sales_order_revision_no,
            sales_order_snapshot_at: snapshot.sales_order_snapshot_at,
            account_seq: account.account_seq,
            source_sales_order_revision_id: account.source_sales_order_revision_id.to_string(),
            current_sales_order_revision_id: snapshot.current_sales_order_revision_id.clone(),
            customer_id: account.customer_id.to_string(),
            customer_name: snapshot.customer_name.clone(),
            counterparty_party_id: account.counterparty_party_id.to_string(),
            counterparty_party_name: snapshot.counterparty_party_name.clone(),
            gross_total: account.gross_total,
            settled_total: account.settled_total,
            open_total: account.open_total,
            invoiceable_total: account.invoiceable_total,
            invoiced_total: account.invoiced_total,
            open_invoiceable_total: account.open_invoiceable_total,
            status: account.stable.status(),
            version: account.base.version,
            account_domain_version: account.base.version.to_string(),
            receipt_facts,
            invoice_facts,
            created_at: account.base.created_at,
            entries: entry_views,
        })
    }
}
