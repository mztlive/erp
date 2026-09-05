//! 应付往来子账列表、详情与创建编排。

use std::collections::{HashMap, HashSet};

use database::{
    AccessControlExt, NoTransaction, PartyExt, PayableExt, PurchaseOrderExt, SupplierExt,
    SupplierSettlementExt, Transactional,
};
use entities::common::time::Instant;
use entities::ids::{PartyBankAccountId, PayableAccountId, PayableEntryId, SupplierAccountId};
use entities::payable::{
    EntryDirection, PayableAccount, PayableAccountData, PayableEntry, PayableEntryData, PayableEntryType,
    PayableSourceType,
};
use id_generator::next_id;
use validator::Validate;

use super::display::{resolve_source_document_no, resolve_supplier_display};
use super::dto::{
    CreatePayableAccountRequest, PageView, PayableAccountListParams, PayableAccountSummaryView,
    PayableAccountView, PaymentRecipientRevealView, RevealPaymentRecipientRequest, SortDir,
};
use super::mapping::{
    payment_recipient_view, resolve_current_payment_recipient, resolve_optional_payment_recipient_for_read,
    zero_amount,
};
use super::payment_task;
use super::{PayableAccountFilter, PayableService};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::party::SensitiveDataCodec;

impl PayableService {
    // -----------------------------------------------------------------------
    // 应付往来子账
    // -----------------------------------------------------------------------

