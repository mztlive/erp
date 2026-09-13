//! 销售单最终通过：包装既有 `formalize_submission` 仓储端口。

use std::collections::BTreeMap;

use erp_sales::repository::SalesOrderExt;

use erp_core::common::time::Instant;
use erp_core::ids::{SalesOrderId, WorkItemId};
use erp_sales::entity::sales_order::{
    procurement_responsibility_key, SalesOrder, SalesOrderRevisionAggregate, SalesOrderSubmission,
    SalesOrderSubmissionLine,
};
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use erp_workflow::WorkItemExt;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::adapter::sales_order_responsible_org_id;
use super::procurement::submission_procurement_inputs;
use super::SalesOrderCommandProcess;
use crate::procure_to_pay::responsibility::{
    AuthorizedResolutionPlan, ProcurementResponsibilityProcess, ResolutionInput,
};
use crate::{Error, Result};
use application_core::AuditActor;
use erp_sales::service::sales_order::formalize::{build_revision_for_order, load_latest_submission};
use erp_sales::service::sales_order::lifecycle::ensure_final_approve_formalize;

/// 事务外授权并在销售形式化事务内重验的采购责任计划。
pub(super) struct ProcurementFormalizationPlan {
    pub(super) inputs: Vec<ResolutionInput>,
    pub(super) resolution: AuthorizedResolutionPlan,
}

/// 销售形式化事务需要一次性消费的完整写入上下文。
pub struct FormalizedSubmissionWrite {
    pub(super) db: Database,
    pub(super) rbac: erp_identity::SharedRbacService,
    pub(super) order_id: String,
    pub(super) order: SalesOrder,
    pub(super) submission: SalesOrderSubmission,
    pub(super) aggregate: SalesOrderRevisionAggregate,
    pub(super) procurement: Option<ProcurementFormalizationPlan>,
    pub(super) procurement_items: Vec<WorkItem>,
    pub(super) now: Instant,
}

