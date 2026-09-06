//! 责任队列业务对象事实装载与展示映射。

use std::collections::{HashMap, HashSet};

use database::{IntegrationOpsExt, LegacyImportExt, SupplierFulfillmentExt, SupplierOfferingExt};
use entities::integration_ops::{ErrorClass, IntegrationErrorTask, ReconciliationDifference};
use entities::supplier_offering::{AvailabilityStatus, OfferingStatus};
use entities::work_item::{
    WorkItemBriefObjectKind, WorkItemBriefRelation, WorkItemSubjectVersions, WorkItemType,
};
use erp_core::common::time::Instant;
use persistence_core::{Executor, NoTransaction};

use crate::errors::Result;

use super::brief;
use super::dto;
use super::WorkItemService;

/// 工作项简报事实装载使用的实体对象种类别名。
pub(super) type ObjectKind = WorkItemBriefObjectKind;

#[derive(Debug, Clone, Default)]
pub(super) struct SubjectBrief {
    pub(super) counterparty_label: Option<String>,
    pub(super) impact_summary: Option<String>,
    pub(super) brief_source: Option<brief::ObjectBriefSource>,
}

#[derive(Debug, Clone)]
pub(super) struct ObjectFact {
    pub(super) root_document_id: String,
    pub(super) label: String,
    pub(super) created_by: String,
    /// 生产者合同允许的权威版本；无约束值对象表示该领域没有通用锁版本约束。
    pub(super) subject_versions: WorkItemSubjectVersions,
    pub(super) counterparty_label: Option<String>,
    pub(super) impact_summary: Option<String>,
    pub(super) brief_source: Option<brief::ObjectBriefSource>,
    pub(super) subject_briefs: HashMap<String, SubjectBrief>,
}

impl ObjectFact {
    /// 构造只有身份标题的对象事实。
    ///
    /// # 参数
    /// * `root_document_id` - 工作面根对象 ID
    /// * `label` - 面向用户的对象标题
    /// * `created_by` - 对象创建人，用于参与权判断
    ///
    /// # 返回
    /// 返回无往来方、无影响覆盖的对象事实。
    ///
    /// # 错误
    /// 无。
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
            brief_source: None,
            subject_briefs: HashMap::new(),
        }
    }
}

/// 组装集成错误任务的结构化简报。
///
/// # 参数
/// * `task` - 集成错误任务正式事实
///
/// # 返回
/// 返回错误分类、关联参考号、发生时间、重试证据、脱敏摘要和处理结果。
///
/// # 错误
/// 无。
fn integration_error_brief_source(task: &IntegrationErrorTask) -> brief::ObjectBriefSource {
    let occurred_at = base_created_at_datetime(task.base.created_at);
    let last_attempt_at = task.last_attempt_at.map(brief::format_instant_datetime);
    let attempt_count = format!("{} 次", task.attempt_count);
    let resolved_at = task.resolved_at.map(brief::format_instant_datetime);
    let resolution_type = task.resolution_type.map(|value| value.label().to_string());
    let reference = task
        .business_object_id
        .clone()
        .or_else(|| task.message_id.as_ref().map(ToString::to_string));
    let mut sections = Vec::new();
    brief::push_section(&mut sections, "错误分类", Some(task.error_class.label()), false);
    brief::push_section(&mut sections, "状态", Some(task.status.label()), false);
    brief::push_section(
        &mut sections,
        "业务对象参考号",
        task.business_object_id.as_deref(),
        false,
    );
    let message_id = task.message_id.as_ref().map(ToString::to_string);
    brief::push_section(&mut sections, "关联消息", message_id.as_deref(), false);
    brief::push_section(&mut sections, "发生时间", occurred_at.as_deref(), false);
    brief::push_section(&mut sections, "重试记录", Some(attempt_count.as_str()), false);
    brief::push_section(&mut sections, "最近尝试", last_attempt_at.as_deref(), false);
    brief::push_section(
        &mut sections,
        "错误摘要",
        task.last_attempt_summary.as_deref(),
        false,
    );
    brief::push_section(&mut sections, "责任角色", task.owner_role.as_deref(), false);
    brief::push_section(&mut sections, "责任人", task.owner_user_id.as_deref(), false);
    brief::push_section(
        &mut sections,
        "安全下一步",
        Some(integration_error_next_step(task.error_class)),
        false,
    );
    brief::push_section(&mut sections, "解决方式", resolution_type.as_deref(), false);
    brief::push_section(&mut sections, "处理证据", task.resolution.as_deref(), false);
    brief::push_section(&mut sections, "完成时间", resolved_at.as_deref(), false);
    brief::ObjectBriefSource {
        customer: None,
        amount_label: None,
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
        list_summary: brief::join_list_summary([
            Some(task.error_class.label().to_string()),
            reference,
            Some(format!("重试 {attempt_count}")),
            task.last_attempt_summary.as_deref().and_then(brief::non_empty),
        ]),
        extra_sections: sections,
    }
}

