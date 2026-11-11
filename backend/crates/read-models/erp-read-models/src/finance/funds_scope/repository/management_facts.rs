//! 财务管理命令的完整根身份及真实来源；本模块不判断功能或数据权限。

use std::collections::BTreeSet;

use entity_core::BaseModel;
use erp_core::ids::{
    CustomerReceiptId, PayableAccountId, PayableEntryId, ReceivableEntryId, SupplierPaymentId,
};
use erp_finance::entity::payable::{PayableAccount, PayableSourceType};
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_identity::ResourceTargets;
use erp_identity::entity::authorization::ObjectFacts;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 由实际财务根与完整关联链确定的来源身份集合。
pub(in crate::finance::funds_scope) struct ManagementFacts {
    pub(in crate::finance::funds_scope) root: ObjectFacts,
    pub(in crate::finance::funds_scope) sales_ids: BTreeSet<String>,
    pub(in crate::finance::funds_scope) purchase_ids: BTreeSet<String>,
    pub(in crate::finance::funds_scope) settlement_ids: BTreeSet<String>,
}

impl ManagementFacts {
    /// 根事实只记录实际持久化身份和版本，不补造负责人或组织维度。
    fn new(resource: &str, base: &BaseModel) -> Self {
        Self {
            root: ObjectFacts {
                resource: resource.into(),
                id: base.id.clone(),
                version: base.version.to_string(),
                ..Default::default()
            },
            sales_ids: BTreeSet::new(),
            purchase_ids: BTreeSet::new(),
            settlement_ids: BTreeSet::new(),
        }
    }

    /// 来源类型随真实应付子账解释，同名采购及结算主键分别保留。
    fn add_payable(&mut self, source_type: PayableSourceType, source_id: &str) {
        let ids = match source_type {
            PayableSourceType::PurchaseOrder => &mut self.purchase_ids,
            PayableSourceType::SupplierSettlement => &mut self.settlement_ids,
        };
        ids.insert(source_id.to_owned());
    }

    /// 仅验证完整来源的规模与身份格式，不执行授权或截断。
    fn validate(self) -> Result<Self> {
        let ids = self
            .sales_ids
            .iter()
            .chain(&self.purchase_ids)
            .chain(&self.settlement_ids)
            .map(String::as_str)
            .collect::<Vec<_>>();
        ResourceTargets::new(&ids)?;
        Ok(self)
    }
}

