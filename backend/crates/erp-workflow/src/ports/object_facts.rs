//! Cross-domain object facts used by work-item authorization.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::{WorkItem, WorkItemBriefObjectKind, WorkItemSubjectVersions};
use crate::error::Result;

/// Work-item brief object kind.
pub type ObjectKind = WorkItemBriefObjectKind;

/// Key for an object-fact lookup.
pub type ObjectFactKey = (ObjectKind, String);

/// Map of loaded object facts.
pub type ObjectFactMap = HashMap<ObjectFactKey, ObjectFact>;

/// 关联任务必须独立读取的 S2 订单；不得使用任务标题、创建人或结算主体推断。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrderTaskSource {
    /// 当前销售单主键，包括沿原单授权的销售变更和发货任务。
    Sales(String),
    /// 当前采购单主键，包括采购变更、入库、电子及服务履约任务。
    Purchase(String),
}

impl OrderTaskSource {
    /// 返回适用 S2 订单读取重验的审批对象种类。
    ///
    /// # 返回
    /// 非订单审批属于其他阶段，返回 None；不得将其伪装成销售单。
    pub fn approval_kind(document_type: DocumentType) -> Option<ObjectKind> {
        match document_type {
            DocumentType::SalesOrder | DocumentType::VoucherSalesOrder => Some(ObjectKind::SalesOrder),
            DocumentType::PurchaseOrder => Some(ObjectKind::PurchaseOrder),
            DocumentType::SalesChangeOrder => Some(ObjectKind::SalesChangeOrder),
            DocumentType::PurchaseChangeOrder => Some(ObjectKind::PurchaseChangeOrder),
            _ => None,
        }
    }
    /// 校验订单种类与任务注册关系一致，拒绝空主键和来源类型错配。
    ///
    /// # 返回
    /// 精确匹配时为 true；展示根节点不能作为缺失来源的替代。
    pub fn matches_kind(&self, kind: ObjectKind) -> bool {
        match self {
            Self::Sales(id) => {
                !id.trim().is_empty()
                    && matches!(
                        kind,
                        ObjectKind::SalesOrder | ObjectKind::SalesChangeOrder | ObjectKind::Delivery
                    )
            }
            Self::Purchase(id) => {
                !id.trim().is_empty()
                    && matches!(
                        kind,
                        ObjectKind::PurchaseOrder
                            | ObjectKind::PurchaseChangeOrder
                            | ObjectKind::PurchaseReceipt
                            | ObjectKind::ElectronicDelivery
                            | ObjectKind::ServiceFulfillment
                    )
            }
        }
    }
    /// 判断对象注册类型是否必须提供订单来源。
    ///
    /// # 返回
    /// S2 订单及其关联履约对象返回 true；其他阶段资源另行接入。
    pub fn required_for(kind: ObjectKind) -> bool {
        matches!(
            kind,
            ObjectKind::SalesOrder
                | ObjectKind::PurchaseOrder
                | ObjectKind::SalesChangeOrder
                | ObjectKind::PurchaseChangeOrder
                | ObjectKind::PurchaseReceipt
                | ObjectKind::Delivery
                | ObjectKind::ElectronicDelivery
                | ObjectKind::ServiceFulfillment
        )
    }
}

/// Subject-level brief overlay.
#[derive(Debug, Clone, Default)]
pub struct SubjectBrief {
    /// Counterparty display name.
    pub counterparty_label: Option<String>,
    /// Impact summary.
    pub impact_summary: Option<String>,
}

/// Minimum object fact used by work-item authorization and labels.
#[derive(Debug, Clone)]
pub struct ObjectFact {
    /// 由业务实体外键提供的订单授权来源；不复用展示根节点或历史参与根节点。
    pub order_scope_source: Option<OrderTaskSource>,
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
    /// Per-subject brief overlays.
    pub subject_briefs: HashMap<String, SubjectBrief>,
}

impl ObjectFact {
    /// Construct an identity-only object fact.
    pub fn new(
        root_document_id: impl Into<String>,
        label: impl Into<String>,
        created_by: impl Into<String>,
    ) -> Self {
        Self {
            order_scope_source: None,
            root_document_id: root_document_id.into(),
            label: label.into(),
            created_by: created_by.into(),
            subject_versions: WorkItemSubjectVersions::unrestricted(),
            counterparty_label: None,
            impact_summary: None,
            subject_briefs: HashMap::new(),
        }
    }

    /// 附加由业务实体证明的 S2 订单来源。
    ///
    /// # 返回
    /// 返回带独立订单读取依据的对象事实，不改变展示或参与关系。
    pub fn with_order_source(mut self, source: OrderTaskSource) -> Self {
        self.order_scope_source = Some(source);
        self
    }
}

