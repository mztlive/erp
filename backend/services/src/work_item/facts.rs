//! Remaining-domain object facts for work-item authorization.

use erp_integration::repository::IntegrationOpsExt;
use std::collections::{HashMap, HashSet};

use database::{SupplierFulfillmentExt, SupplierOfferingExt};
use entities::supplier_offering::{AvailabilityStatus, OfferingStatus};
use erp_import::LegacyImportExt;
use erp_integration::entity::integration_ops::{ErrorClass, IntegrationErrorTask};
use erp_workflow::entity::work_item::{WorkItemBriefObjectKind, WorkItemSubjectVersions};
use persistence_core::Executor;

use crate::errors::Result;

/// Work-item object kind alias used by remaining-domain fact loaders.
pub type ObjectKind = WorkItemBriefObjectKind;

/// Subject-level counterparty and impact overlay.
#[derive(Debug, Clone, Default)]
pub struct SubjectBrief {
    /// Counterparty display name for this subject version.
    pub counterparty_label: Option<String>,
    /// Impact summary for this subject version.
    pub impact_summary: Option<String>,
}

/// Minimum object fact consumed by the workflow object-fact port.
#[derive(Debug, Clone)]
pub struct ObjectFact {
    /// Work-surface root object id.
    pub root_document_id: String,
    /// User-facing object title.
    pub label: String,
    /// Creator used for participation checks.
    pub created_by: String,
    /// Authoritative subject versions when the domain has a lock version.
    pub subject_versions: WorkItemSubjectVersions,
    /// Counterparty display name.
    pub counterparty_label: Option<String>,
    /// Impact summary.
    pub impact_summary: Option<String>,
    /// Per-subject overlays.
    pub subject_briefs: HashMap<String, SubjectBrief>,
}

impl ObjectFact {
    /// Construct an identity-only object fact.
    pub(super) fn new(
        root_document_id: impl Into<String>,
        label: impl Into<String>,
        created_by: impl Into<String>,
    ) -> Self {
        Self {
            root_document_id: root_document_id.into(),
            label: label.into(),
            created_by: created_by.into(),
            subject_versions: WorkItemSubjectVersions::unrestricted(),
            counterparty_label: None,
            impact_summary: None,
            subject_briefs: HashMap::new(),
        }
    }
}

/// Return the integration-error impact shown on work items.
fn integration_error_impact(task: &IntegrationErrorTask) -> &'static str {
    if task.error_class == ErrorClass::ResultUnknown {
        "外部结果尚未确认，盲目重试可能造成重复写入或重复履约"
    } else {
        "集成异常未处理可能造成业务事实缺失、延迟或上下游不一致"
    }
}

/// Map of loaded remaining-domain object facts.
pub type ObjectFactMap = HashMap<(ObjectKind, String), ObjectFact>;

pub(super) const SYSTEM_OBJECT_OWNER: &str = "__system__";

/// Collect object ids of one kind from a fact-key set.
pub fn object_ids(keys: &HashSet<(ObjectKind, String)>, kind: ObjectKind) -> Vec<String> {
    keys.iter()
        .filter(|(candidate, _)| *candidate == kind)
        .map(|(_, id)| id.clone())
        .collect()
}

impl crate::work_item::ProcessObjectFacts {
    /// Load remaining-domain object facts grouped by the registered object kinds.
    pub async fn load_object_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFactMap> {
        let mut facts = ObjectFactMap::new();
        self.load_sales_order_facts(keys, &mut facts, executor).await?;
        self.load_procurement_confirmation_facts(keys, &mut facts, executor)
            .await?;
        self.load_purchase_order_facts(keys, &mut facts, executor).await?;
        self.load_fulfillment_operation_facts(keys, &mut facts, executor)
            .await?;
        self.load_purchase_change_facts(keys, &mut facts, executor)
            .await?;
        self.load_sales_change_review_facts(keys, &mut facts, executor)
            .await?;
        self.load_receivable_account_facts(keys, &mut facts, executor)
            .await?;
        self.load_payable_account_facts(keys, &mut facts, executor)
            .await?;
        self.load_customer_receipt_facts(keys, &mut facts, executor)
            .await?;
        self.load_customer_refund_facts(keys, &mut facts, executor)
            .await?;
        self.load_receipt_reversal_facts(keys, &mut facts, executor)
            .await?;
        self.load_supplier_payment_facts(keys, &mut facts, executor)
            .await?;
        self.load_supplier_refund_facts(keys, &mut facts, executor)
            .await?;
        self.load_payment_reversal_facts(keys, &mut facts, executor)
            .await?;
        self.load_independent_object_facts(keys, &mut facts, executor)
            .await?;
        Ok(facts)
    }

