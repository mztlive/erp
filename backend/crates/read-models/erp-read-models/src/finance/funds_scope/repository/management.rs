//! 财务真实对象事实及共享入口调用；授权组合由 identity 固定。
use std::collections::BTreeSet;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_finance::entity::receivable::{Invoice, InvoiceStatus};
use erp_identity::entity::authorization::{
    AccessContext, FactValue, ObjectFacts, RelatedObjects, RelationKey,
};
use erp_identity::{
    AccessFactsPort, AccessTarget, Error as IdentityError, Permission, ResourceTargets,
    Result as IdentityResult,
};
use erp_permissions::internal;
use erp_procurement::authorization::PurchaseFacts;
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::repository::prelude::*;
use mongodb::Database;
use mongodb::bson::{Bson, Document, deserialize_from_document, doc};
use persistence_core::{Executor, aggregate};

use super::super::FundsAccess;
use super::mapping::{collection, source_projection};
use crate::Result;
use crate::sales_center::repository::authorization::SalesFacts;

impl FundsAccess {
    /// 使用真实审批实例或任务身份执行固定管理入口。
    /// # 参数
    /// actor 为已认证账号，id 必须是 entry 对应的真实对象身份，executor 为原执行器。
    /// # 返回
    /// 固定入口的全部基础及来源要求满足时成功。
    /// # 错误
    /// 对象、资格或完整来源不满足时拒绝。
    pub async fn require_source_operation(
        &self,
        actor: &AuditActor,
        id: &str,
        entry: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.rbac.require(actor, entry, AccessTarget::record(id)?, executor).await?;
        Ok(())
    }
}

/// 组合根为一个固定财务资源绑定的真实对象读取器。
pub struct FinancialFacts {
    db: Database,
    resource: String,
}
impl FinancialFacts {
    /// 绑定固定资源，不接收请求中的事实。
    /// # 参数
    /// db 为共享数据库，resource 为已接入资源。
    /// # 返回
    /// 读取真实根及完整来源的端口。
    /// # 错误
    /// 无；不支持资源在读取时拒绝。
    pub fn new(db: Database, resource: &str) -> Self {
        Self { db, resource: resource.into() }
    }
}
#[async_trait]
impl AccessFactsPort for FinancialFacts {
    async fn load(
        &self,
        context: &AccessContext,
        targets: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        if matches!(self.resource.as_str(), "customer_refund" | "supplier_refund") {
            return self.refunds(context, targets, executor).await;
        }
        if matches!(self.resource.as_str(), "receipt_reversal" | "payment_reversal") {
            return self.reversals(context, targets, executor).await;
        }
        let collection_name = collection(&self.resource)?;
        let mut pipeline = vec![
            doc! { "$match": { "id": { "$in": targets.ids().iter().collect::<Vec<_>>() }, "deleted_at": 0 } },
        ];
        pipeline.extend(source_projection(&self.resource)?);
        let rows: Vec<Document> =
            aggregate::rows(&self.db.collection(collection_name), pipeline, executor).await?;
        let mut result = Vec::new();
        for row in rows {
            invoice_result_state(&context.permission, &row)?;
            if !matches!(row.get_bool("__funds_sources_valid"), Ok(true)) {
                return Err(IdentityError::ValidationError("财务实际来源引用不完整".into()));
            }
            let mut root = root_facts(&self.resource, &row)?;
            for (resource, relation, field) in [
                ("sales_order", "sales_source", "__funds_sales_ids"),
                ("purchase_order", "purchase_source", "__funds_purchase_ids"),
                ("supplier_settlement_statement", "settlement_source", "__funds_settlement_ids"),
            ] {
                let ids = source_ids(&row, field)?;
                let objects = source_objects(&self.db, context, resource, &ids, executor).await?;
                root.relations
                    .insert(RelationKey::parse(relation)?, RelatedObjects { expected_ids: ids, objects });
            }
            result.push(root);
        }
        Ok(result)
    }
}

