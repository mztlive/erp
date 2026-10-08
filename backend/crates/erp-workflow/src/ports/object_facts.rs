//! 工作项授权使用的跨域对象事实。

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
    /// # 参数
    /// * `document_type` - 审批单据类型。
    ///
    /// # 返回
    /// 销售、卡券销售、采购及其变更单返回对应对象种类；其他类型返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
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
    /// # 参数
    /// * `kind` - 任务注册的对象种类。
    ///
    /// # 返回
    /// 主键非空白且种类与销售或采购来源的注册关系一致时返回 `true`，否则返回 `false`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn matches_kind(&self, kind: ObjectKind) -> bool {
        match self {
            Self::Sales(id) => {
                !id.trim().is_empty()
                    && matches!(
                        kind,
                        ObjectKind::SalesOrder | ObjectKind::SalesChangeOrder | ObjectKind::Delivery
                    )
            },
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
            },
        }
    }
    /// 判断对象注册类型是否必须提供订单来源。
    ///
    /// # 参数
    /// * `kind` - 对象注册种类。
    ///
    /// # 返回
    /// 销售、采购、其变更单，以及收货、发货、电子交付或服务履约返回 `true`，其余返回 `false`。
    ///
    /// # 错误
    /// 不返回错误。
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

    /// 按请求销售/采购 ID 判定权威来源是否命中列表收窄条件。
    ///
    /// # 参数
    /// * `source` - 业务实体证明的权威订单来源；缺失表示无来源事实
    /// * `sales_ids` - 请求的销售单 ID，已去重排序；空表示不过滤
    /// * `purchase_ids` - 请求的采购单 ID，已去重排序；空表示不过滤
    ///
    /// # 返回
    /// 两组均为空时返回 `true`；否则同字段 OR、不同字段 AND，缺来源或类型错配返回 `false`。
    ///
    /// # 错误
    /// 无。
    pub fn matches_requested_sources(
        source: Option<&Self>,
        sales_ids: &[String],
        purchase_ids: &[String],
    ) -> bool {
        if sales_ids.is_empty() && purchase_ids.is_empty() {
            return true;
        }
        let sales_ok = sales_ids.is_empty()
            || matches!(source, Some(Self::Sales(id)) if sales_ids.iter().any(|want| want == id));
        let purchase_ok = purchase_ids.is_empty()
            || matches!(source, Some(Self::Purchase(id)) if purchase_ids.iter().any(|want| want == id));
        sales_ok && purchase_ok
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
    /// 构造只含身份的对象事实；订单来源、版本约束和摘要为空。
    ///
    /// # 参数
    /// * `root_document_id` - 工作面根对象 ID。
    /// * `label` - 面向用户的对象标题。
    /// * `created_by` - 用于参与校验的创建人。
    ///
    /// # 返回
    /// 返回不限制对象版本的事实。
    ///
    /// # 错误
    /// 不返回错误。
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
    /// # 参数
    /// * `source` - 业务实体证明的订单来源。
    ///
    /// # 返回
    /// 返回带独立订单读取依据的对象事实，不改变展示或参与关系。消耗 `self`。
    ///
    /// # 错误
    /// 不返回错误。
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
    /// 返回写入领域对象的历史证据引用。
    ///
    /// # 参数
    /// * `work_item_id` - 被关闭的工作项 ID。
    /// * `command_receipt_id` - 命令收据 ID。
    ///
    /// # 返回
    /// 有替代任务时包含替代工作项；否则只包含工作项和命令收据。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn evidence_reference(&self, work_item_id: &str, command_receipt_id: &str) -> String {
        match &self.replacement_work_item_id {
            Some(replacement) => format!(
                "work_item:{work_item_id};replacement_work_item:{replacement};command_receipt:{command_receipt_id}"
            ),
            None => format!("work_item:{work_item_id};command_receipt:{command_receipt_id}"),
        }
    }
}

