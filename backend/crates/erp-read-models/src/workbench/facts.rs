//! 责任队列业务对象事实装载与展示映射。

use std::collections::{HashMap, HashSet};

use erp_workflow::entity::work_item::{WorkItemBriefRelation, WorkItemType};
pub(crate) use erp_workflow::ports::ObjectKind;
use erp_workflow::service::work_item::order_access::filter_order_facts;
use erp_workflow::{WorkItemRow, WorkflowAuthorizationPort};
use persistence_core::Executor;

pub(crate) use super::authority::object_ids;
use super::{WorkbenchReadService, brief, dto};
use crate::errors::Result;

#[derive(Debug, Clone, Default)]
pub(crate) struct WorkbenchSubjectDisplay {
    pub(super) counterparty_label: Option<String>,
    pub(super) impact_summary: Option<String>,
    pub(super) brief_source: Option<brief::ObjectBriefSource>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkbenchObjectDisplay {
    /// 页面跳转的父单据；不参与任务阅读或执行授权。
    pub(super) root_document_id: String,
    /// 无独立提交表的单据只证明当前非草稿审批版本。
    pub(super) approval_subject_version: Option<u32>,
    pub(super) label: String,
    pub(super) counterparty_label: Option<String>,
    pub(super) impact_summary: Option<String>,
    pub(super) brief_source: Option<brief::ObjectBriefSource>,
    pub(super) subject_briefs: HashMap<String, WorkbenchSubjectDisplay>,
}

impl WorkbenchObjectDisplay {
    /// 构造对象展示。
    ///
    /// # 参数
    /// * `root_document_id` - 页面跳转的父单据
    /// * `label` - 对象标题
    ///
    /// # 返回
    /// 返回展示覆盖全空的显示。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn new(root_document_id: String, label: String) -> Self {
        Self {
            root_document_id,
            approval_subject_version: None,
            label,
            counterparty_label: None,
            impact_summary: None,
            brief_source: None,
            subject_briefs: HashMap::new(),
        }
    }

    /// 设置往来方与影响覆盖。
    ///
    /// # 参数
    /// * `counterparty_label` - 往来方覆盖
    /// * `impact_summary` - 影响覆盖
    ///
    /// # 返回
    /// 返回更新后的显示。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn with_counterparty_and_impact(
        mut self,
        counterparty_label: Option<String>,
        impact_summary: Option<String>,
    ) -> Self {
        self.counterparty_label = counterparty_label;
        self.impact_summary = impact_summary;
        self
    }

    /// 设置提交版本展示。
    ///
    /// # 参数
    /// * `subject_briefs` - 提交版本展示
    ///
    /// # 返回
    /// 返回更新后的显示。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn with_subject_briefs(
        mut self,
        subject_briefs: HashMap<String, WorkbenchSubjectDisplay>,
    ) -> Self {
        self.subject_briefs = subject_briefs;
        self
    }
}

/// 权限只消费 authority；明确存在的 display 保存原显示覆盖，包括 None。
#[derive(Debug, Clone)]
pub(crate) struct WorkbenchObjectFact {
    pub(super) authority: erp_workflow::ports::ObjectFact,
    pub(super) display: WorkbenchObjectDisplay,
}
impl WorkbenchObjectFact {
    /// 用权威事实生成展示，不另查显示层。
    ///
    /// # 参数
    /// * `authority` - 命令侧对象事实。
    ///
    /// # 返回
    /// 展示的根单据、标题、往来方、影响和提交摘要均复制自 `authority`；简报源保持为空。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn from_authority(authority: erp_workflow::ports::ObjectFact) -> Self {
        let display =
            WorkbenchObjectDisplay::new(authority.root_document_id.clone(), authority.label.clone())
                .with_counterparty_and_impact(
                    authority.counterparty_label.clone(),
                    authority.impact_summary.clone(),
                )
                .with_subject_briefs(
                    authority
                        .subject_briefs
                        .iter()
                        .map(|(key, value)| {
                            (
                                key.clone(),
                                WorkbenchSubjectDisplay {
                                    counterparty_label: value.counterparty_label.clone(),
                                    impact_summary: value.impact_summary.clone(),
                                    brief_source: None,
                                },
                            )
                        })
                        .collect(),
                );
        Self { authority, display }
    }
}

