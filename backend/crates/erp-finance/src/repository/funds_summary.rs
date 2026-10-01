//! 款项汇总的窄金额流；沿原父单 ID 集合 find 返回顺序读取，不在数据库重组金额。

use std::collections::{BTreeSet, HashMap};

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::money::Amount;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};
use serde::Deserialize;

use super::{PayableExt, ReceivableExt};
use crate::entity::payable::PayableSourceType;
use crate::entity::receivable::AllocationAction;
use crate::{Error, Result};

/// 一条原始分配的必要金额与本域解析的来源，不携带展示 DTO 或外域事实。
pub struct FinancialSummaryLink {
    /// 分配稳定主键。
    pub id: String,
    /// 所属回款或付款。
    pub parent_id: String,
    /// 实际来源单据主键；缺失子账链保持 None。
    pub source_document_id: Option<String>,
    /// 付款真实来源类型；回款为 None。
    pub source_type: Option<PayableSourceType>,
    /// 原始非负金额。
    pub amount: Amount,
    /// 原正反动作，不预先归并。
    pub action: AllocationAction,
}

/// 财务拥有集合的只读金额事实读取器，跨域范围由消费 ReadModel 判断。
pub struct FundsSummaryRepository<'a> {
    db: &'a Database,
}

/// 两类款项的字段映射只控制拥有集合，不包含授权策略。
#[derive(Clone, Copy)]
enum Kind {
    Receipt,
    Payment,
}

impl<'a> FundsSummaryRepository<'a> {
    /// 绑定财务集合数据库。
    ///
    /// # 参数
    /// 财务数据库。
    /// # 返回
    /// 只读事实仓储。
    /// # 错误
    /// 无。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 沿原回款 ID 集合一次 find 读取必要金额，保持分配返回流。
    ///
    /// # 参数
    /// 原最终匹配回款 ID 顺序与原快照执行器。
    /// # 返回
    /// 原流顺序的分配金额与本域来源引用。
    /// # 错误
    /// 查询或必要引用字段损坏时拒绝；缺失关联仍保留 None 由消费方处理。
    pub async fn receipt_links(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinancialSummaryLink>> {
        self.links(Kind::Receipt, ids, executor).await
    }

    /// 沿原付款 ID 集合一次 find 读取必要金额，保留采购与结算实际类型。
    ///
    /// # 参数
    /// 原最终匹配付款 ID 顺序与原事务执行器。
    /// # 返回
    /// 原流顺序的分配事实。
    /// # 错误
    /// 数据读取或必要字段损坏时拒绝。
    pub async fn payment_links(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinancialSummaryLink>> {
        self.links(Kind::Payment, ids, executor).await
    }

    /// 主查询仍与旧 find 使用同一父单条件；仅减少传输字段。
    async fn links(
        &self,
        kind: Kind,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinancialSummaryLink>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let (collection, parent) = match kind {
            Kind::Receipt => (<Database as ReceivableExt>::RECEIPT_ALLOCATIONS, "customer_receipt_id"),
            Kind::Payment => (<Database as PayableExt>::PAYMENT_ALLOCATIONS, "supplier_payment_id"),
        };
        let rows: Vec<AmountRow> = mongo_ops::find_many(
            &self.db.collection(collection),
            doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, parent: { "$in": ids } },
            FindOptions::builder()
                .projection(doc! { "id": 1, "customer_receipt_id": 1,
                "supplier_payment_id": 1, "receivable_entry_id": 1, "payable_entry_id": 1,
                "allocated_amount": 1, "allocation_action": 1 })
                .build(),
            executor,
        )
        .await?;
        for row in &rows {
            row.required_ids(kind)?;
        }
        let entries = self.entries(kind, &rows, executor).await?;
        let accounts = self.accounts(kind, entries.values(), executor).await?;
        assemble_links(kind, rows, &entries, &accounts)
    }

    /// 仅投影分录与子账引用，批次不会改变金额主查询返回顺序。
    async fn entries(
        &self,
        kind: Kind,
        rows: &[AmountRow],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let (collection, field) = match kind {
            Kind::Receipt => (<Database as ReceivableExt>::RECEIVABLE_ENTRIES, "receivable_account_id"),
            Kind::Payment => (<Database as PayableExt>::PAYABLE_ENTRIES, "payable_account_id"),
        };
        let ids = rows
            .iter()
            .filter_map(|row| row.entry(kind))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut mapped = HashMap::new();
        for chunk in ids.chunks(500) {
            let entries: Vec<EntryRow> = mongo_ops::find_many(
                &self.db.collection(collection),
                doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "id": { "$in": chunk } },
                FindOptions::builder().projection(doc! { "id": 1, field: 1 }).build(),
                executor,
            )
            .await?;
            for entry in entries {
                let account = match kind {
                    Kind::Receipt => entry.receivable_account_id,
                    Kind::Payment => entry.payable_account_id,
                }
                .ok_or_else(|| Error::Internal("核销分录子账引用缺失".into()))?;
                mapped.insert(entry.id, account);
            }
        }
        Ok(mapped)
    }

