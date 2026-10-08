//! 其余领域的工作项授权对象事实。

use std::collections::{HashMap, HashSet};

use erp_import::LegacyImportExt;
use erp_integration::entity::integration_ops::{ErrorClass, IntegrationErrorTask};
use erp_supply::entity::supplier_offering::{AvailabilityStatus, OfferingStatus};
use erp_supply::repository::prelude::*;
use erp_supply::repository::{SupplierFulfillmentExt, SupplierOfferingExt};
use erp_workflow::entity::work_item::WorkItemSubjectVersions;
use persistence_core::Executor;

use super::{ObjectFact, ObjectFactMap, ObjectKind};
use crate::errors::Result;

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

pub(in crate::workbench) const SYSTEM_OBJECT_OWNER: &str = "__system__";

/// 从事实键集合中收集指定对象种类的 ID。
///
/// # 参数
/// * `keys` - 对象种类与 ID。
/// * `kind` - 要收集的对象种类。
///
/// # 返回
/// 返回该种类的全部 ID。
///
/// # 错误
/// 不返回错误。
pub fn object_ids(keys: &HashSet<(ObjectKind, String)>, kind: ObjectKind) -> Vec<String> {
    keys.iter().filter(|(candidate, _)| *candidate == kind).map(|(_, id)| id.clone()).collect()
}