pub(crate) type WorkbenchObjectFactMap = HashMap<(ObjectKind, String), WorkbenchObjectFact>;

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
pub(super) fn apply_object_display(fields: &mut dto::WorkItemFields, fact: &WorkbenchObjectFact) {
    fields.business_object_label = fact.display.label.clone();
    fields.root_business_object_id = fact.display.root_document_id.clone();
    let subject = fact.display.subject_briefs.get(&fields.subject_version);
    apply_subject_display(fields, fact, subject);
    if fields.work_item_type == WorkItemType::DocumentApproval
        && super::approval_list::document_summary(fact, fields.subject_version.parse().ok()).is_none()
        && subject.and_then(|value| value.brief_source.as_ref()).is_none()
    {
        fields.brief_source = None;
        fields.counterparty_label = None;
    }
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
    fact: &WorkbenchObjectFact,
    subject: Option<&WorkbenchSubjectDisplay>,
) {
    if fields.work_item_type == WorkItemType::DocumentApproval {
        fields.counterparty_label = subject.and_then(|item| item.counterparty_label.clone());
        fields.brief_source = subject.and_then(|item| item.brief_source.clone());
        if let Some(impact) = subject.and_then(|item| item.impact_summary.clone()) {
            fields.impact_summary = Some(impact);
        }
        return;
    }
    fields.counterparty_label = subject
        .filter(|item| item.brief_source.is_some())
        .map(|item| item.counterparty_label.clone())
        .unwrap_or_else(|| fact.display.counterparty_label.clone());
    let preserve_task_impact = fields.work_item_type.uses_explicit_owner_authorization()
        && fields.impact_summary.as_deref().is_some_and(|impact| !impact.trim().is_empty());
    if !preserve_task_impact
        && let Some(impact) = subject
            .and_then(|item| item.impact_summary.clone())
            .or_else(|| fact.display.impact_summary.clone())
    {
        fields.impact_summary = Some(impact);
    }
    fields.brief_source =
        subject.map(|item| item.brief_source.clone()).unwrap_or_else(|| fact.display.brief_source.clone());
}

/// 订单范围过滤前已装载的任务身份，只保留放回本人履约事实所需的字段。
pub(super) struct OwnedFulfillmentTask<'a> {
    /// 任务类型。
    pub work_item_type: WorkItemType,
    /// 持久化业务对象类型。
    pub business_object_type: &'a str,
    /// 业务对象主键。
    pub business_object_id: &'a str,
    /// 当前个人责任人；无具体责任人时不放回。
    pub owner_user_id: Option<&'a str>,
}

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 共享命令端的订单来源和公共范围判定；必须先过滤再分页或统计。
    ///
    /// # 参数
    /// * `actor_id` - 当前账号。
    /// * `facts` - 已装载的对象事实；就地删掉订单范围不允许的键。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 过滤完成后 `facts` 只保留仍在授权映射中的键。
    ///
    /// # 错误
    /// 订单来源缺失、错配或授权查询失败时返回错误。
    pub(super) async fn filter_order_access(
        &self,
        actor_id: &str,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut authority = facts.iter().map(|(key, fact)| (key.clone(), fact.authority.clone())).collect();
        filter_order_facts(&self.auth, actor_id, &mut authority, executor).await?;
        facts.retain(|key, _| authority.contains_key(key));
        Ok(())
    }

    /// 按订单范围过滤后，把派给本人的履约任务事实放回。
    ///
    /// # 参数
    /// * `actor_id` - 当前账号
    /// * `tasks` - 本批候选任务
    /// * `facts` - 已装载、待按订单范围收窄的对象事实
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 范围过滤完成后，本人履约任务的对象事实仍在 `facts` 中。
    ///
    /// # 错误
    /// 订单来源缺失、错配或授权查询失败时返回错误，不放回任何事实。
    pub(super) async fn filter_order_access_keeping_owned_fulfillment<'a>(
        &self,
        actor_id: &str,
        tasks: impl IntoIterator<Item = OwnedFulfillmentTask<'a>>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let loaded = facts.clone();
        self.filter_order_access(actor_id, facts, executor).await?;
        restore_owned_fulfillment_facts(tasks, actor_id, &loaded, facts);
        Ok(())
    }
}

