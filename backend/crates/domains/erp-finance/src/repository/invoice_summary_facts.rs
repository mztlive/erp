//! 发票汇总的窄金额事实，保持最终发票集合原分配 find 的返回流。

use std::collections::{BTreeSet, HashMap};

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::money::Amount;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};
use serde::Deserialize;

use super::{PayableExt, ReceivableExt};
use crate::Result;
use crate::entity::payable::{AllocationAction, PayableSourceType};

/// 原分配金额与本域来源引用；跨域授权和负责人由 ReadModel 判断。
#[derive(Debug)]
pub struct InvoiceSummaryLink {
    pub id: String,
    pub parent_id: String,
    pub source_document_id: Option<String>,
    pub source_type: Option<PayableSourceType>,
    pub amount: Amount,
    pub action: AllocationAction,
}

/// 只访问财务拥有的分配和子账集合，不读取跨域来源。
pub struct InvoiceSummaryRepository<'a> {
    db: &'a Database,
}

/// 方向只控制本域集合和字段映射，不包含范围政策。
#[derive(Clone, Copy)]
enum Kind {
    Sales,
    Purchase,
}

impl<'a> InvoiceSummaryRepository<'a> {
    /// 绑定财务数据库。
    /// 结果：只读金额事实仓储。
    ///
    /// 输入约定：财务集合所在数据库。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 按原最终发票集合一次读取销项分配的必要事实。
    /// 结果：保持原无排序 find 返回顺序的金额事实。
    /// 查询或类型化解码失败时拒绝。
    ///
    /// 输入约定：最终发票 ID 集合和调用方快照执行器。
    pub async fn sales_links(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<InvoiceSummaryLink>> {
        self.links(Kind::Sales, ids, executor).await
    }

    /// 按原最终发票集合一次读取进项分配的必要事实。
    /// 结果：原返回流的金额及实际采购或结算来源引用。
    /// 查询或必要字段解码失败时拒绝。
    ///
    /// 输入约定：最终发票 ID 集合和调用方快照执行器。
    pub async fn purchase_links(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<InvoiceSummaryLink>> {
        self.links(Kind::Purchase, ids, executor).await
    }

    // 按与原发票分配读取相同的父单范围取金额及关联字段，不增加排序或加载无关数据。
    async fn links(
        &self,
        kind: Kind,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<InvoiceSummaryLink>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let collection = match kind {
            Kind::Sales => <Database as ReceivableExt>::SALES_INVOICE_ALLOCATIONS,
            Kind::Purchase => <Database as PayableExt>::PURCHASE_INVOICE_ALLOCATIONS,
        };
        let rows: Vec<AmountRow> = mongo_ops::find_many(
            &self.db.collection(collection),
            doc! { "invoice_id": { "$in": ids }, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            FindOptions::builder()
                .projection(doc! { "id": 1, "invoice_id": 1,
                "receivable_account_id": 1, "payable_account_id": 1,
                "allocated_gross_amount": 1, "allocation_action": 1 })
                .build(),
            executor,
        )
        .await?;

        let accounts = self.accounts(kind, &rows, executor).await?;
        Ok(assemble_links(kind, rows, &accounts))
    }

    // 为发票分配加载实际应收或应付来源；不按批量子账查询的顺序重排分配明细。
    async fn accounts(
        &self,
        kind: Kind,
        rows: &[AmountRow],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, AccountRow>> {
        let collection = match kind {
            Kind::Sales => <Database as ReceivableExt>::RECEIVABLE_ACCOUNTS,
            Kind::Purchase => <Database as PayableExt>::PAYABLE_ACCOUNTS,
        };
        let ids = rows
            .iter()
            .filter_map(|row| row.account(kind))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut result = HashMap::new();
        for chunk in ids.chunks(500) {
            let accounts: Vec<AccountRow> = mongo_ops::find_many(
                &self.db.collection(collection),
                doc! { "id": { "$in": chunk }, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
                FindOptions::builder()
                    .projection(doc! { "id": 1, "sales_order_id": 1,
                    "source_type": 1, "source_document_id": 1 })
                    .build(),
                executor,
            )
            .await?;
            result.extend(accounts.into_iter().map(|account| (account.id.clone(), account)));
        }
        Ok(result)
    }
}

/// 动作和金额仍由原领域类型解码，非法动作不会变成零。
#[derive(Deserialize)]
struct AmountRow {
    id: String,
    invoice_id: String,
    receivable_account_id: Option<String>,
    payable_account_id: Option<String>,
    allocated_gross_amount: Amount,
    allocation_action: AllocationAction,
}

impl AmountRow {
    // 取得当前方向真实子账引用。
    fn account(&self, kind: Kind) -> Option<&String> {
        match kind {
            Kind::Sales => self.receivable_account_id.as_ref(),
            Kind::Purchase => self.payable_account_id.as_ref(),
        }
    }
}

/// 本域来源引用，不装载无关子账金额或展示字段。
#[derive(Deserialize)]
struct AccountRow {
    id: String,
    sales_order_id: Option<String>,
    source_document_id: Option<String>,
    source_type: Option<PayableSourceType>,
}

// 逐笔关联发票分配和实际子账；查不到的子账保留 `None`，由消费方拒绝该来源。
fn assemble_links(
    kind: Kind,
    rows: Vec<AmountRow>,
    accounts: &HashMap<String, AccountRow>,
) -> Vec<InvoiceSummaryLink> {
    rows.into_iter()
        .map(|row| {
            let account = row.account(kind).and_then(|id| accounts.get(id));
            let (source_document_id, source_type) = match kind {
                Kind::Sales => (account.and_then(|item| item.sales_order_id.clone()), None),
                Kind::Purchase => (
                    account.and_then(|item| item.source_document_id.clone()),
                    account.and_then(|item| item.source_type),
                ),
            };
            InvoiceSummaryLink {
                id: row.id,
                parent_id: row.invoice_id,
                source_document_id,
                source_type,
                amount: row.allocated_gross_amount,
                action: row.allocation_action,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 回归检查：生产映射保留交错正反流及缺失引用，不按父单或动作重排。
    #[test]
    fn links_preserve_interleaved_parent_flow_and_missing_accounts() {
        let rows = serde_json::from_value(serde_json::json!([
            {"id":"a","invoice_id":"second","payable_account_id":"p","allocated_gross_amount":"9","allocation_action":"apply"},
            {"id":"b","invoice_id":"first","payable_account_id":"p","allocated_gross_amount":"2","allocation_action":"reverse"},
            {"id":"c","invoice_id":"second","payable_account_id":"missing","allocated_gross_amount":"1","allocation_action":"apply"}
        ])).unwrap();
        let account: AccountRow = serde_json::from_value(serde_json::json!({
            "id":"p","source_type":"supplier_settlement","source_document_id":"s"
        }))
        .unwrap();
        let links = assemble_links(Kind::Purchase, rows, &HashMap::from([("p".into(), account)]));
        assert_eq!(links.iter().map(|link| link.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(links[1].action, AllocationAction::Reverse);
        assert_eq!(links[0].source_type, Some(PayableSourceType::SupplierSettlement));
        assert_eq!(links[2].source_document_id, None);
    }
}