impl super::WorkItemFactsReader {
    /// 按注册对象种类装载其余领域的权威事实。
    ///
    /// # 参数
    /// * `keys` - 本批对象键。
    /// * `executor` - 调用方执行器；各步复用同一个。
    ///
    /// # 返回
    /// 返回各命令步骤累积的事实表。
    ///
    /// # 错误
    /// 任一步读取失败时立即返回该错误。
    pub async fn load(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFactMap> {
        super::recipe::load(self, keys, executor).await
    }

    /// 装载仍有效的旧数据导入批次事实；创建人固定为系统对象。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有导入批次键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入读到的批次，标题使用批次号。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误。
    pub(in crate::workbench) async fn load_legacy_import_batch_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::LegacyImportBatch);
        if ids.is_empty() {
            return Ok(());
        }
        for batch in self.db.legacy_import_batches().list_active_by_ids(&ids, executor).await? {
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
        for task in self.read_integration_errors(&ids, executor).await? {
            let fact = integration_error_fact(&task);
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
        for difference in self.read_reconciliation_differences(&ids, executor).await? {
            let fact = reconciliation_difference_fact(&difference);
            facts.insert((ObjectKind::ReconciliationDifference, difference.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 装载供应商履约订单事实，并冻结乐观锁版本。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有履约订单键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入订单号标题、系统创建人，以及当前 `base.version` 约束。
    ///
    /// # 错误
    /// 仓储读取失败，或版本约束无法构造时返回错误。
    pub(in crate::workbench) async fn load_supplier_fulfillment_order_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierFulfillmentOrder);
        if ids.is_empty() {
            return Ok(());
        }
        for order in self.db.supplier_fulfillment_orders().list_active_by_ids(&ids, executor).await? {
            facts.insert(
                (ObjectKind::SupplierFulfillmentOrder, order.base.id.clone()),
                ObjectFact {
                    order_scope_source: None,
                    root_document_id: order.base.id.clone(),
                    label: format!("供应商履约订单 {}", order.fulfillment_order_no),
                    created_by: SYSTEM_OBJECT_OWNER.to_string(),
                    subject_versions: WorkItemSubjectVersions::constrained(vec![
                        order.base.version.to_string(),
                    ])?,
                    counterparty_label: None,
                    impact_summary: None,
                    subject_briefs: HashMap::new(),
                },
            );
        }
        Ok(())
    }

    /// 装载已停止的供应商供给事实；供给与可供状态都未停止时不写入。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有供给键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 供给状态为 `Stopped` 时写入 `offering:` 版本；可供状态为 `Stopped` 时写入 `availability:` 版本。两者都未停止则跳过该供给。
    ///
    /// # 错误
    /// 仓储读取失败，或版本约束无法构造时返回错误。
    pub(in crate::workbench) async fn load_supplier_offering_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierOffering);
        if ids.is_empty() {
            return Ok(());
        }
        let offerings = self.db.supplier_offerings().list_active_by_ids(&ids, executor).await?;
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
            if let Some(availability) = availability
                && availability.availability_status == AvailabilityStatus::Stopped
            {
                subject_versions.push(format!("availability:{}", availability.base.version));
            }
            if subject_versions.is_empty() {
                continue;
            }
            facts.insert(
                (ObjectKind::SupplierOffering, offering.base.id.clone()),
                ObjectFact {
                    order_scope_source: None,
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

/// 从已读对象构造唯一权威事实，不加载结构化显示证据。
///
/// # 参数
/// * `task` - 集成异常任务。
///
/// # 返回
/// 返回以任务 ID 为根的事实。创建人取 `owner_user_id`，缺失时用系统对象，并写入安全影响说明。
///
/// # 错误
/// 不返回错误。
pub(in crate::workbench) fn integration_error_fact(
    task: &erp_integration::entity::integration_ops::IntegrationErrorTask,
) -> ObjectFact {
    let owner = task.owner_user_id.clone().unwrap_or_else(|| SYSTEM_OBJECT_OWNER.to_string());
    let mut fact =
        ObjectFact::new(task.base.id.clone(), format!("集成异常 · {}", task.error_class.label()), owner);
    fact.impact_summary = Some(integration_error_impact(task).to_string());
    fact
}

/// 从已读对象构造唯一权威事实，不加载结构化显示证据。
///
/// # 参数
/// * `difference` - 对账差异。
///
/// # 返回
/// 返回以差异 ID 为根、系统对象为创建人的事实，并写入不得直接改写正式事实的影响说明。
///
/// # 错误
/// 不返回错误。
pub(in crate::workbench) fn reconciliation_difference_fact(
    difference: &erp_integration::entity::integration_ops::ReconciliationDifference,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        difference.base.id.clone(),
        format!("业务异常 · {}", difference.difference_type),
        SYSTEM_OBJECT_OWNER,
    );
    fact.impact_summary = Some("需核对两侧不可变证据后处理差异，不得直接改写正式业务事实".to_string());
    fact
}

#[async_trait::async_trait]
impl super::recipe::CommandFactReads for super::WorkItemFactsReader {
    async fn read(
        &self,
        step: super::recipe::Step,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        use super::recipe::Step;
        match step {
            Step::Sales => self.load_sales_order_facts(keys, facts, executor).await,
            Step::Purchase => self.load_purchase_order_facts(keys, facts, executor).await,
            Step::Fulfillment => self.load_fulfillment_operation_facts(keys, facts, executor).await,
            Step::PurchaseChange => self.load_purchase_change_facts(keys, facts, executor).await,
            Step::SalesChange => self.load_sales_change_review_facts(keys, facts, executor).await,
            Step::Receivable => self.load_receivable_account_facts(keys, facts, executor).await,
            Step::Payable => self.load_payable_account_facts(keys, facts, executor).await,
            Step::CustomerReceipt => self.load_customer_receipt_facts(keys, facts, executor).await,
            Step::CustomerRefund => self.load_customer_refund_facts(keys, facts, executor).await,
            Step::ReceiptReversal => self.load_receipt_reversal_facts(keys, facts, executor).await,
            Step::SupplierPayment => self.load_supplier_payment_facts(keys, facts, executor).await,
            Step::SupplierRefund => self.load_supplier_refund_facts(keys, facts, executor).await,
            Step::PaymentReversal => self.load_payment_reversal_facts(keys, facts, executor).await,
            Step::Inventory => self.load_stock_adjustment_facts(keys, facts, executor).await,
            Step::Settlement => self.load_supplier_settlement_facts(keys, facts, executor).await,
            Step::LegacyImport => self.load_legacy_import_batch_facts(keys, facts, executor).await,
            Step::IntegrationError => self.load_integration_error_task_facts(keys, facts, executor).await,
            Step::Reconciliation => self.load_reconciliation_difference_facts(keys, facts, executor).await,
            Step::SupplierFulfillment => {
                self.load_supplier_fulfillment_order_facts(keys, facts, executor).await
            },
            Step::SupplierOffering => self.load_supplier_offering_facts(keys, facts, executor).await,
            Step::SupplierPortal => self.load_supplier_portal_request_facts(keys, facts, executor).await,
        }
    }
}
