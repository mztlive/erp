//! Sales-change and purchase-change object facts for remaining-domain authorization.

use std::collections::{HashMap, HashSet};

use database::{PurchaseOrderExt, SalesOrderExt, SalesReviewExt};
use entities::{
    purchase_order::{PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseOrderRevision},
    sales_order::SalesOrderRevision,
    sales_review::{SalesChangeOrder, SalesChangeSubmission},
};
use persistence_core::Executor;

use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind};
use crate::errors::Result;

struct SalesChangeFactContext {
    base_revisions: HashMap<String, SalesOrderRevision>,
    submissions: HashMap<String, SalesChangeSubmission>,
}

struct PurchaseChangeFactContext {
    base_revisions: HashMap<String, PurchaseOrderRevision>,
    submissions: HashMap<String, PurchaseChangeSubmission>,
}

impl crate::work_item::ProcessObjectFacts {
    /// Load sales-change identity, counterparty and impact.
    pub(super) async fn load_sales_change_review_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SalesChangeOrder);
        if ids.is_empty() {
            return Ok(());
        }
        let changes = self
            .db
            .sales_change_orders()
            .list_active_by_ids(&ids, executor)
            .await?;
        if changes.is_empty() {
            return Ok(());
        }
        let sales_order_ids = changes
            .iter()
            .map(|item| item.sales_order_id.to_string())
            .collect::<Vec<_>>();
        let sales_nos = self
            .db
            .sales_orders()
            .list_active_by_ids(&sales_order_ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.order_no))
            .collect::<HashMap<_, _>>();
        let context = self.sales_change_fact_context(&changes, executor).await?;
        for change in changes {
            let sales_no = sales_nos.get(&change.sales_order_id.to_string()).cloned();
            let base = context.base_revisions.get(&change.base_revision_id.to_string());
            let submission = change
                .current_submission_id
                .as_ref()
                .and_then(|id| context.submissions.get(&id.to_string()));
            let mut fact = ObjectFact::new(
                change.base.id.clone(),
                sales_no
                    .as_deref()
                    .map(|no| format!("销售变更单 {no}"))
                    .unwrap_or_else(|| "销售变更单（来源单号待补全）".to_string()),
                change.stable.created_by.clone(),
            );
            fact.counterparty_label = submission
                .map(|item| item.customer_snapshot.customer_name.clone())
                .or_else(|| base.map(|item| item.customer_snapshot.customer_name.clone()));
            fact.impact_summary = Some("不审批则销售变更不能生效；通过后按目标提交形成新版本".to_string());
            facts.insert((ObjectKind::SalesChangeOrder, change.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load purchase-change identity, counterparty and impact.
    pub(super) async fn load_purchase_change_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PurchaseChangeOrder);
        if ids.is_empty() {
            return Ok(());
        }
        let changes = self
            .db
            .purchase_change_orders()
            .list_active_by_ids(&ids, executor)
            .await?;
        if changes.is_empty() {
            return Ok(());
        }
        let purchase_ids = changes
            .iter()
            .map(|item| item.purchase_order_id.to_string())
            .collect::<Vec<_>>();
        let purchase_nos = self
            .db
            .purchase_orders()
            .list_active_by_ids(&purchase_ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.purchase_no))
            .collect::<HashMap<_, _>>();
        let context = self.purchase_change_fact_context(&changes, executor).await?;
        for change in changes {
            let purchase_no = purchase_nos.get(&change.purchase_order_id.to_string()).cloned();
            let base = context.base_revisions.get(&change.base_revision_id.to_string());
            let submission = change
                .current_submission_id
                .as_ref()
                .and_then(|id| context.submissions.get(&id.to_string()));
            let mut fact = ObjectFact::new(
                change.base.id.clone(),
                purchase_no
                    .as_deref()
                    .map(|no| format!("采购变更单 {no}"))
                    .unwrap_or_else(|| "采购变更单（来源单号待补全）".to_string()),
                change.stable.created_by.clone(),
            );
            fact.counterparty_label = submission
                .map(|item| item.supplier_snapshot.supplier_name.clone())
                .or_else(|| base.map(|item| item.supplier_snapshot.supplier_name.clone()));
            fact.impact_summary = Some("不审批则采购变更不能生效；通过后按目标提交形成新版本".to_string());
            facts.insert((ObjectKind::PurchaseChangeOrder, change.base.id.clone()), fact);
        }
        Ok(())
    }

    async fn sales_change_fact_context(
        &self,
        changes: &[SalesChangeOrder],
        executor: &mut dyn Executor,
    ) -> Result<SalesChangeFactContext> {
        let base_ids = changes
            .iter()
            .map(|change| change.base_revision_id.to_string())
            .collect::<Vec<_>>();
        let base_revisions = self
            .db
            .sales_order_revisions()
            .list_active_by_ids(&base_ids, executor)
            .await?;
        let submission_ids = changes
            .iter()
            .filter_map(|change| change.current_submission_id.clone())
            .collect::<Vec<_>>();
        let submissions = self
            .db
            .sales_change_submissions()
            .list_active_by_ids(
                &submission_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                executor,
            )
            .await?;
        Ok(SalesChangeFactContext {
            base_revisions: base_revisions
                .into_iter()
                .map(|revision| (revision.base.id.clone(), revision))
                .collect(),
            submissions: submissions
                .into_iter()
                .map(|submission| (submission.base.id.clone(), submission))
                .collect(),
        })
    }

    async fn purchase_change_fact_context(
        &self,
        changes: &[PurchaseChangeOrder],
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeFactContext> {
        let base_ids = changes
            .iter()
            .map(|change| change.base_revision_id.to_string())
            .collect::<Vec<_>>();
        let base_revisions = self
            .db
            .purchase_order_revisions()
            .list_active_by_ids(&base_ids, executor)
            .await?;
        let submission_ids = changes
            .iter()
            .filter_map(|change| change.current_submission_id.clone())
            .collect::<Vec<_>>();
        let submissions = self
            .db
            .purchase_change_submissions()
            .list_active_by_ids(
                &submission_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                executor,
            )
            .await?;
        Ok(PurchaseChangeFactContext {
            base_revisions: base_revisions
                .into_iter()
                .map(|revision| (revision.base.id.clone(), revision))
                .collect(),
            submissions: submissions
                .into_iter()
                .map(|submission| (submission.base.id.clone(), submission))
                .collect(),
        })
    }
}