/// 返回集成错误对业务处理的安全影响说明。
///
/// # 参数
/// * `task` - 集成错误任务正式事实
///
/// # 返回
/// 结果未知返回防重复写入说明，其余分类返回通用缺失或重复风险说明。
///
/// # 错误
/// 无。
fn integration_error_impact(task: &IntegrationErrorTask) -> &'static str {
    if task.error_class == ErrorClass::ResultUnknown {
        "外部结果尚未确认，盲目重试可能造成重复写入或重复履约"
    } else {
        "集成异常未处理可能造成业务事实缺失、延迟或上下游不一致"
    }
}

/// 按固定错误分类返回可执行且安全的下一步。
///
/// # 参数
/// * `error_class` - 错误分类
///
/// # 返回
/// 返回不泄露内部实现的处理指引。
///
/// # 错误
/// 无。
fn integration_error_next_step(error_class: ErrorClass) -> &'static str {
    match error_class {
        ErrorClass::CapabilityGap => "确认目标系统能力后转人工补偿或补齐能力",
        ErrorClass::MappingError => "修复映射并验证业务键后再重放",
        ErrorClass::BusinessRejected => "核对拒绝原因并修正业务输入后重新提交",
        ErrorClass::TransientFailure | ErrorClass::RateLimited => "核对最近尝试摘要，按原幂等业务键重试",
        ErrorClass::ResultUnknown => "先查询原请求结果，确认无结果后才允许重放",
        ErrorClass::AuthSignature => "修复鉴权或签名配置，验证通过后再重试",
        ErrorClass::OutOfOrder => "补齐前置事实并确认顺序后再重放",
    }
}

/// 组装对账差异的结构化业务异常简报。
///
/// # 参数
/// * `difference` - 不可变对账差异事实
///
/// # 返回
/// 返回异常对象、差异类型、发现时间与两侧证据引用。
///
/// # 错误
/// 无。
fn reconciliation_difference_brief_source(difference: &ReconciliationDifference) -> brief::ObjectBriefSource {
    let occurred_at = base_created_at_datetime(difference.base.created_at);
    let evidence_count = usize::from(difference.left_fact_reference.is_some())
        + usize::from(difference.right_fact_reference.is_some());
    let evidence_summary = format!("{evidence_count} 侧证据");
    let mut sections = Vec::new();
    brief::push_section(
        &mut sections,
        "异常对象",
        Some(difference.business_object_type.as_str()),
        false,
    );
    brief::push_section(
        &mut sections,
        "外部/业务参考号",
        Some(difference.business_object_id.as_str()),
        false,
    );
    brief::push_section(
        &mut sections,
        "差异类型",
        Some(difference.difference_type.as_str()),
        false,
    );
    brief::push_section(&mut sections, "发现时间", occurred_at.as_deref(), false);
    brief::push_section(
        &mut sections,
        "左侧证据",
        difference.left_fact_reference.as_deref(),
        false,
    );
    brief::push_section(
        &mut sections,
        "右侧证据",
        difference.right_fact_reference.as_deref(),
        false,
    );
    brief::push_section(
        &mut sections,
        "关闭条件",
        Some("两侧事实已核对，并引用正式处理结果或无需处理的证据"),
        false,
    );
    brief::ObjectBriefSource {
        customer: None,
        amount_label: None,
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
        list_summary: brief::join_list_summary([
            Some(difference.business_object_type.clone()),
            Some(difference.business_object_id.clone()),
            Some(difference.difference_type.clone()),
            Some(evidence_summary),
        ]),
        extra_sections: sections,
    }
}