impl SalesOrderCommandProcess {
    /// 准备最终通过的销售事实；重复形式化返回 `None`，由外层流程复用原回执。
    ///
    /// # 错误
    /// 状态、提交或采购责任授权不满足原合同则失败。
    pub async fn prepare_approved_submission(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<FormalizedSubmissionWrite>> {
        let mut order = self
            .db
            .sales_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.is_fully_formalized() {
            return Ok(None);
        }
        ensure_final_approve_formalize(&order)?;
        let (submission, lines) = load_latest_submission(&self.db, id, executor).await?;
        let procurement = self.build_procurement_formalization_plan(&order, &lines).await?;
        prepare_formalized_submission_write(
            &self.db,
            self.require_rbac()?.clone(),
            &mut order,
            submission,
            lines,
            procurement,
            actor,
        )
        .map(Some)
    }

    /// 为实物及服务销售单构造事务外授权的采购责任计划。
    ///
    /// # 参数
    /// * `order` - 待最终生效销售单
    /// * `lines` - 最新冻结提交行
    ///
    /// # 返回
    /// 卡券销售单返回 `None`；实物服务单返回逐行具体负责人计划。
    ///
    /// # 错误
    /// 任一行责任无法确定或负责人不合格时失败关闭。
    async fn build_procurement_formalization_plan(
        &self,
        order: &SalesOrder,
        lines: &[SalesOrderSubmissionLine],
    ) -> Result<Option<ProcurementFormalizationPlan>> {
        if !order.business_type.is_goods_service() {
            return Ok(None);
        }
        let inputs = submission_procurement_inputs(lines)?;
        let resolution = ProcurementResponsibilityProcess::new(self.db.clone(), self.require_rbac()?.clone())
            .resolve_strict(&inputs)
            .await?;
        Ok(Some(ProcurementFormalizationPlan { inputs, resolution }))
    }
}

/// 完成销售形式化的纯领域计算并生成待写上下文。
///
/// # 错误
/// 状态、版本或任务字段不合法时返回错误。
fn prepare_formalized_submission_write(
    db: &Database,
    rbac: erp_identity::SharedRbacService,
    order: &mut SalesOrder,
    mut submission: SalesOrderSubmission,
    lines: Vec<SalesOrderSubmissionLine>,
    procurement: Option<ProcurementFormalizationPlan>,
    actor: &AuditActor,
) -> Result<FormalizedSubmissionWrite> {
    let now = Instant::now();
    let aggregate = build_revision_for_order(order, &submission, &lines, now)?;
    let procurement_items = procurement
        .as_ref()
        .map(|plan| build_procurement_work_items(order, &submission, plan))
        .transpose()?
        .unwrap_or_default();
    erp_sales::service::sales_order::formalize::approve_submission(
        order,
        &mut submission,
        &aggregate,
        now,
        actor.id(),
    )?;
    Ok(FormalizedSubmissionWrite {
        db: db.clone(),
        rbac: rbac.clone(),
        order_id: order.base.id.clone(),
        order: order.clone(),
        submission,
        aggregate,
        procurement,
        procurement_items,
        now,
    })
}

/// 按负责人分组构造供给分配任务。
///
/// # 参数
/// * `order` - 待生效销售单
/// * `submission` - 冻结提交
/// * `plan` - 已授权逐行责任计划
///
/// # 返回
/// 返回每位具体负责人一条任务，责任键冻结稳定销售行集合。
///
/// # 错误
/// 责任组织缺失、行集合为空或任务字段非法时返回错误。
fn build_procurement_work_items(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    plan: &ProcurementFormalizationPlan,
) -> Result<Vec<WorkItem>> {
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for line in &plan.resolution.lines {
        groups
            .entry(line.identity.owner_user_id.clone())
            .or_default()
            .push(line.identity.line_key.clone());
    }
    let organization_id = sales_order_responsible_org_id(order)?;
    groups
        .into_iter()
        .map(|(owner_user_id, mut line_ids)| {
            line_ids.sort();
            line_ids.dedup();
            let responsibility_key = procurement_responsibility_key(&line_ids)?;
            let line_count = line_ids.len();
            WorkItem::new_with_responsibility_scope(
                WorkItemId::new(next_id()),
                WorkItemData {
                    work_item_type: WorkItemType::ProcurementOrderCreation,
                    business_object_type: "sales_order".to_string(),
                    business_object_id: order.base.id.clone(),
                    subject_version: submission.base.id.clone(),
                    owner_role: "role-procurement".to_string(),
                    owner_organization_id: organization_id.clone(),
                    owner_user_id,
                    assignment_source: AssignmentSource::SystemRule,
                    priority: WorkItemPriority::Normal,
                    due_at: None,
                    reason_code: Some("SALES_ORDER_EFFECTIVE".to_string()),
                    impact_summary: Some(format!("销售单 {} 的 {line_count} 行待分配供给", order.order_no)),
                },
                responsibility_key,
                line_ids,
            )
            .map_err(Error::Logic)
        })
        .collect()
}

/// 幂等写入同一销售生效事务中的供给分配任务。
///
/// # 参数
/// * `db` - 数据库
/// * `items` - 按负责人分组后的完整任务集合
/// * `session` - 销售形式化事务会话
///
/// # 返回
/// 全部任务已存在或全部创建成功时返回 `Ok(())`。
///
/// # 错误
/// 同一责任键存在多条开放任务或任一写入失败时返回错误并回滚事务。
pub(super) async fn persist_procurement_work_items(
    db: &Database,
    items: &[WorkItem],
    session: &mut dyn Executor,
) -> Result<()> {
    for item in items {
        let responsibility_key = item
            .responsibility_key()
            .ok_or_else(|| Error::Internal("供给分配任务缺少责任键".to_string()))?;
        let existing = db
            .work_items()
            .list_open_procurement_by_responsibility(
                &SalesOrderId::new(item.business_object_id.clone()),
                responsibility_key,
                session,
            )
            .await?;
        if existing.len() > 1 {
            return Err(Error::ConflictError(
                "同一销售责任行集合存在多条开放供给分配任务".to_string(),
            ));
        }
        if let Some(existing) = existing.first() {
            if existing.responsibility_scope_ids() != item.responsibility_scope_ids() {
                return Err(Error::ConflictError(
                    "开放供给分配任务的冻结责任范围与当前解析不一致".to_string(),
                ));
            }
        }
        if existing.is_empty() {
            db.work_items().create(item, session).await?;
        }
    }
    Ok(())
}

impl FormalizedSubmissionWrite {
    /// 待形式化销售单标识，供组合层在入事务前创建原审计记录。
    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    /// 本次冻结采购责任的授权版本，供流程保持原 CAS 事务栅栏。
    pub fn policy_revision(&self) -> Option<u64> {
        self.procurement
            .as_ref()
            .map(|plan| plan.resolution.policy_revision)
    }
}

/// 验证销售形式化事务的状态、仓储与供给任务合同。
#[cfg(test)]
mod tests {
    use super::{ensure_final_approve_formalize, procurement_responsibility_key};
    use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId};
    use erp_finance::entity::receivable::{AccountReviewStatus, SalesBusinessTypeFact};
    use erp_sales::entity::sales_order::{
        BusinessType, CommercialStatus, ReviewStatus, SalesOrder, SalesOrderData,
    };