    /// 子账事实只绑定实际来源，不读取销售、采购或结算集合。
    async fn accounts<'b>(
        &self,
        kind: Kind,
        ids: impl Iterator<Item = &'b String>,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, AccountRow>> {
        let collection = match kind {
            Kind::Receipt => <Database as ReceivableExt>::RECEIVABLE_ACCOUNTS,
            Kind::Payment => <Database as PayableExt>::PAYABLE_ACCOUNTS,
        };
        let ids = ids.cloned().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
        let mut mapped = HashMap::new();
        for chunk in ids.chunks(500) {
            let accounts: Vec<AccountRow> = mongo_ops::find_many(
                &self.db.collection(collection),
                doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "id": { "$in": chunk } },
                FindOptions::builder()
                    .projection(doc! { "id": 1, "sales_order_id": 1,
                    "source_document_id": 1, "source_type": 1 })
                    .build(),
                executor,
            )
            .await?;
            for account in accounts {
                match kind {
                    Kind::Receipt if account.sales_order_id.is_none() => {
                        return Err(Error::Internal("应收子账来源引用缺失".into()));
                    },
                    Kind::Payment
                        if account.source_document_id.is_none() || account.source_type.is_none() =>
                    {
                        return Err(Error::Internal("应付子账来源引用缺失".into()));
                    },
                    _ => {},
                }
                mapped.insert(account.id.clone(), account);
            }
        }
        Ok(mapped)
    }
}

/// 分配轻量投影保持原枚举和金额类型，非法动作不能转换成零。
#[derive(Deserialize)]
struct AmountRow {
    id: String,
    customer_receipt_id: Option<String>,
    supplier_payment_id: Option<String>,
    receivable_entry_id: Option<String>,
    payable_entry_id: Option<String>,
    allocated_amount: Amount,
    allocation_action: AllocationAction,
}

impl AmountRow {
    /// 必要父单和分录字段损坏时拒绝，不把字段缺失当成未分配。
    fn required_ids(&self, kind: Kind) -> Result<(&String, &String)> {
        let parent = match kind {
            Kind::Receipt => self.customer_receipt_id.as_ref(),
            Kind::Payment => self.supplier_payment_id.as_ref(),
        };
        parent.zip(self.entry(kind)).ok_or_else(|| Error::Internal("核销分配必要引用缺失".into()))
    }

    /// 返回对应资源的真实分录引用。
    fn entry(&self, kind: Kind) -> Option<&String> {
        match kind {
            Kind::Receipt => self.receivable_entry_id.as_ref(),
            Kind::Payment => self.payable_entry_id.as_ref(),
        }
    }
}

#[derive(Deserialize)]
struct EntryRow {
    id: String,
    receivable_account_id: Option<String>,
    payable_account_id: Option<String>,
}

#[derive(Deserialize)]
struct AccountRow {
    id: String,
    sales_order_id: Option<String>,
    source_document_id: Option<String>,
    source_type: Option<PayableSourceType>,
}

/// 关联映射不排序、去重或归并金额，保留原返回流及缺失来源标记。
fn assemble_links(
    kind: Kind,
    rows: Vec<AmountRow>,
    entries: &HashMap<String, String>,
    accounts: &HashMap<String, AccountRow>,
) -> Result<Vec<FinancialSummaryLink>> {
    rows.into_iter()
        .map(|row| {
            let (parent, _) = row.required_ids(kind)?;
            let parent_id = parent.clone();
            let account =
                row.entry(kind).and_then(|entry| entries.get(entry)).and_then(|id| accounts.get(id));
            let (source_document_id, source_type) = match kind {
                Kind::Receipt => (account.and_then(|item| item.sales_order_id.clone()), None),
                Kind::Payment => (
                    account.and_then(|item| item.source_document_id.clone()),
                    account.and_then(|item| item.source_type),
                ),
            };
            Ok(FinancialSummaryLink {
                id: row.id,
                parent_id,
                source_document_id,
                source_type,
                amount: row.allocated_amount,
                action: row.allocation_action,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生产关联函数保持交错正反原流，采购和结算同 ID 的类型仍独立。
    #[test]
    fn amount_facts_keep_original_flow_and_real_source_type() {
        let rows: Vec<AmountRow> = serde_json::from_value(serde_json::json!([
            {"id":"apply","supplier_payment_id":"p","payable_entry_id":"e","allocated_amount":"70","allocation_action":"apply"},
            {"id":"reverse","supplier_payment_id":"p","payable_entry_id":"e","allocated_amount":"20","allocation_action":"reverse"},
            {"id":"missing","supplier_payment_id":"p","payable_entry_id":"missing","allocated_amount":"7","allocation_action":"apply"}
        ])).unwrap();
        let accounts = HashMap::from([(
            "a".into(),
            AccountRow {
                id: "a".into(),
                sales_order_id: None,
                source_document_id: Some("same".into()),
                source_type: Some(PayableSourceType::SupplierSettlement),
            },
        )]);
        let facts =
            assemble_links(Kind::Payment, rows, &HashMap::from([("e".into(), "a".into())]), &accounts)
                .unwrap();
        assert_eq!(
            facts.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["apply", "reverse", "missing"]
        );
        assert_eq!(facts[0].source_type, Some(PayableSourceType::SupplierSettlement));
        assert_eq!(facts[0].source_document_id.as_deref(), Some("same"));
        assert_eq!(facts[1].action, AllocationAction::Reverse);
        assert_eq!(facts[2].source_document_id, None);
        assert!(
            serde_json::from_value::<AmountRow>(
                serde_json::json!({"id":"bad","allocated_amount":"1","allocation_action":"invalid"})
            )
            .is_err()
        );
    }
}