/// 已登记回执的状态只取持久化实体，不以调用方事实或未知字符串推断。
fn invoice_result_state(permission: &Permission, row: &Document) -> IdentityResult<()> {
    if permission != &internal::INVOICE_POSTED_RESULT_READ {
        return Ok(());
    }
    let invoice: Invoice = deserialize_from_document(row.clone())
        .map_err(|_| IdentityError::ValidationError("发票真实登记状态无效".into()))?;
    if !invoice.is_registered() && invoice.stable.status() != InvoiceStatus::RedInvoiced {
        return Err(IdentityError::Forbidden("只有已登记或已红冲发票可读取登记结果".into()));
    }
    Ok(())
}

impl FinancialFacts {
    async fn reversals(
        &self,
        context: &AccessContext,
        targets: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let collection =
            if self.resource == "receipt_reversal" { "receipt_reversals" } else { "payment_reversals" };
        let pipeline = vec![
            doc! { "$match": { "id": { "$in": targets.ids().iter().collect::<Vec<_>>() }, "deleted_at": 0 } },
        ];
        let rows: Vec<Document> =
            aggregate::rows(&self.db.collection(collection), pipeline, executor).await?;
        let mut result = Vec::new();
        for row in rows {
            let ReversalSourceFacts { mut root, source_resource, relation, ids } =
                reversal_facts(&self.resource, &row)?;
            let targets = ResourceTargets::new(&ids.iter().map(String::as_str).collect::<Vec<_>>())?;
            let source = Self::new(self.db.clone(), source_resource);
            let objects = source.load(context, &targets, executor).await?;
            root.relations.insert(relation, RelatedObjects { expected_ids: ids, objects });
            result.push(root);
        }
        Ok(result)
    }

    async fn refunds(
        &self,
        context: &AccessContext,
        targets: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let customer = self.resource == "customer_refund";
        let collection = if customer { "customer_refunds" } else { "supplier_refunds" };
        let pipeline = vec![
            doc! { "$match": { "id": { "$in": targets.ids().iter().collect::<Vec<_>>() }, "deleted_at": 0 } },
        ];
        let rows: Vec<Document> =
            aggregate::rows(&self.db.collection(collection), pipeline, executor).await?;
        let mut objects = Vec::new();
        for row in rows {
            let mut root = root_facts(&self.resource, &row)?;
            root.owner_id = FactValue::Known(row.get_str("handled_by").ok().map(str::to_owned));
            for (relation, resource) in if customer {
                [("receipt_source", "customer_receipt"), ("receivable_source", "receivable_account")]
            } else {
                [("payment_source", "supplier_payment"), ("payable_source", "payable_account")]
            } {
                let id = self.refund_source(&row, resource, executor).await?;
                let ids = id.into_iter().collect::<BTreeSet<_>>();
                let targets = ResourceTargets::new(&ids.iter().map(String::as_str).collect::<Vec<_>>())?;
                let source = Self::new(self.db.clone(), resource);
                let nested = source.load(context, &targets, executor).await?;
                root.relations.insert(
                    RelationKey::parse(relation)?,
                    RelatedObjects { expected_ids: ids, objects: nested },
                );
            }
            objects.push(root);
        }
        Ok(objects)
    }

    async fn refund_source(
        &self,
        row: &Document,
        resource: &str,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Option<String>> {
        let (field, entries, account_field) = match resource {
            "customer_receipt" => return Ok(row.get_str("original_receipt_id").ok().map(str::to_owned)),
            "supplier_payment" => return Ok(row.get_str("original_payment_id").ok().map(str::to_owned)),
            "receivable_account" => {
                ("original_receivable_entry_id", "receivable_entries", "receivable_account_id")
            },
            "payable_account" => ("original_payable_entry_id", "payable_entries", "payable_account_id"),
            _ => return Err(IdentityError::ValidationError("未知退款来源".into())),
        };
        let Some(id) = row.get_str(field).ok() else {
            return Ok(None);
        };
        let pipeline = vec![doc! { "$match": { "id": id, "deleted_at": 0 } }];
        let rows: Vec<Document> = aggregate::rows(&self.db.collection(entries), pipeline, executor).await?;
        rows.first()
            .and_then(|entry| entry.get_str(account_field).ok())
            .map(|id| Some(id.to_owned()))
            .ok_or_else(|| IdentityError::ValidationError("退款原资金分录不存在".into()))
    }
}

struct ReversalSourceFacts {
    root: ObjectFacts,
    source_resource: &'static str,
    relation: RelationKey,
    ids: BTreeSet<String>,
}

/// 冲正归属和来源只取当前真实记录，缺少原款单身份时拒绝。
fn reversal_facts(resource: &str, row: &Document) -> IdentityResult<ReversalSourceFacts> {
    let (source_resource, relation, field) = match resource {
        "receipt_reversal" => ("customer_receipt", "receipt_source", "original_customer_receipt_id"),
        "payment_reversal" => ("supplier_payment", "payment_source", "original_supplier_payment_id"),
        _ => return Err(IdentityError::ValidationError("未知资金冲正资源".into())),
    };
    let id = row
        .get_str(field)
        .ok()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| IdentityError::ValidationError("冲正原款单身份缺失或无效".into()))?;
    let mut root = root_facts(resource, row)?;
    root.owner_id = FactValue::Known(row.get_str("handled_by").ok().map(str::to_owned));
    Ok(ReversalSourceFacts {
        root,
        source_resource,
        relation: RelationKey::parse(relation)?,
        ids: [id.into()].into(),
    })
}