/// Normalized W29 close decision returned by the domain adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct W29CloseFact {
    /// Stable close reason stored on the work item.
    pub close_reason: String,
    /// Replacement task id when closing a duplicate.
    pub replacement_work_item_id: Option<String>,
}

impl W29CloseFact {
    /// Historical evidence reference persisted on the domain object.
    pub fn evidence_reference(&self, work_item_id: &str, audit_log_id: &str) -> String {
        match &self.replacement_work_item_id {
            Some(replacement) => format!(
                "work_item:{work_item_id};replacement_work_item:{replacement};audit_log:{audit_log_id}"
            ),
            None => format!("work_item:{work_item_id};audit_log:{audit_log_id}"),
        }
    }
}

/// Loads cross-domain object facts for work-item authorization.
#[async_trait]
pub trait ObjectFactPort: Send + Sync {
    /// Load facts for the requested object keys.
    async fn load_object_facts(
        &self,
        keys: &HashSet<ObjectFactKey>,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFactMap>;

    /// Whether a customer or supplier counterparty is active.
    async fn counterparty_is_active(&self, kind: &str, id: &str, executor: &mut dyn Executor)
        -> Result<bool>;

    /// Display numbers for counterparties of one kind (`supplier` or `customer`).
    async fn counterparty_numbers(
        &self,
        kind: &str,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>>;

    /// Whether an external identity map id exists.
    async fn external_identity_map_exists(&self, id: &str, executor: &mut dyn Executor) -> Result<bool>;

    /// Actors that must stay separated from a reassignment candidate.
    async fn assignment_separation_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

    /// Load open fulfillment tasks for a purchase-order responsibility key after domain gates.
    async fn purchase_order_fulfillment_scope(
        &self,
        selected: &WorkItem,
        purchase_order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(String, Vec<WorkItem>)>;

    /// Reassign the purchase-order owner on the same executor as work-item writes.
    async fn reassign_purchase_order_owner(
        &self,
        purchase_order_id: &str,
        target_user_id: &str,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// Validate a W29 close reason without I/O.
    fn prepare_w29_close(
        &self,
        reason_code: &str,
        comment: Option<&str>,
        replacement_work_item_id: Option<&str>,
    ) -> Result<W29CloseFact>;

    /// Persist W29 domain evidence on the same executor as the work-item close.
    async fn persist_w29_close(
        &self,
        item: &WorkItem,
        decision: &W29CloseFact,
        evidence_reference: &str,
        actor_id: &str,
        receipt_id: &str,
        closed_at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

/// Fail-closed object-fact port used when composition has not injected a domain adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedObjectFactPort;

#[async_trait]
impl ObjectFactPort for FailClosedObjectFactPort {
    async fn load_object_facts(
        &self,
        _keys: &HashSet<ObjectFactKey>,
        _executor: &mut dyn Executor,
    ) -> Result<ObjectFactMap> {
        Ok(ObjectFactMap::new())
    }

    async fn counterparty_is_active(
        &self,
        _kind: &str,
        _id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(false)
    }

    async fn counterparty_numbers(
        &self,
        _kind: &str,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    async fn external_identity_map_exists(&self, _id: &str, _executor: &mut dyn Executor) -> Result<bool> {
        Ok(false)
    }

    async fn assignment_separation_actors(
        &self,
        _item: &WorkItem,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn purchase_order_fulfillment_scope(
        &self,
        _selected: &WorkItem,
        _purchase_order_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<(String, Vec<WorkItem>)> {
        Err(crate::error::Error::Internal("履约责任适配未接线".to_string()))
    }

    async fn reassign_purchase_order_owner(
        &self,
        _purchase_order_id: &str,
        _target_user_id: &str,
        _actor_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(crate::error::Error::Internal("履约责任适配未接线".to_string()))
    }

    fn prepare_w29_close(
        &self,
        _reason_code: &str,
        _comment: Option<&str>,
        _replacement_work_item_id: Option<&str>,
    ) -> Result<W29CloseFact> {
        Err(crate::error::Error::ValidationError(
            "W29 关闭适配未接线，已按安全策略拒绝".to_string(),
        ))
    }

    async fn persist_w29_close(
        &self,
        _item: &WorkItem,
        _decision: &W29CloseFact,
        _evidence_reference: &str,
        _actor_id: &str,
        _receipt_id: &str,
        _closed_at: Instant,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(crate::error::Error::Internal("W29 关闭适配未接线".to_string()))
    }
}