/// 销售单详情范围滤掉仓发来源后，本人仍要看到已指派的履约任务。
///
/// 仓储经办人没有销售单详情权限。入库过账生成的仓发任务派给本人，
/// 若随来源销售单一并滤掉，待我处理会是空的，范围内待办按钮也不会出现。
/// 非履约任务和非本人任务保持失败关闭。
fn restore_owned_fulfillment_facts<'a>(
    tasks: impl IntoIterator<Item = OwnedFulfillmentTask<'a>>,
    actor_id: &str,
    loaded: &WorkbenchObjectFactMap,
    facts: &mut WorkbenchObjectFactMap,
) {
    for task in tasks {
        if task.work_item_type != WorkItemType::FulfillmentOperation || task.owner_user_id != Some(actor_id) {
            continue;
        }
        let Some(policy) = object_policy(task.work_item_type, task.business_object_type) else {
            continue;
        };
        let key = (policy.object_kind, task.business_object_id.to_string());
        if facts.contains_key(&key) {
            continue;
        }
        let Some(fact) = loaded.get(&key) else {
            continue;
        };
        facts.insert(key, fact.clone());
    }
}

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 无关键词扫描只装载授权、版本与订单来源；关键词保持原富显示检索语义。
    ///
    /// # 参数
    /// `rows` 为本批候选；`with_display` 表示搜索依赖对象富显示；`executor` 为调用方执行器。
    /// # 返回
    /// 返回与原显示投影共享权威构造器的本批事实。
    /// # 错误
    /// 任一权威或显示事实读取失败时传播原错误。
    pub(super) async fn candidate_object_facts(
        &self,
        rows: &[WorkItemRow],
        with_display: bool,
        executor: &mut dyn Executor,
    ) -> Result<WorkbenchObjectFactMap> {
        if with_display {
            return self.object_facts_for_rows(rows, executor).await;
        }
        let keys = rows
            .iter()
            .filter_map(|row| {
                object_policy(row.work_item_type, &row.business_object_type)
                    .map(|policy| (policy.object_kind, row.business_object_id.clone()))
            })
            .collect();
        Ok(self
            .facts_reader()
            .load(&keys, executor)
            .await?
            .into_iter()
            .map(|(key, fact)| (key, WorkbenchObjectFact::from_authority(fact)))
            .collect())
    }

    /// 已完成授权和分页后只给当前页恢复对象富显示，不改变身份、顺序和任务版本。
    ///
    /// # 参数
    /// `fields` 为当前授权页，`executor` 沿用候选扫描的事务快照。
    /// # 返回
    /// 原地补齐对象展示字段。
    /// # 错误
    /// 页面对象或审批展示读取失败时传播原错误。
    pub(super) async fn page_display_fields(
        &self,
        fields: &mut [dto::WorkItemFields],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let keys = fields
            .iter()
            .filter_map(|item| {
                object_policy(item.work_item_type, &item.business_object_type)
                    .map(|policy| (policy.object_kind, item.business_object_id.clone()))
            })
            .collect();
        let facts = self.load_object_facts(&keys, executor).await?;
        for item in fields {
            if let Some(policy) = object_policy(item.work_item_type, &item.business_object_type)
                && let Some(fact) = facts.get(&(policy.object_kind, item.business_object_id.clone()))
            {
                apply_object_display(item, fact);
            }
        }
        Ok(())
    }

    /// 批量读取当前页任务的权威对象事实，避免按行 N+1。
    ///
    /// # 参数
    /// * `rows` - 当前页工作项行。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回这些行已注册对象的事实；未注册类型不进入映射。
    ///
    /// # 错误
    /// 对象事实读取失败时返回错误。
    pub(super) async fn object_facts_for_rows(
        &self,
        rows: &[erp_workflow::WorkItemRow],
        executor: &mut dyn Executor,
    ) -> Result<WorkbenchObjectFactMap> {
        let keys = rows
            .iter()
            .filter_map(|row| {
                object_policy(row.work_item_type, &row.business_object_type)
                    .map(|policy| (policy.object_kind, row.business_object_id.clone()))
            })
            .collect::<HashSet<_>>();
        self.load_object_facts(&keys, executor).await
    }

    /// 按固定对象注册表分组查询；未注册类型不会进入本映射。
    ///
    /// # 参数
    /// * `keys` - 对象种类与 ID。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回当前业务事实，并补上审批展示。
    ///
    /// # 错误
    /// 业务事实或审批展示读取失败时返回错误。
    pub(super) async fn load_object_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<WorkbenchObjectFactMap> {
        let mut facts = self.load_live_object_facts(keys, executor).await?;
        self.load_approval_displays(keys, &mut facts, executor).await?;
        Ok(facts)
    }

    /// 读取当前业务事实；审批提交捕获与普通展示复用，禁止在此叠加旧快照。
    ///
    /// # 参数
    /// * `keys` - 对象种类与 ID。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回销售、采购、履约、变更、资金及独立对象的当前事实，不含审批历史快照。
    ///
    /// # 错误
    /// 任一类事实读取失败时返回错误。
    pub(super) async fn load_live_object_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<WorkbenchObjectFactMap> {
        let mut facts = WorkbenchObjectFactMap::new();
        self.load_sales_order_facts(keys, &mut facts, executor).await?;
        self.load_purchase_order_facts(keys, &mut facts, executor).await?;
        self.load_fulfillment_operation_facts(keys, &mut facts, executor).await?;
        self.load_purchase_change_facts(keys, &mut facts, executor).await?;
        self.load_sales_change_review_facts(keys, &mut facts, executor).await?;
        self.load_receivable_account_facts(keys, &mut facts, executor).await?;
        self.load_payable_account_facts(keys, &mut facts, executor).await?;
        self.load_customer_receipt_facts(keys, &mut facts, executor).await?;
        self.load_customer_refund_facts(keys, &mut facts, executor).await?;
        self.load_receipt_reversal_facts(keys, &mut facts, executor).await?;
        self.load_supplier_payment_facts(keys, &mut facts, executor).await?;
        self.load_supplier_refund_facts(keys, &mut facts, executor).await?;
        self.load_payment_reversal_facts(keys, &mut facts, executor).await?;
        self.load_independent_object_facts(keys, &mut facts, executor).await?;
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
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.load_stock_adjustment_facts(keys, facts, executor).await?;
        self.load_supplier_settlement_facts(keys, facts, executor).await?;
        self.load_legacy_import_batch_facts(keys, facts, executor).await?;
        self.load_integration_error_task_facts(keys, facts, executor).await?;
        self.load_reconciliation_difference_facts(keys, facts, executor).await?;
        self.load_supplier_fulfillment_order_facts(keys, facts, executor).await?;
        self.load_supplier_offering_facts(keys, facts, executor).await?;
        self.load_supplier_portal_facts(keys, facts, executor).await?;
        self.load_supplier_portal_briefs(keys, facts, executor).await?;
        self.load_operational_briefs(keys, facts, executor).await
    }

    /// 权威申请归属与冻结版本由命令和显示共用的 reader 唯一生成。
    async fn load_supplier_portal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut loaded = erp_workflow::ports::ObjectFactMap::new();
        self.facts_reader().load_supplier_portal_request_facts(keys, &mut loaded, executor).await?;
        facts.extend(loaded.into_iter().map(|(key, fact)| (key, WorkbenchObjectFact::from_authority(fact))));
        Ok(())
    }

    async fn load_legacy_import_batch_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut loaded = erp_workflow::ports::ObjectFactMap::new();
        self.facts_reader().load_legacy_import_batch_facts(keys, &mut loaded, executor).await?;
        facts.extend(loaded.into_iter().map(|(key, fact)| (key, WorkbenchObjectFact::from_authority(fact))));
        Ok(())
    }

    /// 批量读取 W26 供应商履约订单事实，并冻结订单乐观锁版本用于任务对象校验。
    async fn load_supplier_fulfillment_order_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut loaded = erp_workflow::ports::ObjectFactMap::new();
        self.facts_reader().load_supplier_fulfillment_order_facts(keys, &mut loaded, executor).await?;
        facts.extend(loaded.into_iter().map(|(key, fact)| (key, WorkbenchObjectFact::from_authority(fact))));
        Ok(())
    }

    /// 批量读取 W21 供应商供给事实；未建模的供应商外部商品继续失败关闭。
    async fn load_supplier_offering_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut loaded = erp_workflow::ports::ObjectFactMap::new();
        self.facts_reader().load_supplier_offering_facts(keys, &mut loaded, executor).await?;
        facts.extend(loaded.into_iter().map(|(key, fact)| (key, WorkbenchObjectFact::from_authority(fact))));
        Ok(())
    }
}