/// 把实体基础时间转换为业务时区展示；非法或测试零值不上屏。
///
/// # 参数
/// * `created_at` - 实体 Unix 秒级创建时间
///
/// # 返回
/// 返回分钟级时间；零值或超出 `i64` 时返回 `None`。
///
/// # 错误
/// 无。
fn base_created_at_datetime(created_at: u64) -> Option<String> {
    (created_at > 0)
        .then(|| i64::try_from(created_at).ok())
        .flatten()
        .map(Instant::from_unix_secs)
        .map(brief::format_instant_datetime)
}

pub(super) type ObjectFactMap = HashMap<(ObjectKind, String), ObjectFact>;

pub(super) const SYSTEM_OBJECT_OWNER: &str = "__system__";

/// 解析实体注册的工作项简报关系。
///
/// # 参数
/// * `work_item_type` - 工作项类型
/// * `business_object_type` - 工作项持久化的业务对象类型
///
/// # 返回
/// 已注册组合返回权威对象种类与读取权限；未注册组合返回 `None`。
///
/// # 错误
/// 无。
pub(super) fn object_policy(
    work_item_type: WorkItemType,
    business_object_type: &str,
) -> Option<&'static WorkItemBriefRelation> {
    work_item_type.brief_relation(business_object_type)
}

/// 从批量对象键中提取指定实体种类的稳定 ID。
///
/// # 参数
/// * `keys` - 工作项关系解析形成的对象键集合
/// * `kind` - 待装载的权威业务对象种类
///
/// # 返回
/// 返回该种类的对象 ID 集合。
///
/// # 错误
/// 无。
pub(super) fn object_ids(keys: &HashSet<(ObjectKind, String)>, kind: ObjectKind) -> Vec<String> {
    keys.iter()
        .filter(|(candidate, _)| *candidate == kind)
        .map(|(_, id)| id.clone())
        .collect()
}

/// 把对象事实中的标题、往来方和影响写回任务投影字段。
///
/// # 参数
/// * `fields` - 待覆盖的任务字段
/// * `fact` - 已授权对象事实
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
pub(super) fn apply_object_display(fields: &mut dto::WorkItemFields, fact: &ObjectFact) {
    fields.business_object_label = fact.label.clone();
    fields.root_business_object_id = fact.root_document_id.clone();
    let subject = fact.subject_briefs.get(&fields.subject_version);
    apply_subject_display(fields, fact, subject);
}

/// 按任务针对的提交版本覆盖往来方、影响和事项简报。
///
/// # 参数
/// * `fields` - 待覆盖的任务字段
/// * `fact` - 对象级默认展示
/// * `subject` - 与 `subject_version` 对应的提交展示；缺失时回退对象默认值
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
fn apply_subject_display(
    fields: &mut dto::WorkItemFields,
    fact: &ObjectFact,
    subject: Option<&SubjectBrief>,
) {
    fields.counterparty_label = subject
        .and_then(|item| item.counterparty_label.clone())
        .or_else(|| fact.counterparty_label.clone());
    let preserve_task_impact = fields.work_item_type.uses_explicit_owner_authorization()
        && fields
            .impact_summary
            .as_deref()
            .is_some_and(|impact| !impact.trim().is_empty());
    if !preserve_task_impact {
        if let Some(impact) = subject
            .and_then(|item| item.impact_summary.clone())
            .or_else(|| fact.impact_summary.clone())
        {
            fields.impact_summary = Some(impact);
        }
    }
    fields.brief_source = subject
        .and_then(|item| item.brief_source.clone())
        .or_else(|| fact.brief_source.clone());
}

