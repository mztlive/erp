//! 已授权销售单的当前办理摘要；不授予关联单据读取或任务操作权限。
mod repository;

use std::collections::HashMap;

use erp_core::common::time::Instant;
use erp_identity::AccessControlExt;
use erp_identity::repository::organization::OrganizationRepository;
use erp_identity::repository::prelude::*;
use erp_sales::dto::sales_order::RevisionView;
use erp_sales::entity::sales_order::{CommercialStatus, SalesOrder};
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use persistence_core::NoTransaction;
use repository::CurrentWorkRepository;
use serde::Serialize;

use super::SalesOrderReadService;
use crate::Result;

/// 当前办理的最小公开摘要；不包含内部业务备注、金额、任务权限或关联单据链接。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderCurrentWork {
    /// 稳定展示键，不作为任务操作授权。
    pub id: String,
    /// 固定任务种类，供页面判定重复提示，不直接展示枚举。
    pub kind: WorkItemType,
    /// 关联对象身份，仅用于分组计数，不作为读取授权。
    pub object_id: String,
    /// 关联对象种类，供区分入库与发货等环节。
    pub object_type: String,
    /// 来源采购单的业务编号；缺失时不展示内部身份。
    pub document_number: Option<String>,
    /// 详情页内导航分区。
    pub section: &'static str,
    /// 当前环节中文名称。
    pub label: &'static str,
    /// 责任岗位中文名称。
    pub role_label: &'static str,
    /// 实际任务责任人；缺失时表示尚未分配。
    pub owner_user_id: Option<String>,
    /// 实际任务责任人姓名；无法解析时保留空值，不以历史操作人补齐。
    pub owner_name: Option<String>,
    /// 当前处理人的有效主属组织名称；不使用业务单所属公司替代部门。
    pub owner_org_name: Option<String>,
    /// 本任务负责的商品范围；只取当前销售版本的商品名称。
    pub scope: Vec<String>,
    /// 下一步办理事项。
    pub instruction: &'static str,
}

