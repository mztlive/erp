//! 责任队列业务对象事实装载与展示映射。

use std::collections::{HashMap, HashSet};

use erp_workflow::entity::work_item::{WorkItemBriefRelation, WorkItemType};
use erp_workflow::ports::ObjectFactMap;
pub(crate) use erp_workflow::ports::ObjectKind;
use futures_util::future::BoxFuture;
use mongodb::Database;
use persistence_core::Executor;

use super::authority::WorkItemFactsReader;
pub(crate) use super::authority::object_ids;
use super::{WorkbenchReadService, brief};
use crate::errors::Result;

/// 工作台对象事实与展示的只读装载器；授权由调用方单独执行。
///
/// 读取不依赖授权 Port 的具体类型，避免为每种授权实现重复生成整条读取链。
pub(super) struct WorkbenchFactsReader<'db> {
    pub(super) db: &'db Database,
}

impl<'db> WorkbenchFactsReader<'db> {
    /// 借用调用方数据库，不执行查询或授权。
    ///
    /// # 参数
    /// * `db` - 与当前用例共用的数据库句柄。
    /// # 返回
    /// 返回不持有授权 Port 的事实读取器。
    /// # 错误
    /// 不返回错误。
    pub(super) fn new(db: &'db Database) -> Self {
        Self { db }
    }

    /// 绑定权威事实来源，沿用各读取步骤传入的执行器。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回与当前读取器共用数据库的权威事实来源。
    /// # 错误
    /// 不返回错误。
    pub(super) fn facts_reader(&self) -> WorkItemFactsReader {
        WorkItemFactsReader::new(self.db.clone())
    }
}

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
    pub(super) delivery_lines: Vec<super::DeliveryTaskLine>,
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
            delivery_lines: Vec::new(),
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

impl WorkbenchReadService {
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
        self.object_reader().load_approval_displays(keys, &mut facts, executor).await?;
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
        let reader = self.object_reader();
        let mut facts = reader.load_document_facts(keys, executor).await?;
        self.load_supplier_portal_briefs(keys, &mut facts, executor).await?;
        reader.load_operational_briefs(keys, &mut facts, executor).await?;
        Ok(facts)
    }
}

impl WorkbenchFactsReader<'_> {
    /// 读取业务对象当前事实；调用方继续按原顺序装载供应商申请人和运营展示。
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
    // 每批只分配一次，将大型异步状态机留在读模型 crate 内生成。
    #[inline(never)]
    fn load_document_facts<'a>(
        &'a self,
        keys: &'a HashSet<(ObjectKind, String)>,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<WorkbenchObjectFactMap>> {
        Box::pin(async move {
            let mut facts = WorkbenchObjectFactMap::new();
            let mut inquiries = ObjectFactMap::new();
            self.facts_reader().load_inquiry_facts(keys, &mut inquiries, executor).await?;
            facts.extend(
                inquiries.into_iter().map(|(key, fact)| (key, WorkbenchObjectFact::from_authority(fact))),
            );

            self.load_sales_order_facts(keys, &mut facts, executor).await?;
            self.load_purchase_order_facts(keys, &mut facts, executor).await?;
            self.load_fulfillment_operation_facts(keys, &mut facts, executor).await?;

            self.load_purchase_change_facts(keys, &mut facts, executor).await?;
            self.load_sales_change_review_facts(keys, &mut facts, executor).await?;

            self.load_receivable_account_facts(keys, &mut facts, executor).await?;
            self.load_payable_account_facts(keys, &mut facts, executor).await?;
            self.load_purchase_payment_request_facts(keys, &mut facts, executor).await?;
            self.load_customer_receipt_facts(keys, &mut facts, executor).await?;
            self.load_customer_refund_facts(keys, &mut facts, executor).await?;
            self.load_receipt_reversal_facts(keys, &mut facts, executor).await?;
            self.load_supplier_payment_facts(keys, &mut facts, executor).await?;
            self.load_supplier_refund_facts(keys, &mut facts, executor).await?;
            self.load_payment_reversal_facts(keys, &mut facts, executor).await?;

            self.load_independent_object_facts(keys, &mut facts, executor).await?;
            Ok(facts)
        })
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
        self.load_supplier_portal_facts(keys, facts, executor).await
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
