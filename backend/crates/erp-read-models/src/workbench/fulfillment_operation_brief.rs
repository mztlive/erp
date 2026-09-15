//! 履约执行任务的标题与来源单据金额简报。
//!
//! 金额取来源单据当前生效版本，明确标注来源，不冒充本次履约批次金额。

use std::collections::{HashMap, HashSet};

use erp_core::money::Amount;
use persistence_core::Executor;

#[cfg(test)]
use super::authority::fulfillment::fulfillment_source_label;
use super::brief::{ObjectBriefSource, non_empty, push_document_section, push_section};
use super::presentation::format_yuan;
use super::{ObjectKind, WorkbenchObjectFact, WorkbenchObjectFactMap, WorkbenchReadService, object_ids};
use crate::errors::Result;

type SourceBriefs = HashMap<(ObjectKind, String), ObjectBriefSource>;
impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 装载履约权威事实，并批量补充来源单据当前生效金额。
    ///
    /// # 参数
    /// * `keys` - 本批任务对象键
    /// * `facts` - 输出事实；仅覆盖展示，不修改权威身份与参与关系
    /// * `executor` - 仓储执行器
    ///
    /// # 返回
    /// 成功时写入履约标题与来源单据金额；来源或生效版本缺失时不虚构金额。
    ///
    /// # 错误
    /// 任一仓储读取失败时返回错误。
    pub(super) async fn load_fulfillment_operation_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut loaded = erp_workflow::ports::ObjectFactMap::new();
        self.facts_reader().load_fulfillment_operation_facts(keys, &mut loaded, executor).await?;
        let source_keys = loaded
            .iter()
            .filter_map(|((kind, _), fact)| {
                source_kind(*kind).map(|kind| (kind, fact.root_document_id.clone()))
            })
            .collect();
        let mut briefs = self.fulfillment_sales_briefs(&source_keys, executor).await?;
        briefs.extend(self.fulfillment_purchase_briefs(&source_keys, executor).await?);
        facts.extend(loaded.into_iter().map(|(key, authority)| {
            let mut fact = WorkbenchObjectFact::from_authority(authority);
            apply_source_brief(key.0, &mut fact, &briefs);
            (key, fact)
        }));
        self.load_fulfillment_details(keys, facts, executor).await
    }
}

/// 按履约对象类型选择来源单据，发货使用销售单，其余使用采购单。
fn source_kind(kind: ObjectKind) -> Option<ObjectKind> {
    match kind {
        ObjectKind::Delivery => Some(ObjectKind::SalesOrder),
        ObjectKind::PurchaseReceipt | ObjectKind::ElectronicDelivery | ObjectKind::ServiceFulfillment => {
            Some(ObjectKind::PurchaseOrder)
        },
        _ => None,
    }
}

