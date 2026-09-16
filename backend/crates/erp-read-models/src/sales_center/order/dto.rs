//! Cross-domain sales order list, procurement and approval response views.
use erp_core::money::{Amount, Quantity, Rate};
use erp_sales::dto::sales_order::{RevisionView, SalesOrderLineView, SubmissionView, WorkingCopyView};
use erp_sales::entity::sales_order::{BusinessType, CommercialStatus, OriginSystem};
use erp_workflow::service::work_item::{ProcessingBlockerView, ProcessingState, WorkItemPartyView};
use serde::Serialize;
/// 销售单列表行视图（契约形状；金额以字符串序列化）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderView {
    /// 实体主键。
    pub id: String,
    /// 销售单号。
    pub order_no: String,
    /// 业务性质。
    pub business_type: BusinessType,
    /// 最初创建入口。
    pub origin_system: OriginSystem,
    /// 客户稳定身份。
    pub customer_id: String,
    /// 合同稳定身份。
    pub contract_id: Option<String>,
    /// 商业主状态。
    pub commercial_status: CommercialStatus,
    /// 审核轨状态。
    pub review_status: erp_sales::entity::sales_order::ReviewStatus,
    /// 履约进度。
    pub fulfillment_progress: erp_sales::entity::sales_order::FulfillmentProgress,
    /// 回款进度。
    pub collection_progress: erp_sales::entity::sales_order::CollectionProgress,
    /// 开票进度。
    pub invoice_progress: erp_sales::entity::sales_order::InvoiceProgress,
    /// 关闭状态。
    pub close_status: erp_sales::entity::sales_order::CloseStatus,
    /// 生效时间（秒级时间戳）。
    pub effective_at: Option<u64>,
    /// ERP 关闭时间（秒级时间戳）。
    pub closed_at: Option<u64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 更新时间（秒级时间戳）。
    pub updated_at: u64,
    /// 负责销售账号。
    pub owner_user_id: String,
    /// 负责销售姓名；账号已删除时为空。
    pub owner_user_name: Option<String>,
    /// 当前阶段摘要（服务端权威计算，替代前端字符串拼接）。
    pub stage: SalesOrderStageSummary,
}

/// 销售单当前阶段摘要（列表行与详情共用）。
///
/// `code` 与 erp-client `filter-orders.ts::SALES_ORDER_STATUS_OPTIONS` 的 9 个
/// 筛选码一一对应，前端筛选逻辑改为直接比较该码，不再维护一份中文 label 匹配。
/// 责任人/时限来自批量查询命中的待办（详情单条查、列表整页批量查，见
/// `SalesOrderReadService::sales_order_list`），审核轨未在途或无命中待办时为 `None`。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderStageSummary {
    /// 阶段码：`draft`/`awaiting_confirm`/`awaiting_sales`/`awaiting_sales_lead`/
    /// `awaiting_ops`/`fulfilling`/`effective`/`closed`/`voided`。
    pub code: &'static str,
    /// 中文展示文案。
    pub label: &'static str,
    /// 展示语气：`success`/`warning`/`info`/`void`/`neutral`。
    pub tone: &'static str,
    /// 当前责任岗位（来自命中的待办 `owner_role`）；无待办时为 `None`。
    pub owner_role: Option<String>,
    /// 当前责任人账号（来自命中的待办 `owner_user_id`）；团队待处理时为 `None`。
    pub owner_user_id: Option<String>,
    /// 当前责任人姓名。
    pub owner_user_name: Option<String>,
    /// 预计完成时限（秒级时间戳）；当前待办派发时尚未赋值时为 `None`。
    pub due_at: Option<u64>,
}

