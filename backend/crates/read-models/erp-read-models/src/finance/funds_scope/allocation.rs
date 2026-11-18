//! 进项发票分配与采购关联范围查询。

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::dto::payable::{
    PurchaseInvoiceAllocationListParams, PurchaseInvoiceAllocationListQuery, PurchaseInvoiceAllocationView,
};
use erp_finance::entity::payable::AllocationAction as PayableAllocationAction;
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_permissions::{Permission as AccessPermission, payable as payable_permissions};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::authorization::{FundsReadContext, *};
use super::linked::organization_condition;
use super::repository::allocation::{
    AllocationPageRow, AllocationSnapshot, AllocationVersion, allocation_snapshot,
};
use super::repository::payable_source::source_key;
use super::rows::*;
use crate::{Error, Result};

/// 进项发票分配的授权裁剪单元。
pub(super) struct PurchaseInvoiceLink {
    /// 正反动作后的含税方向金额。
    pub(super) signed: Amount,
    /// 归属采购单；结算来源使用带类型关联键；缺失来源拒绝。
    pub(super) order: LinkedOrderId,
    /// 响应视图。
    pub(super) view: PurchaseInvoiceAllocationView,
}

impl FundsAccess {
    /// 确定进项发票分配的适用条件时，销售与采购双关联同时证明；发票双方向查询使用，单方向复用 resolve 即可。
    ///
    /// # 参数
    /// * `permission` - 目标资金入口的公共权限常量。
    ///
    /// # 返回
    /// 销售、采购及结算双方向已解析授权。
    ///
    /// # 错误
    /// 授权解析或持久化失败时拒绝。
    pub async fn resolve_dual(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        executor: &mut dyn Executor,
    ) -> Result<(FundsReadContext, FundsAuthorization)> {
        self.resolve_sources(actor, permission, true, true, executor).await
    }

    // 应用进项发票分配筛选时，进项发票分配按应付子账反查采购单；结算来源保留自身责任边界。
    //
    // 调用参数：
    // * `invoice_ids` - 发票主键。
    //
    // 保持原分配流、实际采购或结算来源类型的关联集合。
    //
    // 分配或子账读取失败时返回错误。
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
            let view = PurchaseInvoiceAllocationView {
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
            let invoice = item.invoice_id.to_string();
            links.entry(invoice).or_default().push(PurchaseInvoiceLink { signed, order, view });
        }
        Ok(links)
    }
}
impl FundsAccess {
    /// 调用此入口时，分页查询进项发票分配范围行：采购负责人与收票经办人分别查询（仅列表）。
    ///
    /// # 参数
    /// * `params` - 进项分配列表请求。
    ///
    /// # 返回
    /// 最终授权进项分配页面与同口径汇总。
    ///
    /// # 错误
    /// 参数、范围版本、授权、读取或上限检查失败时拒绝。
    pub async fn purchase_invoice_allocation_list_scoped(
        &self,
        params: &PurchaseInvoiceAllocationListParams,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;

        let snapshot = self
            .checked_purchase_invoice_allocations(params, &query, actor, params.scope_version.as_deref())
            .await?;
        Ok(snapshot)
    }

    // 交付进项发票分配前重新读取授权及完整候选事实版本；比对发现变化时拒绝原结果。
    //
    // 调用参数：
    // * `expected` - 调用方回传的范围版本；首页可为 `None`。
    //
    // 独立轻量完整复核通过后的首拍分配页面。
    //
    // 首拍预期版本或二拍完整范围版本不一致时拒绝；授权或读取失败时返回对应错误。
    pub(super) async fn checked_purchase_invoice_allocations(
        &self,
        params: &PurchaseInvoiceAllocationListParams,
        query: &PurchaseInvoiceAllocationListQuery,
        actor: &AuditActor,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        checked_revalidated(
            expected,
            || self.snapshot_purchase_invoice_allocations(params, query, actor),
            || self.allocation_fingerprint_snapshot(query, actor),
        )
        .await
    }