/// 沿真实财务关联链装载完整根和所有来源身份。
/// # 参数
/// db 为拥有财务集合的数据库，resource 为服务端固定财务资源，id 为完整根身份；
/// executor 为原管理命令事务执行器。
/// # 返回
/// 实际根版本与销售、采购、结算来源集合；真实零分配保留空集合。
/// # 错误
/// 根或任一中间引用不存在时返回 NotFound；未登记资源、身份非法、超限或仓储失败时拒绝。
pub(in crate::finance::funds_scope) async fn load(
    db: &Database,
    resource: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<ManagementFacts> {
    let facts = match resource {
        "receivable_account" => receivable(db, id, executor).await?,
        "sales_invoice_request" => invoice_request(db, id, executor).await?,
        "payable_account" => payable(db, id, executor).await?,
        "customer_receipt" => receipt(db, id, executor).await?,
        "supplier_payment" => payment(db, id, executor).await?,
        _ => return Err(Error::ValidationError("未登记财务管理来源资源".into())),
    };
    facts.validate()
}

/// 应收子账直接引用唯一销售来源。
async fn receivable(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<ManagementFacts> {
    let account = db
        .receivable_accounts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("应收子账不存在".into()))?;
    let mut facts = ManagementFacts::new("receivable_account", &account.base);
    facts.sales_ids.insert(account.sales_order_id.to_string());
    Ok(facts)
}

/// 开票申请的直接来源与实际应收子账必须完整一致。
async fn invoice_request(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<ManagementFacts> {
    let request = db
        .sales_invoice_requests()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售开票申请不存在".into()))?;
    let account = db
        .receivable_accounts()
        .find_by_id(request.receivable_account_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("开票申请来源应收子账不存在".into()))?;
    if account.sales_order_id != request.sales_order_id {
        return Err(Error::NotFound("开票申请来源关联不完整".into()));
    }
    let mut facts = ManagementFacts::new("sales_invoice_request", &request.base);
    facts.sales_ids.insert(request.sales_order_id.to_string());
    Ok(facts)
}

/// 应付子账按真实来源类型引用采购或结算单。
async fn payable(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<ManagementFacts> {
    let account = db
        .payable_accounts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("应付子账不存在".into()))?;
    let mut facts = ManagementFacts::new("payable_account", &account.base);
    facts.add_payable(account.source_type, &account.source_document_id);
    Ok(facts)
}

/// 回款的每条真实核销都沿应收分录和子账展开，不忽略反向分配。
async fn receipt(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<ManagementFacts> {
    let receipt = db
        .customer_receipts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款不存在".into()))?;
    let allocations = db
        .receipt_allocations()
        .find_allocations_by_receipts(&[CustomerReceiptId::new(id)], executor)
        .await?;
    let entry_ids = allocations
        .iter()
        .map(|line| line.receivable_entry_id.to_string())
        .chain(receipt.pending_allocations.iter().map(|line| line.receivable_entry_id.to_string()))
        .collect();
    let mut facts = ManagementFacts::new("customer_receipt", &receipt.base);
    facts.sales_ids = receipt_sales(db, entry_ids, executor).await?;
    Ok(facts)
}

/// 回款中间引用必须全部存在，不能把缺失分录或子账变成无来源回款。
async fn receipt_sales(
    db: &Database,
    entry_ids: BTreeSet<String>,
    executor: &mut dyn Executor,
) -> Result<BTreeSet<String>> {
    validate_ids(&entry_ids)?;
    let ids = entry_ids.iter().cloned().map(ReceivableEntryId::new).collect::<Vec<_>>();
    let mut entries = Vec::new();
    for batch in ids.chunks(500) {
        entries.extend(db.receivable_entries().find_entries_by_ids(batch, executor).await?);
    }
    require_complete(&entry_ids, entries.iter().map(|row| row.base.id.clone()))?;
    let account_ids = entries.iter().map(|row| row.receivable_account_id.to_string()).collect();
    receipt_account_sales(db, account_ids, executor).await
}

/// 读取实际应收子账对应销售来源，缺失任何一笔均拒绝返回部分集合。
async fn receipt_account_sales(
    db: &Database,
    account_ids: BTreeSet<String>,
    executor: &mut dyn Executor,
) -> Result<BTreeSet<String>> {
    validate_ids(&account_ids)?;
    let ids = account_ids.iter().cloned().collect::<Vec<_>>();
    let mut accounts = Vec::new();
    for batch in ids.chunks(500) {
        accounts.extend(db.receivable_accounts().find_accounts_by_ids(batch, executor).await?);
    }
    require_complete(&account_ids, accounts.iter().map(|row| row.base.id.clone()))?;
    Ok(accounts.into_iter().map(|account| account.sales_order_id.to_string()).collect())
}

/// 付款的全部核销沿真实应付链展开，同一根允许同时涉及采购与结算来源。
async fn payment(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<ManagementFacts> {
    let payment = db
        .supplier_payments()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商付款不存在".into()))?;
    let allocations = db
        .payment_allocations()
        .find_allocations_by_payments(&[SupplierPaymentId::new(id)], executor)
        .await?;
    let entry_ids = allocations.iter().map(|line| line.payable_entry_id.to_string()).collect();
    let mut facts = ManagementFacts::new("supplier_payment", &payment.base);
    let mut accounts = payment_accounts(db, entry_ids, executor).await?;
    let requested = payment.request_allocations.iter().map(|line| line.payable_account_id.clone()).collect();
    accounts.extend(payment_sources(db, requested, executor).await?);
    for account in accounts {
        facts.add_payable(account.source_type, &account.source_document_id);
    }
    Ok(facts)
}

/// 付款中间引用必须完整存在；分录集合为空时仓储不执行额外查询。
async fn payment_accounts(
    db: &Database,
    entry_ids: BTreeSet<String>,
    executor: &mut dyn Executor,
) -> Result<Vec<PayableAccount>> {
    validate_ids(&entry_ids)?;
    let ids = entry_ids.iter().cloned().map(PayableEntryId::new).collect::<Vec<_>>();
    let mut entries = Vec::new();
    for batch in ids.chunks(500) {
        entries.extend(db.payable_entries().find_entries_by_ids(batch, executor).await?);
    }
    require_complete(&entry_ids, entries.iter().map(|row| row.base.id.clone()))?;
    let account_ids = entries.iter().map(|row| row.payable_account_id.to_string()).collect();
    payment_sources(db, account_ids, executor).await
}

/// 应付来源类型由拥有领域实体提供，不按同名主键猜测来源类型。
async fn payment_sources(
    db: &Database,
    account_ids: BTreeSet<String>,
    executor: &mut dyn Executor,
) -> Result<Vec<PayableAccount>> {
    validate_ids(&account_ids)?;
    let ids = account_ids.iter().cloned().map(PayableAccountId::new).collect::<Vec<_>>();
    let mut accounts = Vec::new();
    for batch in ids.chunks(500) {
        accounts.extend(db.payable_accounts().find_accounts_by_ids(batch, executor).await?);
    }
    require_complete(&account_ids, accounts.iter().map(|row| row.base.id.clone()))?;
    Ok(accounts)
}

/// 中间财务引用只按批次校验身份格式，最终来源规模由完整来源集合统一验证。
fn validate_ids(ids: &BTreeSet<String>) -> Result<()> {
    let references = ids.iter().map(String::as_str).collect::<Vec<_>>();
    for batch in references.chunks(500) {
        ResourceTargets::new(batch)?;
    }
    Ok(())
}

/// 比对完整实际引用，额外、缺失及重复返回行均拒绝。
fn require_complete(expected: &BTreeSet<String>, found: impl IntoIterator<Item = String>) -> Result<()> {
    let found = found.into_iter().collect::<Vec<_>>();
    let unique = found.iter().cloned().collect::<BTreeSet<_>>();
    if &unique != expected || unique.len() != found.len() {
        return Err(Error::NotFound("财务来源引用不存在或不完整".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_references_accept_unordered_rows_and_reject_missing_extra_or_duplicate_rows() {
        let expected = ["a".into(), "b".into()].into();
        assert!(require_complete(&expected, ["b".into(), "a".into()]).is_ok());
        for rows in [
            vec!["a".into()],
            vec!["a".into(), "b".into(), "extra".into()],
            vec!["a".into(), "a".into(), "b".into()],
        ] {
            assert!(matches!(require_complete(&expected, rows), Err(Error::NotFound(_))));
        }
        assert!(require_complete(&BTreeSet::new(), []).is_ok());
    }

    #[test]
    fn actual_root_version_and_empty_sources_do_not_invent_responsibility_facts() {
        let mut base = BaseModel::fake();
        base.id = "receipt".into();
        base.version = 9;
        let facts = ManagementFacts::new("customer_receipt", &base).validate().unwrap();
        assert_eq!(facts.root.resource, "customer_receipt");
        assert_eq!(facts.root.id, "receipt");
        assert_eq!(facts.root.version, "9");
        assert!(facts.sales_ids.is_empty());
        assert!(facts.purchase_ids.is_empty());
        assert!(facts.settlement_ids.is_empty());
        assert_eq!(facts.root.owner_id, Default::default());
        assert_eq!(facts.root.org_unit_id, Default::default());
    }

    #[test]
    fn same_id_purchase_and_settlement_remain_distinct_actual_sources() {
        let mut facts = ManagementFacts::new("supplier_payment", &BaseModel::fake());
        facts.add_payable(PayableSourceType::PurchaseOrder, "same-id");
        facts.add_payable(PayableSourceType::SupplierSettlement, "same-id");
        facts.add_payable(PayableSourceType::PurchaseOrder, "same-id");
        let facts = facts.validate().unwrap();
        assert_eq!(facts.purchase_ids, ["same-id".into()].into());
        assert_eq!(facts.settlement_ids, ["same-id".into()].into());
        assert!(facts.sales_ids.is_empty());
    }

    #[test]
    fn source_scale_is_rejected_without_truncating_the_actual_set() {
        let ids = (0..1001).map(|index| format!("sales-{index}")).collect::<BTreeSet<_>>();
        assert!(validate_ids(&ids).is_ok());
        let mut facts = ManagementFacts::new("customer_receipt", &BaseModel::fake());
        facts.sales_ids = ids.clone();
        assert!(facts.validate().is_err());
        assert_eq!(ids.len(), 1001);
        assert!(validate_ids(&BTreeSet::new()).is_ok());
        assert!(validate_ids(&[" leading-space".into()].into()).is_err());
    }
}