impl SalesOrderReadService {
    /// 在销售详情授权内汇总开放责任，只返回该销售单及其直接来源对象的办理摘要。
    /// # 参数
    /// `order` 为已授权销售单，`revisions` 为该单已读取的版本快照。
    /// # 返回
    /// 返回按环节和创建顺序稳定排列的当前任务；终结任务与无关任务不进入结果。
    /// # 错误
    /// 关联读取、责任读取或姓名查询失败时返回错误，不伪装成无人处理。
    pub(super) async fn current_work(
        &self,
        order: &SalesOrder,
        revisions: &[RevisionView],
    ) -> Result<Vec<SalesOrderCurrentWork>> {
        if order.commercial_status == CommercialStatus::Voided {
            return Ok(Vec::new());
        }
        let current = CurrentWorkRepository { db: &self.db }.tasks(order).await?;
        let tasks = current.tasks;
        let ids = tasks.iter().filter_map(|task| task.owner_user_id.clone()).collect::<Vec<_>>();
        let names = self.db.accounts().names_by_ids(&ids, &mut NoTransaction).await?;
        let org_names = OrganizationRepository::new(&self.db)
            .primary_org_names(&ids, Instant::now(), &mut NoTransaction)
            .await?;
        let lines = revisions
            .iter()
            .find(|revision| Some(revision.id.as_str()) == order.current_revision_id())
            .map(|revision| {
                revision
                    .commercial_lines
                    .iter()
                    .map(|line| {
                        (
                            line.sales_order_line_id.clone(),
                            format!("第 {} 项 · {}", line.line_no, line.item_name_snapshot),
                        )
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let mut rows = tasks
            .iter()
            .filter_map(|task| SalesOrderCurrentWork::from_task(task, &names, &lines))
            .collect::<Vec<_>>();
        for row in &mut rows {
            row.owner_org_name = row.owner_user_id.as_ref().and_then(|id| org_names.get(id).cloned());
            if row.object_type == "purchase_order" {
                row.document_number = current.purchase_numbers.get(&row.object_id).cloned();
            }
        }
        rows.sort_by_key(|row| match row.section {
            "approval" => 0,
            "fulfillment" => 1,
            "acceptance" => 2,
            "invoice" => 3,
            _ => 4,
        });
        Ok(rows)
    }
}

impl SalesOrderCurrentWork {
    /// 从开放任务映射有限展示信息，不使用自由文本摘要或历史责任人。
    fn from_task(
        task: &WorkItem,
        names: &HashMap<String, String>,
        lines: &HashMap<String, String>,
    ) -> Option<Self> {
        if task.status != WorkItemStatus::Open {
            return None;
        }
        let (section, label, role_label, instruction) = task_copy(task)?;
        Some(Self {
            id: task.base.id.clone(),
            kind: task.work_item_type,
            object_id: task.business_object_id.clone(),
            object_type: task.business_object_type.clone(),
            document_number: None,
            section,
            label,
            role_label,
            instruction,
            owner_user_id: task.owner_user_id.clone(),
            owner_name: task.owner_user_id.as_ref().and_then(|id| names.get(id).cloned()),
            owner_org_name: None,
            scope: if task.work_item_type == WorkItemType::ProcurementOrderCreation {
                task.responsibility_scope_ids().iter().filter_map(|id| lines.get(id).cloned()).collect()
            } else {
                Vec::new()
            },
        })
    }
}

/// 固定任务类型与对象对应的中文环节、岗位及动作；未知关系不作推断。
fn task_copy(task: &WorkItem) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    use WorkItemType::*;
    match (task.work_item_type, task.business_object_type.as_str()) {
        (ProcurementOrderCreation, "sales_order") => {
            Some(("fulfillment", "待安排采购 / 库存出库", "采购", "按负责商品安排采购或库存出库。"))
        },
        (PurchaseSourceUpdate, "purchase_order") => {
            Some(("fulfillment", "待更新采购依据", "采购", "按销售变更调整采购内容并提交。"))
        },
        (PurchaseOrderReview | DocumentApproval, "purchase_order") => {
            Some(("fulfillment", "采购审批中", "审批", "完成采购单当前节点审批。"))
        },
        (ApprovalRework, "purchase_order") => {
            Some(("fulfillment", "采购待修改重提", "采购", "按审批意见修改采购单并重新提交。"))
        },
        (CustomerAcceptanceRegistration, "sales_order") => {
            Some(("acceptance", "待客户验收", "销售", "核对客户实际收货结果并登记验收。"))
        },
        (SalesInvoiceExecution, "receivable_account" | "sales_invoice_request") => {
            Some(("invoice", "待开票", "财务", "按开票申请执行开票。"))
        },
        (DocumentApproval, "sales_invoice_request") => {
            Some(("invoice", "开票审批中", "审批", "完成开票申请当前节点审批。"))
        },
        (ApprovalRework, "sales_invoice_request") => {
            Some(("invoice", "开票申请待修改", "申请人", "按审批意见修改开票申请并重新提交。"))
        },
        (DocumentApproval, "sales_order" | "voucher_sales_order") => {
            Some(("approval", "销售审批中", "审批", "完成销售单当前节点审批。"))
        },
        _ => fulfillment_copy(task),
    }
}

/// 履约岗位以任务冻结角色为准，未知角色不替换为采购。
fn fulfillment_copy(task: &WorkItem) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    use WorkItemType::FulfillmentOperation;
    match (task.work_item_type, task.business_object_type.as_str()) {
        (FulfillmentOperation, "purchase_receipt") => {
            Some(("fulfillment", "待采购入库", "仓储", "核对到货并登记采购入库。"))
        },
        (FulfillmentOperation, "delivery") => Some((
            "fulfillment",
            "待发货",
            match task.owner_role.as_str() {
                "warehouse_outbound_handler" => "仓储",
                "purchase_order_owner" => "采购",
                _ => "交付",
            },
            "完成发货并登记物流信息。",
        )),
        (FulfillmentOperation, "electronic_delivery") => {
            Some(("fulfillment", "待电子交付", "采购", "完成电子交付并确认交付结果。"))
        },
        (FulfillmentOperation, "service_fulfillment") => {
            Some(("fulfillment", "待服务交付", "采购", "完成服务并登记履约结果。"))
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};

    use super::*;

    fn task(owner: &str, scope: &str) -> WorkItem {
        WorkItem::new_with_responsibility_scope(
            WorkItemId::new(format!("task-{owner}")),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: "sales_order".into(),
                business_object_id: "sale".into(),
                subject_version: "revision-1".into(),
                owner_role: "procurement".into(),
                owner_organization_id: "company".into(),
                owner_user_id: owner.into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: Some("不得展示内部备注".into()),
            },
            owner,
            vec![scope.into()],
        )
        .unwrap()
    }

    #[test]
    fn parallel_procurement_preserves_each_owner_and_product_scope() {
        let names = HashMap::from([("buyer-a".into(), "张采购".into()), ("buyer-b".into(), "李采购".into())]);
        let lines = HashMap::from([
            ("line-a".into(), "第 1 项 · 茶叶".into()),
            ("line-b".into(), "第 2 项 · 咖啡".into()),
        ]);
        let first = SalesOrderCurrentWork::from_task(&task("buyer-a", "line-a"), &names, &lines).unwrap();
        let second = SalesOrderCurrentWork::from_task(&task("buyer-b", "line-b"), &names, &lines).unwrap();
        assert_eq!(first.owner_name.as_deref(), Some("张采购"));
        assert_eq!(first.scope, vec!["第 1 项 · 茶叶"]);
        assert_eq!(second.owner_name.as_deref(), Some("李采购"));
        assert_eq!(second.scope, vec!["第 2 项 · 咖啡"]);
        assert_eq!(first.section, "fulfillment");
    }

    #[test]
    fn completed_and_closed_tasks_never_become_current() {
        let mut item = task("buyer", "line");
        for status in [WorkItemStatus::Completed, WorkItemStatus::Closed] {
            item.status = status;
            assert!(SalesOrderCurrentWork::from_task(&item, &HashMap::new(), &HashMap::new()).is_none());
        }
    }

    #[test]
    fn missing_name_and_unassigned_task_do_not_fall_back_to_historical_actor() {
        let mut item = task("buyer", "removed-line");
        item.completed_by = Some("old-reviewer".into());
        let names = HashMap::from([("old-reviewer".into(), "历史审批人".into())]);
        let missing = SalesOrderCurrentWork::from_task(&item, &names, &HashMap::new()).unwrap();
        assert_eq!(missing.owner_user_id.as_deref(), Some("buyer"));
        assert!(missing.owner_name.is_none());
        assert!(missing.scope.is_empty());
        item.owner_user_id = None;
        let unassigned = SalesOrderCurrentWork::from_task(&item, &names, &HashMap::new()).unwrap();
        assert!(unassigned.owner_user_id.is_none());
        assert!(unassigned.owner_name.is_none());
    }

    #[test]
    fn delivery_acceptance_and_invoice_keep_independent_responsibility() {
        let mut item = task("handler", "line");
        let cases = [
            (
                WorkItemType::FulfillmentOperation,
                "delivery",
                "warehouse_outbound_handler",
                "fulfillment",
                "仓储",
            ),
            (WorkItemType::FulfillmentOperation, "delivery", "purchase_order_owner", "fulfillment", "采购"),
            (WorkItemType::CustomerAcceptanceRegistration, "sales_order", "sales", "acceptance", "销售"),
            (WorkItemType::SalesInvoiceExecution, "sales_invoice_request", "finance", "invoice", "财务"),
        ];
        for (kind, object, role, section, label) in cases {
            item.work_item_type = kind;
            item.business_object_type = object.into();
            item.owner_role = role.into();
            let row = SalesOrderCurrentWork::from_task(&item, &HashMap::new(), &HashMap::new()).unwrap();
            assert_eq!(row.section, section);
            assert_eq!(row.role_label, label);
        }
        item.business_object_type = "unrelated".into();
        assert!(SalesOrderCurrentWork::from_task(&item, &HashMap::new(), &HashMap::new()).is_none());
    }
}