fn root_facts(resource: &str, row: &Document) -> IdentityResult<ObjectFacts> {
    let id = row.get_str("id").map_err(|_| IdentityError::ValidationError("财务对象缺少身份".into()))?;
    let version = match row.get("version") {
        Some(Bson::Int64(v)) => v.to_string(),
        Some(Bson::Int32(v)) => v.to_string(),
        _ => return Err(IdentityError::ValidationError("财务对象缺少有效版本".into())),
    };
    Ok(ObjectFacts {
        resource: resource.into(),
        id: id.into(),
        version,
        owner_id: FactValue::Known(row.get_str("created_by").ok().map(str::to_owned)),
        ..Default::default()
    })
}
fn source_ids(row: &Document, field: &str) -> IdentityResult<BTreeSet<String>> {
    row.get_array(field)
        .map_err(|_| IdentityError::ValidationError("财务来源集合未读取".into()))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| IdentityError::ValidationError("财务来源身份非法".into()))
        })
        .collect()
}

async fn source_objects(
    db: &Database,
    context: &AccessContext,
    resource: &str,
    ids: &BTreeSet<String>,
    executor: &mut dyn Executor,
) -> IdentityResult<Vec<ObjectFacts>> {
    let context = AccessContext { resource: resource.into(), model: None, ..context.clone() };
    let targets = ResourceTargets::new(&ids.iter().map(String::as_str).collect::<Vec<_>>())?;
    match resource {
        "sales_order" => SalesFacts(db.clone()).load(&context, &targets, executor).await,
        "purchase_order" => PurchaseFacts::new(db.clone()).load(&context, &targets, executor).await,
        "supplier_settlement_statement" => settlement_objects(db, &targets, executor).await,
        _ => Err(IdentityError::ValidationError("未知财务实际来源".into())),
    }
}