/// 结案资格判定（服务端权威；移植自 erp-client `close-eligibility.ts`）。
///
/// 规则（W05 §5.3/§12）：非卡券以客户验收完成判定交付；卡券以履约期限到期判定
/// （不因已消费完提前算完成）；结案门槛为交付完成且回款收齐，开票进度不参与。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CloseEligibilityView {
    /// 交付是否已完成。
    pub fulfillment_complete: bool,
    /// 应收是否已结清。
    pub receivable_settled: bool,
    /// 开票是否已完成（不影响 `eligible_to_close`）。
    pub invoice_complete: bool,
    /// 是否具备结案资格。
    pub eligible_to_close: bool,
    /// 阻塞原因（`eligible_to_close=false` 时非空）。
    pub blockers: Vec<String>,
    /// 面向用户的说明文案。
    pub note: String,
}

/// 卡券销售审批工作面允许执行的固定动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CardSalesApprovalAllowedAction {
    /// 提交通过决定。
    Approve,
    /// 提交驳回决定。
    Reject,
    /// 提交终止决定；该动作不会形成驳回记录。
    Terminate,
    /// 由原提交人撤回尚未形成不可逆决定的审批。
    Cancel,
}

/// 销售单详情内嵌的唯一活动卡券审批投影。
///
/// 首步骤责任解析失败时实例和步骤仍会以 `BLOCKED` 落库，但可以没有待办；
/// 因此任务字段均允许为空，页面不得为此伪造任务身份或版本。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActiveCardSalesApprovalView {
    /// 审批实例。
    pub approval_instance_id: String,
    /// 审批实例乐观锁版本。
    pub instance_version: u64,
    /// 当前审批步骤。
    pub approval_step_instance_id: String,
    /// 当前步骤乐观锁版本。
    pub step_version: u64,
    /// 当前开放待办；解析阻塞且尚未形成待办时为空。
    pub work_item_id: Option<String>,
    /// 当前待办乐观锁版本。
    pub task_version: Option<u64>,
    /// 当前固定任务类型。
    pub work_item_type: Option<erp_workflow::entity::work_item::WorkItemType>,
    /// 当前任务状态。
    pub work_item_status: Option<erp_workflow::entity::work_item::WorkItemStatus>,
    /// 当前处理状态。
    pub processing_state: ProcessingState,
    /// 权限安全的阻塞摘要。
    pub processing_blocker: Option<ProcessingBlockerView>,
    /// 当前个人责任人安全摘要。
    pub owner_user: Option<WorkItemPartyView>,
    /// 冻结业务版本。
    pub subject_version: String,
    /// 被审批的销售提交。
    pub sales_order_submission_id: String,
    /// 提交序号。
    pub submission_no: u32,
    /// 权限安全的冻结提交摘要。
    pub frozen_submission_summary: String,
    /// 当前步骤要求的销售审核轨状态。
    pub expected_review_status: String,
    /// 服务端当前允许动作。
    pub allowed_actions: Vec<CardSalesApprovalAllowedAction>,
    /// 当前不可执行原因。
    pub action_blockers: Vec<ProcessingBlockerView>,
}

/// 销售当前版本的统一供给覆盖进度。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesProcurementCoverageView {
    /// 当前销售版本商品/服务目标总数量。
    pub total_quantity: Quantity,
    /// 有效采购与现有库存直接分配的覆盖数量。
    pub covered_quantity: Quantity,
    /// 当前仍待分配供给的数量。
    pub remaining_quantity: Quantity,
    /// 覆盖进度，范围 `0..=1`。
    pub progress: Rate,
}

/// 当前账号从销售单继续执行供给分配的访问投影。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct PurchaseCreationAccessView {
    /// 当前账号是否拥有该销售单的开放供给分配任务。
    pub allowed: bool,
    /// 当前账号拥有的开放任务数量。
    pub task_count: usize,
    /// 不允许时的稳定业务说明。
    pub blocker: Option<String>,
}