#[cfg(test)]
mod owned_fulfillment_fact_tests {
    use erp_workflow::entity::work_item::WorkItemType;
    use erp_workflow::ports::{ObjectFact, OrderTaskSource};

    use super::{
        OwnedFulfillmentTask, WorkbenchObjectFact, WorkbenchObjectFactMap, object_policy,
        restore_owned_fulfillment_facts,
    };

    fn delivery_facts(id: &str) -> ((super::ObjectKind, String), WorkbenchObjectFactMap) {
        let policy = object_policy(WorkItemType::FulfillmentOperation, "delivery").expect("仓发已注册");
        let key = (policy.object_kind, id.to_string());
        let fact = WorkbenchObjectFact::from_authority(
            ObjectFact::new("so-1", "仓库发货 · 销售单 SO-1", "system")
                .with_order_source(OrderTaskSource::Sales("so-1".into())),
        );
        let loaded = WorkbenchObjectFactMap::from([(key.clone(), fact)]);
        (key, loaded)
    }

    #[test]
    fn assigned_warehouse_ship_fact_is_restored_after_sales_scope_filter() {
        let (key, loaded) = delivery_facts("delivery-1");
        let mut facts = WorkbenchObjectFactMap::new();
        restore_owned_fulfillment_facts(
            [OwnedFulfillmentTask {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: "delivery",
                business_object_id: "delivery-1",
                owner_user_id: Some("cangchu"),
            }],
            "cangchu",
            &loaded,
            &mut facts,
        );
        assert!(facts.contains_key(&key));
    }