    /// 分页查询应付往来子账列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`supplier_id`/`source_type`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn payable_account_list(
        &self,
        params: &PayableAccountListParams,
    ) -> Result<PageView<PayableAccountSummaryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PayableAccountFilter {
            supplier_id: query.supplier_id,
            source_type: query.source_type,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .payable_accounts()
            .search_payable_accounts(&filter, &mut NoTransaction)
            .await?;
        let account_ids = page
            .items
            .iter()
            .map(|row| PayableAccountId::new(row.id.clone()))
            .collect::<Vec<_>>();
        let mut entries_by_account = HashMap::<String, Vec<PayableEntry>>::new();
        for entry in self
            .db
            .payable_entries()
            .find_entries_by_accounts(&account_ids, &mut NoTransaction)
            .await?
        {
            entries_by_account
                .entry(entry.payable_account_id.to_string())
                .or_default()
                .push(entry);
        }
        for entries in entries_by_account.values_mut() {
            entries.sort_unstable_by_key(|entry| entry.source_sequence);
        }

        let supplier_ids = page
            .items
            .iter()
            .map(|row| SupplierAccountId::new(row.supplier_id.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let suppliers = self
            .db
            .supplier_accounts()
            .find_accounts_by_ids(&supplier_ids, &mut NoTransaction)
            .await?;
        let party_ids = suppliers
            .iter()
            .map(|supplier| supplier.party_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let parties = self
            .db
            .parties()
            .find_parties_by_ids(&party_ids, &mut NoTransaction)
            .await?;
        let revision_ids = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let revisions = self
            .db
            .party_revisions()
            .find_revisions_by_ids(&revision_ids, &mut NoTransaction)
            .await?;
        let supplier_by_id = suppliers
            .into_iter()
            .map(|supplier| (supplier.base.id.clone(), supplier))
            .collect::<HashMap<_, _>>();
        let party_by_id = parties
            .into_iter()
            .map(|party| (party.base.id.clone(), party))
            .collect::<HashMap<_, _>>();
        let revision_by_id = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();

        let purchase_order_ids = page
            .items
            .iter()
            .filter(|row| row.source_type == PayableSourceType::PurchaseOrder)
            .map(|row| row.source_document_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let settlement_ids = page
            .items
            .iter()
            .filter(|row| row.source_type == PayableSourceType::SupplierSettlement)
            .map(|row| row.source_document_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let purchase_order_nos = self
            .db
            .purchase_order()
            .find_orders_by_ids(&purchase_order_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|order| (order.base.id.clone(), order.purchase_no))
            .collect::<HashMap<_, _>>();
        let settlement_nos = self
            .db
            .supplier_settlement_statements()
            .find_statements_by_ids(&settlement_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|statement| (statement.base.id.clone(), statement.statement_no))
            .collect::<HashMap<_, _>>();

        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let source_document_no = match row.source_type {
                PayableSourceType::PurchaseOrder => purchase_order_nos.get(&row.source_document_id),
                PayableSourceType::SupplierSettlement => settlement_nos.get(&row.source_document_id),
            }
            .filter(|value| !value.trim().is_empty())
            .cloned();
            let supplier = supplier_by_id.get(&row.supplier_id);
            let supplier_no = supplier.map(|value| value.supplier_no.clone());
            let supplier_name = supplier
                .and_then(|value| party_by_id.get(value.party_id.as_ref()))
                .and_then(|party| party.stable.current_revision_id.as_ref())
                .and_then(|revision_id| revision_by_id.get(revision_id))
                .map(|revision| revision.legal_name.clone());
            let entries = entries_by_account
                .remove(&row.id)
                .unwrap_or_default()
                .into_iter()
                .map(|entry| crate::payable::dto::PayableEntryView {
                    id: entry.base.id,
                    entry_type: entry.entry_type,
                    direction: entry.direction,
                    amount: entry.amount,
                    due_date: entry.due_date,
                    source_document_no: (entry.source_document_id == row.source_document_id)
                        .then(|| source_document_no.clone())
                        .flatten(),
                    source_document_id: entry.source_document_id,
                    source_sequence: entry.source_sequence,
                    posted_at: entry.posted_at,
                })
                .collect();
            views.push(PayableAccountSummaryView {
                id: row.id,
                source_document_id: row.source_document_id,
                source_document_no,
                supplier_id: row.supplier_id,
                supplier_no,
                supplier_name,
                source_type: row.source_type,
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
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询应付往来子账详情（子账 + 分录）。
    ///
    /// # 参数
    /// * `id` - 应付往来子账 ID
    ///
    /// # 返回
    /// 返回完整应付台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    pub async fn payable_account_detail(&self, id: &str) -> Result<PayableAccountView> {
        self.payable_account_view(id.to_string(), true).await
    }

    /// 在付款任务责任校验后揭示当前默认收款账号。
    ///
    /// 查看不修改任务版本；页面可在核对账号后继续使用同一任务版本提交付款。
    /// 每次成功揭示均写入敏感信息审计。
    ///
    /// # 参数
    /// * `id` - 当前付款任务绑定的应付往来子账 ID
    /// * `req` - 任务身份、任务版本与页面所见收款账户
    /// * `actor` - 当前操作人
    /// * `sensitive_data` - 应用启动期共享的敏感数据编解码器
    ///
    /// # 返回
    /// 返回完整收款账号和对应账户事实行主键。
    ///
    /// # 错误
    /// 任务责任、版本、账户身份或敏感密文不合法时失败关闭。
    pub async fn reveal_payment_recipient(
        &self,
        id: &str,
        req: RevealPaymentRecipientRequest,
        actor: &AuditActor,
        sensitive_data: &SensitiveDataCodec,
    ) -> Result<PaymentRecipientRevealView> {
        req.validate()?;
        let expected_task_version = crate::work_item::expected_task_version(&req.expected_task_version)?;
        let account_id = PayableAccountId::new(id);
        let (_, account) = payment_task::authorize_payment_execution(
            &self.db,
            &req.work_item_id,
            expected_task_version,
            Some(&account_id),
            actor,
            &mut NoTransaction,
        )
        .await?;
        let recipient =
            resolve_current_payment_recipient(&self.db, &account.supplier_id, &mut NoTransaction).await?;
        if !recipient.matches_expected(
            &PartyBankAccountId::new(req.expected_bank_account_id.trim()),
            req.expected_bank_account_version,
        ) {
            return Err(Error::ConflictError(
                "供应商收款账户已变化，请刷新付款任务并重新核对".to_string(),
            ));
        }
        let account_number = sensitive_data.decrypt(&recipient.account_number_ciphertext)?;
        let audit = actor.clone().resource_log(
            "party_bank_account.reveal_for_payment",
            "party_bank_account",
            recipient.base.id.clone(),
        )?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(PaymentRecipientRevealView {
            bank_account_id: recipient.base.id,
            account_number,
        })
    }

    /// 建立应付往来子账与原始应付分录（跨集合事务写入）。
    ///
    /// 校验来源单据存在（D15 `purchase_orders()`）；同事务写入子账与分录，
    /// 保证「子账 + 原始应付」原子可见（数据模型 §6.9）。业务幂等唯一
    /// `(payable_account_id, source_fact_type, source_document_id,
    /// source_revision_id, entry_type, source_sequence)` 由唯一索引保证。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建子账的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源采购单不存在
    /// * `ConflictError` - 业务唯一键重复
    pub async fn create_payable_account(
        &self,
        req: CreatePayableAccountRequest,
        actor: &AuditActor,
    ) -> Result<PayableAccountView> {
        req.validate()?;
        self.db
            .purchase_orders()
            .find_by_id(&req.source_document_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;

        let account_id = PayableAccountId::new(next_id());
        let entry_id = PayableEntryId::new(next_id());
        let account = PayableAccount::new(
            account_id.clone(),
            PayableAccountData {
                source_document_id: req.source_document_id.clone(),
                supplier_id: req.supplier_id.clone(),
                source_type: req.source_type,
                gross_total: req.gross_total,
                settled_total: zero_amount(),
                invoiceable_total: req.invoiceable_total.unwrap_or(req.gross_total),
                invoiced_total: zero_amount(),
            },
            actor.id(),
        )?;
        let entry = PayableEntry::new(
            entry_id,
            PayableEntryData {
                payable_account_id: account_id.clone(),
                entry_type: PayableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: account.gross_total,
                due_date: req.due_date,
                source_fact_type: "purchase_order".to_string(),
                source_document_id: req.source_document_id,
                source_revision_id: req.source_revision_id,
                source_sequence: req.source_sequence,
                posted_at: Instant::now(),
            },
        )?;
        let audit = actor.clone().resource_log(
            "payable_account.create",
            "payable_account",
            account_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.payable()
                        .create_payable_with_entry(&account, &entry, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.payable_account_detail(&account_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配应付往来子账视图。
    ///
    /// # 参数
    /// * `id` - 子账 ID
    /// * `include_payment_recipient` - 是否加载任务/详情所需的当前收款账户
    ///
    /// # 返回
    /// 返回完整应付台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    async fn payable_account_view(
        &self,
        id: String,
        include_payment_recipient: bool,
    ) -> Result<PayableAccountView> {
        let account = self
            .db
            .payable_accounts()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
        let source_document_no = resolve_source_document_no(&self.db, &account).await?;
        let account_source_id = account.source_document_id.clone();
        let entries = self
            .db
            .payable_entries()
            .find_entries_by_account(&account.base.id.clone().into(), &mut NoTransaction)
            .await?
            .into_iter()
            .map(|entry| crate::payable::dto::PayableEntryView {
                id: entry.base.id.clone(),
                entry_type: entry.entry_type,
                direction: entry.direction,
                amount: entry.amount,
                due_date: entry.due_date,
                source_document_id: entry.source_document_id.clone(),
                source_document_no: (entry.source_document_id == account_source_id)
                    .then(|| source_document_no.clone())
                    .flatten(),
                source_sequence: entry.source_sequence,
                posted_at: entry.posted_at,
            })
            .collect();
        let (supplier_no, supplier_name) =
            resolve_supplier_display(&self.db, account.supplier_id.as_ref()).await?;
        let payment_recipient = if include_payment_recipient {
            resolve_optional_payment_recipient_for_read(&self.db, &account.supplier_id, &mut NoTransaction)
                .await?
                .as_ref()
                .map(payment_recipient_view)
        } else {
            None
        };
        Ok(PayableAccountView {
            id: account.base.id.clone(),
            source_document_id: account.source_document_id,
            source_document_no,
            supplier_id: account.supplier_id.to_string(),
            supplier_no,
            supplier_name,
            payment_recipient,
            source_type: account.source_type,
            gross_total: account.gross_total,
            settled_total: account.settled_total,
            open_total: account.open_total,
            invoiceable_total: account.invoiceable_total,
            invoiced_total: account.invoiced_total,
            open_invoiceable_total: account.open_invoiceable_total,
            status: account.stable.status(),
            version: account.base.version,
            created_at: account.base.created_at,
            entries,
        })
    }
}