/// 销售单详情视图（订单 + 稳定明细 + 草稿 + 提交历史 + 版本历史）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderDetailView {
    /// 实体主键。
    pub id: String,
    /// 销售单号。
    pub order_no: String,
    /// 业务性质。
    pub business_type: BusinessType,
    /// 最初创建入口。
    pub origin_system: OriginSystem,
    /// 客户稳定身份。
    pub customer_id: String,
    /// 合同稳定身份。
    pub contract_id: Option<String>,
    /// 结算主体。
    pub settlement_party_id: String,
    /// 商业主状态。
    pub commercial_status: CommercialStatus,
    /// 审核轨状态。
    pub review_status: erp_sales::entity::sales_order::ReviewStatus,
    /// 履约/回款/开票/关闭进度。
    pub fulfillment_progress: erp_sales::entity::sales_order::FulfillmentProgress,
    /// 回款进度。
    pub collection_progress: erp_sales::entity::sales_order::CollectionProgress,
    /// 开票进度。
    pub invoice_progress: erp_sales::entity::sales_order::InvoiceProgress,
    /// 关闭状态。
    pub close_status: erp_sales::entity::sales_order::CloseStatus,
    /// 当前生效版本。
    pub current_revision_id: Option<String>,
    /// 生效时间（秒级时间戳）。
    pub effective_at: Option<u64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 当前销售单负责人账号。
    pub owner_user_id: String,
    /// 当前销售单负责人姓名。
    pub owner_user_name: Option<String>,
    /// 当前关联的未作废采购单数量。
    pub purchase_order_count: u64,
    /// 当前销售版本供给目标、覆盖、剩余与进度。
    pub purchase_coverage: SalesProcurementCoverageView,
    /// 当前账号是否可以从该销售单继续执行供给分配。
    pub purchase_creation_access: PurchaseCreationAccessView,
    /// 应收子账已核销含税合计。
    pub settled_total: Amount,
    /// 应收子账净已开票含税合计。
    pub invoiced_total: Amount,
    /// 稳定明细行。
    pub lines: Vec<SalesOrderLineView>,
    /// 有效草稿（首次提交目的）。
    pub working_copy: Option<WorkingCopyView>,
    /// 提交历史（新提交在前）。
    pub submissions: Vec<SubmissionView>,
    /// 版本历史（新版本在前）。
    pub revisions: Vec<RevisionView>,
    /// 当前阶段详情（含责任人与时限）。
    pub stage: SalesOrderStageSummary,
    /// 结案资格判定。
    pub close_eligibility: CloseEligibilityView,
    /// 是否可以发起销售变更单。
    pub can_start_sales_change_order: bool,
    /// 不可发起销售变更单时的原因；可发起时为 `None`。
    pub change_order_blocker: Option<String>,
    /// 唯一活动卡券审批；非卡券、无活动实例或数据不完整时为空。
    pub active_card_sales_approval: Option<ActiveCardSalesApprovalView>,
    /// 统一只读审批结构（`SalesOrder` 与 `VoucherSalesOrder`）。
    pub approval: Option<DocumentApprovalView>,
}

/// 单据详情返回的统一只读审批结构。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalView {
    /// `PROCESS_REQUIRED` 或 `NO_APPROVAL`。
    pub requirement: String,
    /// 创建时冻结的定义摘要；未绑定为空。
    pub definition: Option<DocumentApprovalDefinitionView>,
    /// 已启动后的实例摘要；未提交为空。
    pub instance: Option<DocumentApprovalInstanceView>,
    /// 有界最近历史。
    pub recent_history: Vec<DocumentApprovalHistoryItemView>,
    /// 完整历史分页游标。
    pub history_page: DocumentApprovalHistoryPageView,
    /// 服务端允许的动作；不含选择定义或审批人。
    pub allowed_actions: Vec<String>,
}

/// 绑定定义只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalDefinitionView {
    /// 定义主键。
    pub id: String,
    /// 定义名称。
    pub name: String,
    /// 定义业务版本。
    pub version: u32,
    /// 节点摘要。单据详情不展开审批人。
    pub nodes: Vec<DocumentApprovalNodeView>,
}