    fn draft_order() -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("so-1"),
            SalesOrderData {
                sales_owner_user_id: "admin-1".to_string(),
                order_no: "SO-1".into(),
                business_type: BusinessType::GoodsService,
                origin_system: erp_sales::entity::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "user-1",
        )
        .expect("草稿必须可构造")
    }

    /// 验证销售形式化的状态闸门、仓储入口与采购授权提交栅栏。
    ///
    /// 生产代码必须只接受审批中状态，并通过 policy CAS 提交采购责任授权快照。
    #[test]
    fn formalize_wraps_repository_and_only_accepts_in_approval() {
        let source = concat!(
            include_str!("formalization_root.rs"),
            include_str!("formalization_posting.rs"),
            include_str!("formalize.rs"),
            include_str!("../../../erp-sales/src/service/sales_order/formalize.rs")
        );
        let production = source.split("/// 验证销售形式化事务").next().expect("生产代码");
        let formalize_at = production
            .find("formalize::persist_revision(")
            .expect("写入销售当前版本");
        let synchronize_at = production[formalize_at..]
            .find("sync_procurement_tasks_for_sales_order(")
            .map(|offset| formalize_at + offset)
            .expect("校准供给分配任务");
        assert!(
            synchronize_at > formalize_at,
            "供给任务必须在销售当前版本落库后按权威覆盖量校准"
        );
        assert!(production.contains("ensure_final_approve_formalize"));
        assert!(production.contains("run_authorized_policy_transaction(policy_revision"));
        assert!(!production.contains("CARD_SALES_APPROVAL"));
        let mut order = draft_order();
        assert!(ensure_final_approve_formalize(&order).is_err());
        order.start_approval_submission("user-1").expect("提交进入审批中");
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        assert!(ensure_final_approve_formalize(&order).is_ok());
        order.commercial_status = CommercialStatus::Effective;
        order.review_status = ReviewStatus::Approved;
        assert!(order.is_fully_formalized());
        assert!(ensure_final_approve_formalize(&order).is_err());
    }

    #[test]
    fn procurement_responsibility_key_is_stable_and_boundary_safe() {
        let first = procurement_responsibility_key(&["line-1".to_string(), "line-23".to_string()])
            .expect("稳定行集合合法");
        let repeated = procurement_responsibility_key(&["line-1".to_string(), "line-23".to_string()])
            .expect("重复计算合法");
        let different = procurement_responsibility_key(&["line-12".to_string(), "line-3".to_string()])
            .expect("不同边界集合合法");

        assert_eq!(first, repeated);
        assert_ne!(first, different);
        assert!(first.starts_with("sales-lines:"));
        assert!(procurement_responsibility_key(&[]).is_err());
    }

    /// 卡券最终通过同样只接受审批中.
    #[test]
    fn voucher_formalize_accepts_in_approval() {
        let mut order = draft_order();
        order.business_type = BusinessType::Voucher;
        assert!(ensure_final_approve_formalize(&order).is_err());
        order.start_approval_submission("user-1").expect("卡券进入审批中");
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        assert!(ensure_final_approve_formalize(&order).is_ok());
        assert_eq!(
            AccountReviewStatus::initial_for_sales_business_type(SalesBusinessTypeFact::Voucher),
            AccountReviewStatus::NotApplicable
        );
        assert_eq!(
            AccountReviewStatus::initial_for_sales_business_type(SalesBusinessTypeFact::GoodsService),
            AccountReviewStatus::NotApplicable
        );
    }
}