/// 只挂接同类来源单据的简报；缺失关联时保留标题，不把未知金额补零。
fn apply_source_brief(kind: ObjectKind, fact: &mut WorkbenchObjectFact, briefs: &SourceBriefs) {
    let Some(kind) = source_kind(kind) else {
        return;
    };
    let Some(brief) = briefs.get(&(kind, fact.authority.root_document_id.clone())) else {
        return;
    };
    fact.display.counterparty_label = brief.customer.clone();
    fact.display.brief_source = Some(brief.clone());
}

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 批量读取发货来源销售单与其当前生效版本；缺失版本不回退审核中提交。
    async fn fulfillment_sales_briefs(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<SourceBriefs> {
        let ids = object_ids(keys, ObjectKind::SalesOrder);
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let orders = self.facts_reader().read_sales_orders(&ids, executor).await?;
        let ids =
            orders.iter().filter_map(|order| order.stable.current_revision_id.clone()).collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let revisions = self
            .facts_reader()
            .read_sales_revisions(&ids, executor)
            .await?
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();
        Ok(sales_source_briefs(&orders, &revisions))
    }

    /// 批量读取入库、电子交付与服务履约来源采购单的当前生效版本。
    async fn fulfillment_purchase_briefs(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<SourceBriefs> {
        let ids = object_ids(keys, ObjectKind::PurchaseOrder);
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let orders = self.facts_reader().read_purchase_orders(&ids, executor).await?;
        let ids =
            orders.iter().filter_map(|order| order.stable.current_revision_id.clone()).collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let revisions = self
            .facts_reader()
            .read_purchase_revisions(&ids, executor)
            .await?
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();
        Ok(purchase_source_briefs(&orders, &revisions))
    }
}

/// 仅为当前生效版本存在且归属正确的销售来源单据生成金额。
fn sales_source_briefs(
    orders: &[erp_sales::entity::sales_order::SalesOrder],
    revisions: &HashMap<String, erp_sales::entity::sales_order::SalesOrderRevision>,
) -> SourceBriefs {
    orders
        .iter()
        .filter_map(|order| {
            let revision = revisions.get(order.stable.current_revision_id.as_ref()?)?;
            if revision.sales_order_id.as_ref() != order.base.id {
                return None;
            }
            let brief = source_brief(
                ObjectKind::SalesOrder,
                &order.base.id,
                &order.order_no,
                &revision.customer_snapshot.customer_name,
                &revision.gross_amount,
            );
            Some(((ObjectKind::SalesOrder, order.base.id.clone()), brief))
        })
        .collect()
}

/// 仅为当前生效版本存在且归属正确的采购来源单据生成金额。
fn purchase_source_briefs(
    orders: &[erp_procurement::entity::purchase_order::PurchaseOrder],
    revisions: &HashMap<String, erp_procurement::entity::purchase_order::PurchaseOrderRevision>,
) -> SourceBriefs {
    orders
        .iter()
        .filter_map(|order| {
            let revision = revisions.get(order.stable.current_revision_id.as_ref()?)?;
            if revision.purchase_order_id.as_ref() != order.base.id {
                return None;
            }
            let brief = source_brief(
                ObjectKind::PurchaseOrder,
                &order.base.id,
                &order.purchase_no,
                &revision.supplier_snapshot.supplier_name,
                &revision.gross_amount,
            );
            Some(((ObjectKind::PurchaseOrder, order.base.id.clone()), brief))
        })
        .collect()
}

/// 组装明确标注来源的含税总额，不生成含义不明的默认「含税金额」。
fn source_brief(kind: ObjectKind, id: &str, number: &str, party: &str, amount: &Amount) -> ObjectBriefSource {
    let (document_label, amount_label) = match kind {
        ObjectKind::SalesOrder => ("来源销售单", "来源销售单金额"),
        _ => ("来源采购单", "来源采购单金额"),
    };
    let mut sections = Vec::new();
    push_document_section(&mut sections, document_label, Some(number), Some(id));
    push_section(&mut sections, amount_label, Some(&format_yuan(amount)), true);
    ObjectBriefSource { customer: non_empty(party), extra_sections: sections, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fulfillment_amount_keeps_source_semantics_and_zero() {
        for (kind, label) in
            [(ObjectKind::SalesOrder, "来源销售单金额"), (ObjectKind::PurchaseOrder, "来源采购单金额")]
        {
            for raw in ["12800.50", "0"] {
                let amount = raw.parse().unwrap();
                let source = source_brief(kind, "source", "ORDER-1", "往来方", &amount);
                let brief = super::super::brief::assemble_brief(&source, None);
                assert!(
                    brief
                        .sections
                        .iter()
                        .any(|entry| entry.label == label && entry.value == format_yuan(&amount))
                );
                assert!(!brief.sections.iter().any(|entry| entry.label == "含税金额"));
                assert_eq!(
                    brief
                        .sections
                        .iter()
                        .find(|entry| entry.label.starts_with("来源") && !entry.numeric)
                        .unwrap()
                        .object_id
                        .as_deref(),
                    Some("source")
                );
            }
        }
    }

    #[test]
    fn matching_source_enriches_display_without_changing_authority() {
        let mut fact = WorkbenchObjectFact::from_authority(erp_workflow::ports::ObjectFact::new(
            "source", "发货", "owner",
        ));
        let authority = fact.authority.clone();
        let brief =
            source_brief(ObjectKind::SalesOrder, "source", "SO-1", "客户甲", &"12.50".parse().unwrap());
        let sources = HashMap::from([((ObjectKind::SalesOrder, "source".into()), brief)]);
        apply_source_brief(ObjectKind::Delivery, &mut fact, &sources);
        assert_eq!(fact.display.counterparty_label.as_deref(), Some("客户甲"));
        assert!(fact.display.brief_source.is_some());
        assert_eq!(fact.authority.root_document_id, authority.root_document_id);
        assert_eq!(fact.authority.counterparty_label, authority.counterparty_label);
    }

    #[test]
    fn absent_or_wrong_kind_source_does_not_invent_amounts() {
        let source =
            source_brief(ObjectKind::SalesOrder, "source", "SO-1", "客户甲", &"12.50".parse().unwrap());
        let sources = HashMap::from([((ObjectKind::SalesOrder, "source".into()), source)]);
        for (kind, id) in [
            (ObjectKind::Delivery, "missing"),
            (ObjectKind::PurchaseReceipt, "source"),
            (ObjectKind::SalesOrder, "source"),
        ] {
            let mut fact = WorkbenchObjectFact::from_authority(erp_workflow::ports::ObjectFact::new(
                id, "作业", "owner",
            ));
            apply_source_brief(kind, &mut fact, &sources);
            assert!(fact.display.brief_source.is_none());
        }
        for kind in
            [ObjectKind::PurchaseReceipt, ObjectKind::ElectronicDelivery, ObjectKind::ServiceFulfillment]
        {
            assert_eq!(source_kind(kind), Some(ObjectKind::PurchaseOrder));
        }
    }

    #[test]
    fn fulfillment_title_uses_source_document_number() {
        assert_eq!(
            fulfillment_source_label("供应商直发", "销售单", Some("SO20260826-000001")),
            "供应商直发 · 销售单 SO20260826-000001"
        );
        assert_eq!(fulfillment_source_label("供应商直发", "销售单", Some("  ")), "供应商直发");
        assert_eq!(fulfillment_source_label("采购入库", "采购单", None), "采购入库");
    }
}