impl DocumentApprovalDefinitionView {
    /// 构造绑定定义只读摘要。
    ///
    /// # 参数
    /// * `id` - 定义主键
    /// * `name` - 定义名称
    ///
    /// # 返回
    /// 返回版本为零、节点为空的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn new(id: String, name: String) -> Self {
        Self { id, name, version: 0, nodes: Vec::new() }
    }

    /// 设置定义业务版本。
    ///
    /// # 参数
    /// * `version` - 定义业务版本
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    /// 设置节点摘要。
    ///
    /// # 参数
    /// * `nodes` - 节点摘要
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_nodes(mut self, nodes: Vec<DocumentApprovalNodeView>) -> Self {
        self.nodes = nodes;
        self
    }
}

/// 定义节点只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalNodeView {
    /// 节点键。
    pub key: String,
    /// 节点名称。
    pub name: String,
}

/// 运行实例只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalInstanceView {
    /// 实例主键。
    pub id: String,
    /// 实例状态。
    pub status: String,
    /// 当前轮次。
    pub current_round_no: u32,
    /// 当前节点键。
    pub current_node: Option<String>,
    /// 当前节点名称。
    pub current_node_name: Option<String>,
    /// 当前审批人。
    pub current_assignee: Option<String>,
    /// 当前审批人显示名。
    pub current_assignee_name: Option<String>,
    /// 最近驳回原因。
    pub latest_rejection: Option<String>,
    /// 绑定定义业务版本。
    pub process_version: Option<u32>,
    /// 受阻代码；非 BLOCKED 为空。
    pub blocker_code: Option<String>,
}

impl DocumentApprovalInstanceView {
    /// 构造运行实例只读摘要。
    ///
    /// # 参数
    /// * `id` - 实例主键
    /// * `status` - 实例状态
    ///
    /// # 返回
    /// 返回首轮、可选字段全空的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn new(id: String, status: String) -> Self {
        Self {
            id,
            status,
            current_round_no: 1,
            current_node: None,
            current_node_name: None,
            current_assignee: None,
            current_assignee_name: None,
            latest_rejection: None,
            process_version: None,
            blocker_code: None,
        }
    }

    /// 设置当前轮次。
    ///
    /// # 参数
    /// * `round_no` - 当前轮次
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_current_round_no(mut self, round_no: u32) -> Self {
        self.current_round_no = round_no;
        self
    }

    /// 设置当前节点。
    ///
    /// # 参数
    /// * `node` - 当前节点键
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_current_node(mut self, node: Option<String>) -> Self {
        self.current_node = node;
        self
    }

    /// 设置当前节点名称。
    ///
    /// # 参数
    /// * `node_name` - 当前节点名称
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_current_node_name(mut self, node_name: Option<String>) -> Self {
        self.current_node_name = node_name;
        self
    }

    /// 设置当前审批人。
    ///
    /// # 参数
    /// * `assignee` - 当前审批人
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_current_assignee(mut self, assignee: Option<String>) -> Self {
        self.current_assignee = assignee;
        self
    }

    /// 设置当前审批人显示名。
    ///
    /// # 参数
    /// * `assignee_name` - 当前审批人显示名
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_current_assignee_name(mut self, assignee_name: Option<String>) -> Self {
        self.current_assignee_name = assignee_name;
        self
    }

    /// 设置最近驳回原因。
    ///
    /// # 参数
    /// * `reason` - 最近驳回原因
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_latest_rejection(mut self, reason: Option<String>) -> Self {
        self.latest_rejection = reason;
        self
    }

    /// 设置绑定定义业务版本。
    ///
    /// # 参数
    /// * `version` - 绑定定义业务版本
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_process_version(mut self, version: Option<u32>) -> Self {
        self.process_version = version;
        self
    }

    /// 设置受阻代码。
    ///
    /// # 参数
    /// * `code` - 受阻代码
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_blocker_code(mut self, code: Option<String>) -> Self {
        self.blocker_code = code;
        self
    }
}

