//! Purchase-order object facts for remaining-domain work-item authorization.

use std::collections::{HashMap, HashSet};

use database::PurchaseOrderExt;
use entities::purchase_order::{PurchaseOrder, PurchaseOrderSubmission};
use erp_core::ids::PurchaseOrderSubmissionId;
use persistence_core::Executor;

use super::amount::{non_empty, purchase_review_impact_summary};
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, SubjectBrief};
use crate::errors::Result;

struct PurchaseReviewDisplay {
    purchase_order_id: String,
    counterparty: Option<String>,
    impact: String,
}

impl crate::work_item::ProcessObjectFacts {
    /// Load purchase-order identity and per-submission counterparty/impact overlays.
    pub(super) async fn load_purchase_order_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let orders = self.purchase_orders_for_keys(keys, executor).await?;
        if orders.is_empty() {
            return Ok(());
        }
        let displays = self.purchase_review_displays(&orders, executor).await?;
        insert_purchase_order_facts(facts, &orders, &displays);
        Ok(())
    }

    async fn purchase_orders_for_keys(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrder>> {
        let ids = object_ids(keys, ObjectKind::PurchaseOrder);
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .db
            .purchase_orders()
            .list_active_by_ids(&ids, executor)
            .await?)
    }

    async fn purchase_review_displays(
        &self,
        orders: &[PurchaseOrder],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, PurchaseReviewDisplay>> {
        let submissions = self.purchase_submissions_for_orders(orders, executor).await?;
        let line_counts = self
            .purchase_submission_line_counts(&submissions, executor)
            .await?;
        Ok(assemble_purchase_review_displays(&submissions, &line_counts))
    }

    async fn purchase_submissions_for_orders(
        &self,
        orders: &[PurchaseOrder],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderSubmission>> {
        let order_ids = orders
            .iter()
            .map(|order| order.base.id.clone())
            .collect::<Vec<_>>();
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .db
            .purchase_order_submissions()
            .list_work_item_brief_submissions_by_orders(&order_ids, executor)
            .await?)
    }

    async fn purchase_submission_line_counts(
        &self,
        submissions: &[PurchaseOrderSubmission],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, usize>> {
        let submission_ids = submissions
            .iter()
            .map(|item| PurchaseOrderSubmissionId::new(item.base.id.clone()))
            .collect::<Vec<_>>();
        if submission_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut counts = HashMap::new();
        for line in self
            .db
            .purchase_order_submission_lines()
            .find_lines_by_submission_ids(&submission_ids, executor)
            .await?
        {
            *counts
                .entry(line.purchase_order_submission_id.to_string())
                .or_default() += 1;
        }
        Ok(counts)
    }
}

fn insert_purchase_order_facts(
    facts: &mut ObjectFactMap,
    orders: &[PurchaseOrder],
    displays: &HashMap<String, PurchaseReviewDisplay>,
) {
    for order in orders {
        facts.insert(
            (ObjectKind::PurchaseOrder, order.base.id.clone()),
            purchase_order_fact(order, displays),
        );
    }
}

fn purchase_order_fact(
    order: &PurchaseOrder,
    displays: &HashMap<String, PurchaseReviewDisplay>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        order.base.id.clone(),
        format!("采购单 {}", order.purchase_no),
        order.stable.created_by.clone(),
    );
    for (submission_id, display) in displays {
        if display.purchase_order_id != order.base.id {
            continue;
        }
        let brief = SubjectBrief {
            counterparty_label: display.counterparty.clone(),
            impact_summary: Some(display.impact.clone()),
        };
        if order.current_submission_id.as_deref() == Some(submission_id.as_str()) {
            fact.counterparty_label = brief.counterparty_label.clone();
            fact.impact_summary = brief.impact_summary.clone();
        }
        fact.subject_briefs.insert(submission_id.clone(), brief);
    }
    fact
}

fn assemble_purchase_review_displays(
    submissions: &[PurchaseOrderSubmission],
    line_counts: &HashMap<String, usize>,
) -> HashMap<String, PurchaseReviewDisplay> {
    let previous_ids = previous_formal_submission_ids(submissions);
    submissions
        .iter()
        .map(|submission| {
            let previous = previous_ids.contains_key(&submission.base.id);
            (
                submission.base.id.clone(),
                purchase_review_display(
                    submission,
                    line_counts.get(&submission.base.id).copied(),
                    previous,
                ),
            )
        })
        .collect()
}

fn previous_formal_submission_ids(submissions: &[PurchaseOrderSubmission]) -> HashMap<String, String> {
    let mut grouped: HashMap<String, Vec<(u32, String)>> = HashMap::new();
    for submission in submissions {
        let Some(sequence) = submission.formal_sequence() else {
            continue;
        };
        grouped
            .entry(submission.purchase_order_id.to_string())
            .or_default()
            .push((sequence, submission.base.id.clone()));
    }
    let mut previous = HashMap::new();
    for mut values in grouped.into_values() {
        values.sort_by_key(|(sequence, _)| *sequence);
        for pair in values.windows(2) {
            let [(_, before), (_, after)] = pair else {
                continue;
            };
            previous.insert(after.clone(), before.clone());
        }
    }
    previous
}

fn purchase_review_display(
    submission: &PurchaseOrderSubmission,
    line_count: Option<usize>,
    previous: bool,
) -> PurchaseReviewDisplay {
    let supplier = non_empty(&submission.supplier_snapshot.supplier_name);
    let mut impact = purchase_review_impact_summary(
        line_count.filter(|count| *count > 0),
        Some(&submission.gross_amount),
        submission.payment_term_snapshot.prepay_gate,
    );
    if previous {
        impact.push_str("；本次为再次提交，必须先核对前后差异");
    }
    PurchaseReviewDisplay {
        purchase_order_id: submission.purchase_order_id.to_string(),
        counterparty: supplier,
        impact,
    }
}