async fn settlement_objects(
    db: &Database,
    ids: &ResourceTargets,
    executor: &mut dyn Executor,
) -> IdentityResult<Vec<ObjectFacts>> {
    let rows = db
        .supplier_settlement_statements()
        .find_statements_by_ids(&ids.ids().iter().cloned().collect::<Vec<_>>(), executor)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| ObjectFacts {
            resource: "supplier_settlement_statement".into(),
            id: row.base.id,
            version: row.base.version.to_string(),
            owner_id: FactValue::Known(Some(row.prepared_by)),
            org_unit_id: FactValue::Known(Some(row.business_org_unit_id)),
            ..Default::default()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{InvoiceId, PartyId};
    use erp_finance::entity::receivable::{InvoiceData, InvoiceDirection, InvoiceKind};
    use erp_permissions::receivable;
    use mongodb::bson::serialize_to_document;

    use super::*;

    #[test]
    fn reversal_facts_keep_real_identity_handler_and_exact_original_fund() {
        for (resource, source, relation, field) in [
            ("receipt_reversal", "customer_receipt", "receipt_source", "original_customer_receipt_id"),
            ("payment_reversal", "supplier_payment", "payment_source", "original_supplier_payment_id"),
        ] {
            let mut row = doc! {
                "id": "reversal", "version": 4_i64,
                "handled_by": "handler", "created_by": "different-creator",
                "original_receipt_id": "unrelated-receipt", "original_payment_id": "unrelated-payment"
            };
            row.insert(field, "original-fund");
            let facts = reversal_facts(resource, &row).unwrap();
            assert_eq!(facts.root.resource, resource);
            assert_eq!(facts.root.id, "reversal");
            assert_eq!(facts.root.version, "4");
            assert_eq!(facts.root.owner_id, FactValue::Known(Some("handler".into())));
            assert_eq!(facts.source_resource, source);
            assert_eq!(facts.relation, RelationKey::parse(relation).unwrap());
            assert_eq!(facts.ids, BTreeSet::from(["original-fund".into()]));
            row.remove("handled_by");
            assert_eq!(reversal_facts(resource, &row).unwrap().root.owner_id, FactValue::Known(None));
            for invalid in [Bson::Null, Bson::String(String::new()), Bson::String("  ".into())] {
                row.insert(field, invalid);
                assert!(reversal_facts(resource, &row).is_err());
            }
            row.remove(field);
            assert!(reversal_facts(resource, &row).is_err());
        }
        assert!(reversal_facts("unknown", &doc! {}).is_err());
    }

    #[test]
    fn posted_invoice_result_requires_real_registered_state() {
        let mut invoice = Invoice::new(
            InvoiceId::new("invoice"),
            InvoiceData {
                invoice_direction: InvoiceDirection::Sales,
                invoice_kind: InvoiceKind::Blue,
                party_id: PartyId::new("customer-party"),
                invoice_code: None,
                invoice_no: "INV-1".into(),
                invoice_date: BusinessDate::from_ymd(2026, 11, 13).unwrap(),
                gross_amount: "10.00".parse().unwrap(),
                net_amount: "8.00".parse().unwrap(),
                tax_amount: "2.00".parse().unwrap(),
                rounding_adjustment_amount: "0.00".parse().unwrap(),
                rounding_reason: None,
                original_invoice_id: None,
            },
            "issuer",
        )
        .unwrap();
        let permission = &internal::INVOICE_POSTED_RESULT_READ;
        let draft = serialize_to_document(&invoice).unwrap();
        assert!(matches!(invoice_result_state(permission, &draft), Err(IdentityError::Forbidden(_))));
        invoice_result_state(&receivable::INVOICE_DETAIL, &draft).unwrap();
        invoice.mark_registered("issuer").unwrap();
        invoice_result_state(permission, &serialize_to_document(&invoice).unwrap()).unwrap();
        invoice.mark_red_invoiced("issuer").unwrap();
        let red = serialize_to_document(&invoice).unwrap();
        invoice_result_state(permission, &red).unwrap();
        for status in [Bson::String("unknown".into()), Bson::Null] {
            let mut invalid = red.clone();
            invalid.insert("status", status);
            assert!(matches!(
                invoice_result_state(permission, &invalid),
                Err(IdentityError::ValidationError(_))
            ));
        }
    }

    #[test]
    fn financial_root_uses_persisted_identity_version_and_creator() {
        let facts = root_facts(
            "customer_receipt",
            &doc! { "id": "receipt", "version": 7_i64, "created_by": "clerk" },
        )
        .unwrap();
        assert_eq!(facts.resource, "customer_receipt");
        assert_eq!(facts.id, "receipt");
        assert_eq!(facts.version, "7");
        assert_eq!(facts.owner_id, FactValue::Known(Some("clerk".into())));
        assert!(root_facts("customer_receipt", &doc! { "id": "receipt" }).is_err());
        assert!(root_facts("customer_receipt", &doc! { "version": 1 }).is_err());
    }

    #[test]
    fn known_empty_sources_remain_distinct_from_missing_or_invalid_references() {
        assert!(source_ids(&doc! { "ids": [] }, "ids").unwrap().is_empty());
        assert_eq!(
            source_ids(&doc! { "ids": ["b", "a", "b"] }, "ids").unwrap(),
            ["a".into(), "b".into()].into()
        );
        for row in [doc! {}, doc! { "ids": null }, doc! { "ids": [null] }, doc! { "ids": [""] }] {
            assert!(source_ids(&row, "ids").is_err());
        }
    }
}