/// 有界历史项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryItemView {
    /// 执行主键。
    pub execution_id: String,
    /// 轮次。
    pub round_no: u32,
    /// 实例内执行序号。
    pub execution_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 节点名称。
    pub node_name: String,
    /// 结束结果。
    pub result: String,
    /// 审批人显示名。
    pub assignee_name: Option<String>,
    /// 决定人。
    pub decided_by: Option<String>,
    /// 决定原因。
    pub decision_reason: Option<String>,
    /// 决定时间（unix 秒）。
    pub decided_at: Option<i64>,
}

impl DocumentApprovalHistoryItemView {
    /// 构造有界历史项。
    ///
    /// # 参数
    /// * `execution_id` - 执行主键
    /// * `node_key` - 节点键
    /// * `node_name` - 节点名称
    /// * `result` - 结束结果
    ///
    /// # 返回
    /// 返回首轮、可选字段全空的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn new(execution_id: String, node_key: String, node_name: String, result: String) -> Self {
        Self {
            execution_id,
            round_no: 1,
            execution_no: 1,
            node_key,
            node_name,
            result,
            assignee_name: None,
            decided_by: None,
            decision_reason: None,
            decided_at: None,
        }
    }

    /// 设置轮次。
    ///
    /// # 参数
    /// * `round_no` - 轮次
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_round_no(mut self, round_no: u32) -> Self {
        self.round_no = round_no;
        self
    }

    /// 设置实例内执行序号。
    ///
    /// # 参数
    /// * `execution_no` - 实例内执行序号
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_execution_no(mut self, execution_no: u32) -> Self {
        self.execution_no = execution_no;
        self
    }

    /// 设置审批人显示名。
    ///
    /// # 参数
    /// * `name` - 审批人显示名
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_assignee_name(mut self, name: Option<String>) -> Self {
        self.assignee_name = name;
        self
    }

    /// 设置决定人。
    ///
    /// # 参数
    /// * `decided_by` - 决定人
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_decided_by(mut self, decided_by: Option<String>) -> Self {
        self.decided_by = decided_by;
        self
    }

    /// 设置决定原因。
    ///
    /// # 参数
    /// * `reason` - 决定原因
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_decision_reason(mut self, reason: Option<String>) -> Self {
        self.decision_reason = reason;
        self
    }

    /// 设置决定时间。
    ///
    /// # 参数
    /// * `decided_at` - 决定时间（unix 秒）
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_decided_at(mut self, decided_at: Option<i64>) -> Self {
        self.decided_at = decided_at;
        self
    }
}

/// 完整历史分页。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryPageView {
    /// 下一页游标。
    pub next_cursor: Option<String>,
    /// 是否还有更多。
    pub has_more: bool,
}

#[cfg(test)]
mod wire_tests {
    use super::*;

    #[test]
    fn history_page_default_is_closed_cursor() {
        let page = DocumentApprovalHistoryPageView::default();
        assert!(page.next_cursor.is_none());
        assert!(!page.has_more);
    }

    #[test]
    fn definition_view_builder_keeps_identity_and_version() {
        let view =
            DocumentApprovalDefinitionView::new("def-1".to_string(), "销售审批".to_string()).with_version(2);
        assert_eq!(view.id, "def-1");
        assert_eq!(view.name, "销售审批");
        assert_eq!(view.version, 2);
        assert!(view.nodes.is_empty());
    }

    #[test]
    fn instance_and_history_builders_preserve_mandatory_fields() {
        let instance = DocumentApprovalInstanceView::new("inst-1".to_string(), "RUNNING".to_string())
            .with_current_round_no(3);
        assert_eq!(instance.id, "inst-1");
        assert_eq!(instance.status, "RUNNING");
        assert_eq!(instance.current_round_no, 3);
        let access = PurchaseCreationAccessView::default();
        assert!(!access.allowed);
        assert_eq!(access.task_count, 0);
        assert!(access.blocker.is_none());
        let item = DocumentApprovalHistoryItemView::new(
            "exec-1".to_string(),
            "node-1".to_string(),
            "节点一".to_string(),
            "APPROVED".to_string(),
        )
        .with_round_no(2);
        assert_eq!(item.execution_id, "exec-1");
        assert_eq!(item.round_no, 2);
    }
}