    /// Load inventory, settlement, import, integration and supply-side facts.
    async fn load_independent_object_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.load_stock_adjustment_facts(keys, facts, executor).await?;
        self.load_supplier_settlement_facts(keys, facts, executor).await?;
        self.load_legacy_import_batch_facts(keys, facts, executor).await?;
        self.load_integration_error_task_facts(keys, facts, executor)
            .await?;
        self.load_reconciliation_difference_facts(keys, facts, executor)
            .await?;
        self.load_supplier_fulfillment_order_facts(keys, facts, executor)
            .await?;
        self.load_supplier_offering_facts(keys, facts, executor).await
    }

    async fn load_legacy_import_batch_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::LegacyImportBatch);
        if ids.is_empty() {
            return Ok(());
        }
        for batch in self
            .db
            .legacy_import_batches()
            .list_active_by_ids(&ids, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::LegacyImportBatch, batch.base.id.clone()),
                ObjectFact::new(
                    batch.base.id.clone(),
                    format!("旧数据导入批次 {}", batch.batch_no),
                    SYSTEM_OBJECT_OWNER,
                ),
            );
        }
        Ok(())
    }

    async fn load_integration_error_task_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::IntegrationErrorTask);
        if ids.is_empty() {
            return Ok(());
        }
        for task in self
            .db
            .integration_error_tasks()
            .list_active_by_ids(&ids, executor)
            .await?
        {
            let owner = task
                .owner_user_id
                .clone()
                .unwrap_or_else(|| SYSTEM_OBJECT_OWNER.to_string());
            let mut fact = ObjectFact::new(
                task.base.id.clone(),
                format!("集成异常 · {}", task.error_class.label()),
                owner,
            );
            fact.impact_summary = Some(integration_error_impact(&task).to_string());
            facts.insert((ObjectKind::IntegrationErrorTask, task.base.id.clone()), fact);
        }
        Ok(())
    }

    async fn load_reconciliation_difference_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReconciliationDifference);
        if ids.is_empty() {
            return Ok(());
        }
        for difference in self
            .db
            .reconciliation_differences()
            .list_active_by_ids(&ids, executor)
            .await?
        {
            let mut fact = ObjectFact::new(
                difference.base.id.clone(),
                format!("业务异常 · {}", difference.difference_type),
                SYSTEM_OBJECT_OWNER,
            );
            fact.impact_summary =
                Some("需核对两侧不可变证据后处理差异，不得直接改写正式业务事实".to_string());
            facts.insert(
                (ObjectKind::ReconciliationDifference, difference.base.id.clone()),
                fact,
            );
        }
        Ok(())
    }

    /// Load W26 supplier fulfillment order facts and freeze the optimistic-lock version.
    async fn load_supplier_fulfillment_order_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierFulfillmentOrder);
        if ids.is_empty() {
            return Ok(());
        }
        for order in self
            .db
            .supplier_fulfillment_orders()
            .list_active_by_ids(&ids, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::SupplierFulfillmentOrder, order.base.id.clone()),
                ObjectFact {
                    root_document_id: order.base.id.clone(),
                    label: format!("供应商履约订单 {}", order.fulfillment_order_no),
                    created_by: SYSTEM_OBJECT_OWNER.to_string(),
                    subject_versions: WorkItemSubjectVersions::constrained(vec![order
                        .base
                        .version
                        .to_string()])?,
                    counterparty_label: None,
                    impact_summary: None,
                    subject_briefs: HashMap::new(),
                },
            );
        }
        Ok(())
    }

    /// Load W21 supplier offering facts; unmodeled external goods stay fail-closed.
    async fn load_supplier_offering_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierOffering);
        if ids.is_empty() {
            return Ok(());
        }
        let offerings = self
            .db
            .supplier_offerings()
            .list_active_by_ids(&ids, executor)
            .await?;
        let offering_ids = offerings
            .iter()
            .map(|offering| erp_core::ids::SupplierOfferingId::new(offering.base.id.clone()))
            .collect::<Vec<_>>();
        let availabilities = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_ids(&offering_ids, executor)
            .await?
            .into_iter()
            .map(|availability| (availability.supplier_offering_id.to_string(), availability))
            .collect::<HashMap<_, _>>();
        for offering in offerings {
            let availability = availabilities.get(&offering.base.id);
            let mut subject_versions = Vec::with_capacity(2);
            if offering.stable.status == OfferingStatus::Stopped {
                subject_versions.push(format!("offering:{}", offering.base.version));
            }
            if let Some(availability) = availability {
                if availability.availability_status == AvailabilityStatus::Stopped {
                    subject_versions.push(format!("availability:{}", availability.base.version));
                }
            }
            if subject_versions.is_empty() {
                continue;
            }
            facts.insert(
                (ObjectKind::SupplierOffering, offering.base.id.clone()),
                ObjectFact {
                    root_document_id: offering.base.id.clone(),
                    label: format!("供应商供给 {}", offering.supplier_sku_code),
                    created_by: offering.stable.created_by,
                    subject_versions: WorkItemSubjectVersions::constrained(subject_versions)?,
                    counterparty_label: None,
                    impact_summary: None,
                    subject_briefs: HashMap::new(),
                },
            );
        }
        Ok(())
    }
}
