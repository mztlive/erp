//! 资金命令范围守卫。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, ReceivableExt, SupplierPaymentFilter};
use erp_procurement::PurchaseAccess;
use erp_sales::repository::SalesOrderExt;
use persistence_core::Executor;

use super::allocation::*;
use super::authorization::*;
use super::invoice::*;
use super::payment::*;
use super::receipt::*;
use super::rows::*;
use crate::{Error, Result};

impl FundsAccess {
    /// 命令守卫：同一事务内按详情动作独立解析并重验责任新鲜度。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `resource` - 资金资源（仅读动作已注册的七资源）
    /// * `id` - 命令目标单据主键
    /// * `purchase_access` - 采购关联资源必填的组合层采购访问器
    /// * `executor` - 命令原事务执行器，不得另开事务
    ///
    /// # 返回
    /// 可见时成功；不可见与不存在统一为 NotFound，未注册命令资源为 Forbidden。
    ///
    /// # 错误
    /// 授权解析、事实读取或版本校验失败时拒绝命令。
    ///
    /// # 关键业务约束
    /// 正式审批准入不变；本守卫只做读包含与责任新鲜度重验，不授予审批资格。
    pub async fn guard_funds_command(
        &self,
        actor: &AuditActor,
        resource: &str,
        id: &str,
        purchase_access: Option<&PurchaseAccess>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match resource {
            "receivable_account" => self.guard_receivable_account(actor, id, executor).await,
            "customer_receipt" => self.guard_customer_receipt(actor, id, executor).await,
            "invoice" => {
                let access =
                    purchase_access.ok_or_else(|| Error::Forbidden("发票命令缺少采购访问器".into()))?;
                self.guard_invoice(actor, id, access, executor).await
            },
            "sales_invoice_request" => self.guard_request(actor, id, executor).await,
            "payable_account" => {
                let access =
                    purchase_access.ok_or_else(|| Error::Forbidden("应付命令缺少采购访问器".into()))?;
                self.guard_payable_account(actor, id, access, executor).await
            },
            "supplier_payment" => {
                let access =
                    purchase_access.ok_or_else(|| Error::Forbidden("付款命令缺少采购访问器".into()))?;
                self.guard_supplier_payment(actor, id, access, executor).await
            },
            _ => Err(Error::Forbidden("该资源不支持资金命令守卫".into())),
        }
    }

    /// 新建命令守卫：关联销售/采购单必须在操作人当前授权集合内。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `sales_order_ids` - 新单据关联的销售单主键集合
    /// * `purchase_order_ids` - 新单据关联的采购单主键集合
    /// * `purchase_access` - 含采购关联时必填的组合层采购访问器
    /// * `executor` - 命令原事务执行器
    ///
    /// # 返回
    /// 全部关联单据均在授权集合内时成功，否则为 Forbidden。
    pub async fn guard_funds_create(
        &self,
        actor: &AuditActor,
        sales_order_ids: &[String],
        purchase_order_ids: &[String],
        purchase_access: Option<&PurchaseAccess>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !sales_order_ids.is_empty() {
            let (_, authorization) = self.resolve(actor, "receivable_account", "list", executor).await?;
            let allowed = self.authorized_sales_ids(&authorization, executor).await?;
            if let Some(allowed) = allowed
                && !sales_order_ids.iter().all(|id| allowed.iter().any(|item| item == id))
            {
                return Err(Error::Forbidden("关联销售单超出当前数据范围".into()));
            }
        }
        if !purchase_order_ids.is_empty() {
            let access = purchase_access.ok_or_else(|| Error::Forbidden("新建命令缺少采购访问器".into()))?;
            let (_, authorization) =
                self.resolve_with_purchase(actor, "payable_account", "list", access, executor).await?;
            let scope = authorization.purchase_scope.clone().unwrap_or_default();
            let allowed = self.authorized_purchase_ids(access, &scope, executor).await?;
            if let Some(allowed) = allowed
                && !purchase_order_ids.iter().all(|id| allowed.iter().any(|item| item == id))
            {
                return Err(Error::Forbidden("关联采购单超出当前数据范围".into()));
            }
        }
        Ok(())
    }