    // 读取进项发票分配事实时，将身份解析与业务查询放在调用方同一事务中，不复用缓存的授权结论。
    //
    // 同事务形成的最终授权进项分配页面。
    //
    // 授权解析或持久化读取失败时拒绝。
    pub(super) async fn snapshot_purchase_invoice_allocations(
        &self,
        params: &PurchaseInvoiceAllocationListParams,
        query: &PurchaseInvoiceAllocationListQuery,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_purchase_invoice_allocations(&params, &query, &actor, executor).await
                })
            })
            .await
    }

    // 读取进项发票分配时，最终授权与人员条件先于数据库分页，总数、金额和指纹采用同一集合。
    //
    // 调用参数：
    // * `params` - 原始列表请求，用于比对范围版本。
    //
    // 数据库分页、同口径计数及完整份额汇总；无可见范围时返回空页。
    //
    // 读取、范围版本或查询上限检查失败时拒绝。
    pub(super) async fn load_purchase_invoice_allocations(
        &self,
        params: &PurchaseInvoiceAllocationListParams,
        query: &PurchaseInvoiceAllocationListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        let (authorization, snapshot) = self.allocation_projection(query, actor, true, executor).await?;
        let version = allocation_scope_version(&authorization, &snapshot.versions);
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;

        let summary = snapshot.summary(&version)?;
        let items = snapshot.items.iter().map(allocation_page_row).collect();

        authorization.deliver(executor).await?;
        Ok(FundsScopedPage {
            items,
            total: snapshot.total(),
            summary,
            page: query.paging.page,
            page_size: query.paging.page_size,
            scope_version: version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "收票分配继承应付子账的采购或结算来源边界，仅返回获授权分配",
            ownership_basis: "linked_purchase_owner_and_invoice_operator",
        })
    }

    // 组装进项发票分配的聚合管道时，每拍独立解析真实来源授权与组织筛选，只读取本拍必要的聚合分支。
    async fn allocation_projection(
        &self,
        query: &PurchaseInvoiceAllocationListQuery,
        actor: &AuditActor,
        materialize: bool,
        executor: &mut dyn Executor,
    ) -> Result<(FundsAuthorization, AllocationSnapshot)> {
        let (_, authorization) = self
            .resolve_with_purchase(actor, &payable_permissions::PURCHASE_INVOICE_ALLOCATION_LIST, executor)
            .await?;
        let condition = self.purchase_invoice_condition(query, executor).await?;
        let snapshot =
            allocation_snapshot(&self.db, query, &authorization, &condition, materialize, executor).await?;
        Ok((authorization, snapshot))
    }

    // 为进项发票分配绑定范围版本时，给出前独立事务只重验完整匹配身份及来源版本，不重建当前页与金额汇总。
    async fn allocation_fingerprint_snapshot(
        &self,
        query: &PurchaseInvoiceAllocationListQuery,
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
                    let (authorization, snapshot) =
                        this.allocation_projection(&query, &actor, false, executor).await?;
                    authorization.deliver(executor).await?;
                    Ok(allocation_scope_version(&authorization, &snapshot.versions))
                })
            })
            .await
    }

    // 应用进项发票分配筛选时，收票分配关联筛选条件：采购负责人、收票经办人与组织分别精确匹配。
    //
    // 调用参数：
    // * `query` - 已规范化的进项分配查询。
    //
    // 采购或结算负责人、经办人与展开组织的精确条件。
    //
    // 组织展开读取失败时返回错误。
    pub(super) async fn purchase_invoice_condition(
        &self,
        query: &PurchaseInvoiceAllocationListQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = organization_condition(
            self.scope.as_ref(),
            query.org_unit_ids.as_ref(),
            query.include_descendants,
            executor,
        )
        .await?;
        Ok(FundsLinkedCondition {
            owner_user_ids: query.procurement_owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            operator_user_ids: query.operator_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
            secondary_operator_user_ids: None,
            org_unit_ids,
        })
    }
}

// 为进项发票分配绑定范围版本时，分配与来源版本按原完整匹配顺序形成跨页凭据。
fn allocation_scope_version(authorization: &FundsAuthorization, versions: &[AllocationVersion]) -> String {
    let mut fingerprint = DefaultHasher::new();
    authorization.context.scope_version.hash(&mut fingerprint);
    for row in versions {
        row.hash_into(&mut fingerprint);
    }
    format!("{:x}", fingerprint.finish())
}

// 生成进项发票分配视图时，当前页由来源范围完整授权的分配，沿正反动作返回方向金额。
fn allocation_page_row(row: &AllocationPageRow) -> ScopedPurchaseInvoiceAllocationRow {
    let item = &row.item;
    let signed = match item.allocation_action {
        PayableAllocationAction::Apply => item.allocated_gross_amount,
        PayableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_gross_amount),
    };
    ScopedPurchaseInvoiceAllocationRow {
        id: item.base.id.clone(),
        invoice_id: item.invoice_id.to_string(),
        invoice_no: row.invoice_no.clone(),
        payable_account_id: item.payable_account_id.to_string(),
        source_document_no: row
            .source_document_no
            .as_deref()
            .map(str::trim)
            .filter(|number| !number.is_empty())
            .map(str::to_owned),
        created_at: item.base.created_at,
        visible_allocated_amount: signed,
        allocated_gross_amount: Some(signed),
        permission_limited: false,
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::ids::{InvoiceId, PayableAccountId};
    use erp_finance::entity::payable::PurchaseInvoiceAllocation;

    use super::*;

    fn allocation_page() -> AllocationPageRow {
        AllocationPageRow {
            item: PurchaseInvoiceAllocation {
                base: BaseModel::fake(),
                invoice_id: InvoiceId::new("invoice"),
                payable_account_id: PayableAccountId::new("account"),
                allocation_seq: 2,
                allocation_action: PayableAllocationAction::Reverse,
                allocated_gross_amount: "30".parse().unwrap(),
                allocated_net_amount: "30".parse().unwrap(),
                allocated_tax_amount: Amount::zero(),
                reverses_allocation_id: None,
            },
            invoice_no: None,
            source_document_no: Some(" PO-20261009-001 ".into()),
        }
    }

    // 验证进项发票分配：实际页面映射保留冲正方向、缺失票号与真实来源单号。
    #[test]
    fn allocation_page_preserves_reverse_direction_and_missing_invoice_number() {
        let row = allocation_page();
        let item = allocation_page_row(&row);
        assert_eq!(item.visible_allocated_amount, "-30".parse().unwrap());
        assert_eq!(item.allocated_gross_amount, Some("-30".parse().unwrap()));
        assert_eq!(item.invoice_no, None);
        assert!(!item.permission_limited);
        assert_eq!(item.payable_account_id, "account");
        assert_eq!(item.source_document_no.as_deref(), Some("PO-20261009-001"));
    }

    // 验证进项发票分配：来源单号缺失或全空白时保持空名称，不把应付子账内部主键作为名称返回。
    #[test]
    fn allocation_page_omits_missing_source_number() {
        for source_document_no in [None, Some(" \t\n ".into())] {
            let mut row = allocation_page();
            row.source_document_no = source_document_no;
            let item = allocation_page_row(&row);
            assert_eq!(item.source_document_no, None);
            assert_eq!(item.payable_account_id, "account");
            let json = serde_json::to_value(&item).unwrap();
            assert!(json.get("source_document_no").is_none());
        }
    }
}