impl WorkItemService {
    /// 批量读取当前页任务的权威对象事实，避免按行 N+1。
    pub(super) async fn object_facts_for_rows(
        &self,
        rows: &[database::WorkItemRow],
    ) -> Result<ObjectFactMap> {
        let keys = rows
            .iter()
            .filter_map(|row| {
                object_policy(row.work_item_type, &row.business_object_type)
                    .map(|policy| (policy.object_kind, row.business_object_id.clone()))
            })
            .collect::<HashSet<_>>();
        self.load_object_facts(&keys, &mut NoTransaction).await
    }

    /// 按固定对象注册表分组查询；未注册类型不会进入本映射。
    pub(super) async fn load_object_facts(
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

    /// 装载库存、结算、导入、集成和供应侧对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入已注册独立对象的事实。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
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
            fact.brief_source = Some(integration_error_brief_source(&task));
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?
        {
            let mut fact = ObjectFact::new(
                difference.base.id.clone(),
                format!("业务异常 · {}", difference.difference_type),
                SYSTEM_OBJECT_OWNER,
            );
            fact.impact_summary =
                Some("需核对两侧不可变证据后处理差异，不得直接改写正式业务事实".to_string());
            fact.brief_source = Some(reconciliation_difference_brief_source(&difference));
            facts.insert(
                (ObjectKind::ReconciliationDifference, difference.base.id.clone()),
                fact,
            );
        }
        Ok(())
    }

    /// 批量读取 W26 供应商履约订单事实，并冻结订单乐观锁版本用于任务对象校验。
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
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
                    brief_source: None,
                    subject_briefs: HashMap::new(),
                },
            );
        }
        Ok(())
    }

    /// 批量读取 W21 供应商供给事实；未建模的供应商外部商品继续失败关闭。
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
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
                    brief_source: None,
                    subject_briefs: HashMap::new(),
                },
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod integration_brief_tests {
    use entities::integration_ops::{
        ErrorClass, IntegrationErrorTask, IntegrationErrorTaskData, ReconciliationDifference,
        ReconciliationDifferenceData,
    };
    use erp_core::common::time::Instant;
    use erp_core::ids::{IntegrationErrorTaskId, ReconciliationDifferenceId};

    use super::{integration_error_brief_source, reconciliation_difference_brief_source};

    #[test]
    fn integration_brief_exposes_retry_and_redacted_error_evidence() {
        let mut task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new("integration-1"),
            IntegrationErrorTaskData {
                message_id: None,
                business_object_id: Some("EXT-2026-001".to_string()),
                error_class: ErrorClass::ResultUnknown,
                owner_role: Some("integration-operator".to_string()),
                owner_user_id: Some("operator-1".to_string()),
            },
        )
        .unwrap();
        task.attempt_count = 2;
        task.last_attempt_at = Some(Instant::from_unix_secs(1_787_457_600));
        task.last_attempt_summary = Some("目标系统超时，未取得业务结果".to_string());

        let brief = integration_error_brief_source(&task);

        assert!(brief
            .extra_sections
            .iter()
            .any(|section| { section.label == "业务对象参考号" && section.value == "EXT-2026-001" }));
        assert!(brief
            .extra_sections
            .iter()
            .any(|section| section.label == "重试记录" && section.value == "2 次"));
        assert!(brief.list_summary.contains("目标系统超时"));
    }

    #[test]
    fn reconciliation_brief_exposes_both_immutable_evidence_references() {
        let difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("difference-1"),
            ReconciliationDifferenceData {
                business_object_type: "商城订单".to_string(),
                business_object_id: "MALL-1001".to_string(),
                difference_type: "金额不一致".to_string(),
                left_fact_reference: Some("mall-snapshot:7".to_string()),
                right_fact_reference: Some("erp-revision:9".to_string()),
            },
        )
        .unwrap();

        let brief = reconciliation_difference_brief_source(&difference);

        assert!(brief
            .extra_sections
            .iter()
            .any(|section| section.label == "左侧证据"));
        assert!(brief
            .extra_sections
            .iter()
            .any(|section| section.label == "右侧证据"));
        assert!(brief.list_summary.contains("2 侧证据"));
    }
}