/// Loads cross-domain object facts for work-item authorization.
#[async_trait]
pub trait ObjectFactPort: Send + Sync {
    /// 按请求的对象键加载事实。
    ///
    /// # 参数
    /// * `keys` - 对象种类与主键。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回已加载的对象事实映射。
    ///
    /// # 错误
    /// 实现无法读取对象事实时返回错误。
    async fn load_object_facts(
        &self,
        keys: &HashSet<ObjectFactKey>,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFactMap>;

    /// 判断客户或供应商往来方是否有效。
    ///
    /// # 参数
    /// * `kind` - 往来方种类。
    /// * `id` - 往来方 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 有效时返回 `true`，否则返回 `false`。
    ///
    /// # 错误
    /// 实现无法确认往来方状态时返回错误。
    async fn counterparty_is_active(&self, kind: &str, id: &str, executor: &mut dyn Executor)
    -> Result<bool>;

    /// 返回同一往来方种类的展示编号。
    ///
    /// # 参数
    /// * `kind` - `supplier` 或 `customer`。
    /// * `ids` - 往来方 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回 ID 到展示编号的映射。
    ///
    /// # 错误
    /// 实现无法读取展示编号时返回错误。
    async fn counterparty_numbers(
        &self,
        kind: &str,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>>;

    /// 判断外部身份映射 ID 是否存在。
    ///
    /// # 参数
    /// * `id` - 外部身份映射 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 存在时返回 `true`，否则返回 `false`。
    ///
    /// # 错误
    /// 实现无法确认映射是否存在时返回错误。
    async fn external_identity_map_exists(&self, id: &str, executor: &mut dyn Executor) -> Result<bool>;

    /// 返回必须与转派候选人保持分离的操作人。
    ///
    /// # 参数
    /// * `item` - 待转派的工作项。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回不得接收该任务的账号 ID。
    ///
    /// # 错误
    /// 实现无法解析分离对象时返回错误。
    async fn assignment_separation_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

    /// 在领域门禁之后，按采购单责任键加载开放履约任务。
    ///
    /// # 参数
    /// * `selected` - 当前选中的工作项。
    /// * `purchase_order_id` - 采购单 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回责任键，以及该键下的开放履约任务。
    ///
    /// # 错误
    /// 实现无法完成加载时返回错误。
    async fn purchase_order_fulfillment_scope(
        &self,
        selected: &WorkItem,
        purchase_order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(String, Vec<WorkItem>)>;

    /// 在与工作项写入相同的执行器上改派采购单负责人。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单 ID。
    /// * `target_user_id` - 新负责人。
    /// * `actor_id` - 当前操作人。
    /// * `executor` - 与工作项写入相同的执行器。
    ///
    /// # 返回
    /// 无返回值；采购单负责人已改派。
    ///
    /// # 错误
    /// 实现无法完成改派时返回错误。
    async fn reassign_purchase_order_owner(
        &self,
        purchase_order_id: &str,
        target_user_id: &str,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 改派 W29 对象当前处理人，同步实体与任务组织。
    ///
    /// # 参数
    /// * `item` - 已改派用户的正式任务
    /// * `target_user_id` - 接收人
    /// * `executor` - 与任务写入相同的执行器
    ///
    /// # 返回
    /// 非 W29 对象时成功且不写入。
    ///
    /// # 错误
    /// 处理人缺少有效内部组织、对象不存在或适配未接线时拒绝。
    async fn reassign_integration_handler(
        &self,
        item: &mut WorkItem,
        target_user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 校验 W29 关闭原因，不执行 I/O。
    ///
    /// # 参数
    /// * `reason_code` - 关闭原因代码。
    /// * `comment` - 可选关闭说明。
    /// * `replacement_work_item_id` - 重复关闭时的替代任务 ID。
    ///
    /// # 返回
    /// 返回规范化后的关闭事实。
    ///
    /// # 错误
    /// 关闭原因无法通过校验时返回错误。
    fn prepare_w29_close(
        &self,
        reason_code: &str,
        comment: Option<&str>,
        replacement_work_item_id: Option<&str>,
    ) -> Result<W29CloseFact>;

    /// 在与工作项关闭相同的执行器上写入 W29 领域证据。
    ///
    /// # 参数
    /// * `item` - 被关闭的工作项。
    /// * `decision` - 已校验的关闭事实。
    /// * `evidence_reference` - 写入领域对象的证据引用。
    /// * `actor_id` - 关闭操作人。
    /// * `receipt_id` - 命令收据 ID。
    /// * `closed_at` - 关闭时间。
    /// * `executor` - 与工作项关闭相同的执行器。
    ///
    /// # 返回
    /// 无返回值；领域证据已写入。
    ///
    /// # 错误
    /// 实现无法写入证据时返回错误。
    // 事务内证据写入：db 经执行器 + 审计字段 + 收据标识顺序敏感，拆包会破坏调用点可读性；告警逐项压制。
    #[allow(clippy::too_many_arguments)]
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

    async fn reassign_integration_handler(
        &self,
        item: &mut WorkItem,
        _target_user_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        if matches!(
            item.business_object_type.as_str(),
            "integration_error_task" | "reconciliation_difference"
        ) {
            return Err(crate::error::Error::Internal("集成改派适配未接线".to_string()));
        }
        Ok(())
    }

    fn prepare_w29_close(
        &self,
        _reason_code: &str,
        _comment: Option<&str>,
        _replacement_work_item_id: Option<&str>,
    ) -> Result<W29CloseFact> {
        Err(crate::error::Error::ValidationError("W29 关闭适配未接线，已按安全策略拒绝".to_string()))
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

#[cfg(test)]
mod tests {
    use super::OrderTaskSource;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn requested_sources_match_by_field_or_and_across_fields() {
        let sales = Some(OrderTaskSource::Sales("so-1".to_string()));
        let purchase = Some(OrderTaskSource::Purchase("po-1".to_string()));
        assert!(OrderTaskSource::matches_requested_sources(None, &[], &[]));
        assert!(OrderTaskSource::matches_requested_sources(sales.as_ref(), &ids(&["so-1"]), &[]));
        assert!(OrderTaskSource::matches_requested_sources(purchase.as_ref(), &[], &ids(&["po-1"])));
        assert!(!OrderTaskSource::matches_requested_sources(sales.as_ref(), &ids(&["so-2"]), &[]));
        assert!(!OrderTaskSource::matches_requested_sources(purchase.as_ref(), &[], &ids(&["po-2"])));
    }

    #[test]
    fn requested_sources_fail_closed_on_missing_or_mismatched_source() {
        let sales = Some(OrderTaskSource::Sales("so-1".to_string()));
        assert!(!OrderTaskSource::matches_requested_sources(None, &[], &ids(&["po-1"])));
        assert!(!OrderTaskSource::matches_requested_sources(None, &ids(&["so-1"]), &[]));
        assert!(!OrderTaskSource::matches_requested_sources(
            sales.as_ref(),
            &ids(&["so-1"]),
            &ids(&["po-1"])
        ));
        assert!(!OrderTaskSource::matches_requested_sources(sales.as_ref(), &[], &ids(&["po-1"])));
    }
}