    /// 应收子账命令守卫：详情动作重验关联销售当前负责人与登记经办。
    pub(super) async fn guard_receivable_account(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (access, _) = self.resolve(actor, "receivable_account", "detail", executor).await?;
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(id, executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(account.sales_order_id.as_ref(), executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
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
        Ok(())
    }

    /// 回款命令守卫：详情动作重验核销关联销售与经办人新鲜度。
    pub(super) async fn guard_customer_receipt(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        use erp_finance::repository::CustomerReceiptFilter;
        let (access, authorization) = self.resolve(actor, "customer_receipt", "detail", executor).await?;
        let filter = CustomerReceiptFilter {
            keyword_ids: None,
            receipt_ids: Some(vec![id.to_string()]),
            pending_entry_ids: Vec::new(),
            receipt_no: None,
            counterparty_party_id: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.customer_receipts().search_customer_receipts(&filter, executor).await?;
        let row = page.items.into_iter().next().ok_or_else(|| Error::NotFound("客户回款单不存在".into()))?;
        let links = self.receipt_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let allowed = self
            .authorized_sales_ids(&authorization, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let empty_links: Vec<ReceiptLink> = Vec::new();
        let row_links = links.get(&row.id).unwrap_or(&empty_links);
        let tuples = receipt_tuples(&row, row_links, &facts);
        let matched = matched_orders(&tuples, &allowed);
        let whole = authorization.whole();
        let visible = row_visible(&access, &tuples, &[], &[])?;
        let unlinked = row_links.iter().all(|link| link.order.is_none());
        if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
            return Err(Error::NotFound("客户回款单不存在".into()));
        }
        Ok(())
    }

    /// 发票命令守卫：双方向详情动作重验分配关联与登记人新鲜度。
    pub(super) async fn guard_invoice(
        &self,
        actor: &AuditActor,
        id: &str,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        use erp_finance::entity::receivable::InvoiceDirection;
        use erp_finance::repository::InvoiceFilter;
        let (access, authorization) =
            self.resolve_dual(actor, "invoice", "detail", purchase_access, executor).await?;
        let filter = InvoiceFilter {
            keyword_ids: None,
            invoice_ids: Some(vec![id.to_string()]),
            invoice_direction: None,
            invoice_kind: None,
            party_id: None,
            invoice_no: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.invoices().search_invoices(&filter, executor).await?;
        let row = page.items.into_iter().next().ok_or_else(|| Error::NotFound("发票不存在".into()))?;
        let sales_links = self.sales_invoice_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let purchase_links =
            self.purchase_invoice_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let (sales_facts, purchase_facts) =
            self.invoice_fact_maps(&sales_links, &purchase_links, executor).await?;
        let empty_sales: Vec<SalesInvoiceLink> = Vec::new();
        let empty_purchase: Vec<PurchaseInvoiceLink> = Vec::new();
        let sales = sales_links.get(&row.id).unwrap_or(&empty_sales);
        let purchase = purchase_links.get(&row.id).unwrap_or(&empty_purchase);
        let mut combined = invoice_sales_tuples(&row, sales, &sales_facts);
        combined.extend(invoice_purchase_tuples(&row, purchase, &purchase_facts));
        let sales_allowed = self
            .authorized_sales_ids(&authorization, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let purchase_allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let sales_part: Vec<OrderTuple> = invoice_sales_tuples(&row, sales, &sales_facts);
        let purchase_part: Vec<OrderTuple> = invoice_purchase_tuples(&row, purchase, &purchase_facts);
        let matched_any = !matched_orders(&sales_part, &sales_allowed).is_empty()
            || !matched_orders(&purchase_part, &purchase_allowed).is_empty();
        let whole = match row.invoice_direction {
            InvoiceDirection::Sales => authorization.whole(),
            InvoiceDirection::Purchase => purchase_whole(&authorization),
        };
        let operators = vec![row.stable.created_by.clone()];
        let visible = row_visible(&access, &combined, &operators, &[])?;
        let unlinked = combined.iter().all(|(order, _, _, _, _)| order.is_none());
        let empty = sales.is_empty() && purchase.is_empty();
        if !keep_row(visible, whole, matched_any, empty || unlinked) {
            return Err(Error::NotFound("发票不存在".into()));
        }
        Ok(())
    }

    /// 开票申请命令守卫：详情动作重验关联销售、申请人与处理人新鲜度。
    pub(super) async fn guard_request(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (access, authorization) =
            self.resolve(actor, "sales_invoice_request", "detail", executor).await?;
        let row = self
            .db
            .sales_invoice_requests()
            .find_by_id(id, executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        let facts = self.sales_fact_map(&[row.sales_order_id.to_string()], executor).await?;
        let fact = facts
            .get(&row.sales_order_id.to_string())
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        let allowed = self.authorized_sales_ids(&authorization, executor).await?;
        if allowed.is_some_and(|list| !list.contains(&row.sales_order_id.to_string())) {
            return Err(Error::NotFound("开票申请不存在".into()));
        }
        let handlers = self
            .work_item_handlers(&row.work_item_id.clone().into_iter().collect::<Vec<_>>(), executor)
            .await?;
        let row_facts = FundsLinkedFacts {
            owner_user_id: Some(fact.owner_user_id.clone()),
            business_org_unit_id: Some(fact.business_org_unit_id.clone()),
            operator_user_ids: vec![row.created_by.clone()],
            secondary_operator_user_ids: row
                .work_item_id
                .as_ref()
                .and_then(|item| handlers.get(item).cloned())
                .into_iter()
                .collect(),
            linked_document_id: row.sales_order_id.to_string(),
            linked_document_version: fact.version,
        };
        if !Self::allows(&access, &row_facts)? {
            return Err(Error::NotFound("开票申请不存在".into()));
        }
        Ok(())
    }

    /// 应付子账命令守卫：详情动作重验来源采购当前负责人新鲜度。
    pub(super) async fn guard_payable_account(
        &self,
        actor: &AuditActor,
        id: &str,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
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
        Ok(())
    }

    /// 付款命令守卫：详情动作重验核销关联采购与付款经办新鲜度。
    pub(super) async fn guard_supplier_payment(
        &self,
        actor: &AuditActor,
        id: &str,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (access, authorization) = self
            .resolve_with_purchase(actor, "supplier_payment", "detail", purchase_access, executor)
            .await?;
        let filter = SupplierPaymentFilter {
            keyword_ids: Some(vec![id.to_string()]),
            keyword: None,
            keyword_supplier_ids: Vec::new(),
            payment_no: None,
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.supplier_payments().search_supplier_payments(&filter, executor).await?;
        let row =
            page.items.into_iter().next().ok_or_else(|| Error::NotFound("供应商付款单不存在".into()))?;
        let links = self.payment_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.purchase_fact_map(&order_ids, executor).await?;
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let empty_links: Vec<PaymentLink> = Vec::new();
        let row_links = links.get(&row.id).unwrap_or(&empty_links);
        let tuples = payment_tuples(&row, row_links, &facts);
        let matched = matched_orders(&tuples, &allowed);
        let whole = purchase_whole(&authorization);
        let operators = self.payment_operators(std::slice::from_ref(&row.id), true, executor).await?;
        let doc_operators = operators.get(&row.id).cloned().unwrap_or_default();
        let visible = row_visible(&access, &tuples, &doc_operators, &[])?;
        let unlinked = row_links.iter().all(|link| link.order.is_none());
        if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
            return Err(Error::NotFound("供应商付款单不存在".into()));
        }
        Ok(())
    }
}