    #[test]
    fn other_owner_and_non_fulfillment_tasks_stay_filtered() {
        let (_key, loaded) = delivery_facts("delivery-1");
        let mut facts = WorkbenchObjectFactMap::new();
        restore_owned_fulfillment_facts(
            [
                OwnedFulfillmentTask {
                    work_item_type: WorkItemType::FulfillmentOperation,
                    business_object_type: "delivery",
                    business_object_id: "delivery-1",
                    owner_user_id: Some("xiaoshou"),
                },
                OwnedFulfillmentTask {
                    work_item_type: WorkItemType::DocumentApproval,
                    business_object_type: "sales_order",
                    business_object_id: "so-1",
                    owner_user_id: Some("cangchu"),
                },
            ],
            "cangchu",
            &loaded,
            &mut facts,
        );
        assert!(facts.is_empty());
    }
}

#[cfg(test)]
mod authority_display_tests {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority};
    use erp_workflow::ports::{ObjectFact, SubjectBrief};

    use super::super::access::{ActorAccess, has_object_participation};
    use super::*;

    fn fields() -> dto::WorkItemFields {
        WorkItem::new(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsReview,
                business_object_type: "receivable_account".to_string(),
                business_object_id: "object".to_string(),
                subject_version: "submission".to_string(),
                owner_role: "role-finance".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "actor".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
        )
        .unwrap()
        .into()
    }

    /// 富显示只影响页面字段；最小权威事实保持授权、版本与来源筛选结果。
    #[test]
    fn candidate_authority_and_rich_display_keep_identical_admission_and_versions() {
        use erp_identity::Permission;
        use erp_workflow::entity::work_item::WorkItemSubjectVersions;
        use erp_workflow::ports::OrderTaskSource;

        let item = WorkItem::new_with_responsibility_key(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: "purchase_receipt".into(),
                business_object_id: "receipt".into(),
                subject_version: "current-version".into(),
                owner_role: "warehouse_inbound_handler".into(),
                owner_organization_id: "warehouse".into(),
                owner_user_id: "actor".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "purchase_order:order",
        )
        .unwrap();
        let row: WorkItemRow = serde_json::from_value(serde_json::to_value(&item).unwrap()).unwrap();
        let policy = object_policy(row.work_item_type, &row.business_object_type).unwrap();
        let access = ActorAccess::new("actor".into())
            .with_permissions(vec![Permission::parse(policy.read_permission).unwrap()]);
        let mut authority = ObjectFact::new("order", "采购入库", "__system__")
            .with_order_source(OrderTaskSource::Purchase("order".into()));
        authority.subject_versions =
            WorkItemSubjectVersions::constrained(vec!["current-version".into()]).unwrap();
        let key = (policy.object_kind, "receipt".into());
        let minimal = WorkbenchObjectFactMap::from([(
            key.clone(),
            WorkbenchObjectFact::from_authority(authority.clone()),
        )]);
        let mut rich_fact = WorkbenchObjectFact::from_authority(authority);
        rich_fact.display.label = "采购入库 · 采购单 PO-1".into();
        rich_fact.display.counterparty_label = Some("供应商名称".into());
        let rich = WorkbenchObjectFactMap::from([(key, rich_fact)]);
        let minimal_fields = super::super::access::authorized_fields(vec![row.clone()], &access, &minimal);
        let rich_fields = super::super::access::authorized_fields(vec![row.clone()], &access, &rich);
        assert_eq!(minimal_fields.len(), 1);
        assert_eq!(rich_fields.len(), 1);
        assert_eq!(minimal_fields[0].id, rich_fields[0].id);
        assert_eq!(minimal_fields[0].task_version, rich_fields[0].task_version);
        assert_eq!(minimal_fields[0].root_business_object_id, rich_fields[0].root_business_object_id);
        let order_ids = vec!["order".to_string()];
        assert!(super::super::query::matches_order_sources(&minimal_fields[0], &minimal, &[], &order_ids));
        assert!(super::super::query::matches_order_sources(&rich_fields[0], &rich, &[], &order_ids));
        assert!(super::super::query::matches_keyword(&rich_fields[0], Some("供应商名称")));
        let mut stale = row.clone();
        stale.subject_version = "old-version".into();
        let mut other = row;
        other.owner_user_id = Some("other-owner".into());
        for facts in [&minimal, &rich] {
            assert!(
                super::super::access::authorized_fields(vec![stale.clone(), other.clone()], &access, facts)
                    .is_empty()
            );
        }
    }

    #[test]
    fn approval_missing_submission_never_inherits_current_impact() {
        let mut fact = WorkbenchObjectFact::from_authority(ObjectFact::new("root", "title", "creator"));
        fact.display.impact_summary = Some("current confidential amount".into());
        fact.display.counterparty_label = Some("current customer".into());
        let mut fields = fields();
        fields.work_item_type = WorkItemType::DocumentApproval;
        apply_object_display(&mut fields, &fact);
        assert_eq!(fields.impact_summary, None);
        assert_eq!(fields.counterparty_label, None);
        assert!(fields.brief_source.is_none());
        fields.impact_summary = Some("frozen task impact".into());
        apply_object_display(&mut fields, &fact);
        assert_eq!(fields.impact_summary.as_deref(), Some("frozen task impact"));
    }

    #[test]
    fn absent_display_counterparty_never_falls_back_to_command_counterparty() {
        let mut authority = ObjectFact::new("participation-root", "command-title", "creator");
        authority.counterparty_label = Some("command-origin-name".to_string());
        let mut fact = WorkbenchObjectFact::from_authority(authority);
        fact.display.label = "display-title".to_string();
        fact.display.counterparty_label = None;
        let mut fields = fields();
        apply_object_display(&mut fields, &fact);
        assert_eq!(fields.business_object_label, "display-title");
        assert_eq!(fields.root_business_object_id, "participation-root");
        assert_eq!(fields.counterparty_label, None);
        assert_eq!(fact.authority.counterparty_label.as_deref(), Some("command-origin-name"));
    }

    #[test]
    fn subject_display_and_authority_impact_remain_separate_and_original_overlay_fallback_is_preserved() {
        let mut authority = ObjectFact::new("root", "title", "creator");
        authority.subject_briefs.insert(
            "submission".to_string(),
            SubjectBrief {
                counterparty_label: Some("authority-subject".to_string()),
                impact_summary: Some("all-source-lines".to_string()),
            },
        );
        let mut fact = WorkbenchObjectFact::from_authority(authority);
        fact.display.counterparty_label = Some("display-root".to_string());
        fact.display.subject_briefs.insert(
            "submission".to_string(),
            WorkbenchSubjectDisplay {
                counterparty_label: None,
                impact_summary: Some("visible-diff-lines".to_string()),
                brief_source: None,
            },
        );
        let mut fields = fields();
        apply_object_display(&mut fields, &fact);
        assert_eq!(fields.counterparty_label.as_deref(), Some("display-root"));
        assert_eq!(fields.impact_summary.as_deref(), Some("visible-diff-lines"));
        assert_eq!(
            fact.authority.subject_briefs["submission"].impact_summary.as_deref(),
            Some("all-source-lines")
        );
    }

    #[test]
    fn participation_uses_authority_root_and_creator_without_responsibility_org_grants() {
        let fact = WorkbenchObjectFact::from_authority(ObjectFact::new(
            "root",
            "display-is-not-an-organization",
            "creator",
        ));
        let mut access = ActorAccess::new("reader".to_string())
            .with_participant_document_ids(HashSet::from(["root".to_string()]))
            .with_managed_scope(Some(Vec::new()), false);
        assert!(has_object_participation(&access, "role", "company", &fact));
        access.participant_document_ids.clear();
        assert!(!has_object_participation(&access, "role", "company", &fact));
        access.actor_id = "creator".to_string();
        assert!(has_object_participation(&access, "role", "company", &fact));
        access.actor_id = "reader".to_string();
        access.can_manage = true;
        access.managed_owner_ids = None;
        assert!(!has_object_participation(&access, "role", "company", &fact));
        assert!(!has_object_participation(&access, "role", "other-company", &fact));
    }
}
