//! 编译期审批定义、处理器、解析器与强类型领域动作注册表。

use entities::{
    approval::{ApprovalAssignmentMode, ApprovalDecision},
    work_item::WorkItemType,
};

/// 卡券销售审批的稳定定义编码。
pub const CARD_SALES_APPROVAL: &str = "CARD_SALES_APPROVAL";
/// 当前已发布的卡券销售审批定义版本。
pub const CARD_SALES_APPROVAL_VERSION: u32 = 1;
/// 销售领导审批步骤编码。
pub const SALES_MANAGER_APPROVAL: &str = "SALES_MANAGER_APPROVAL";
/// 运营审批步骤编码。
pub const OPERATIONS_APPROVAL: &str = "OPERATIONS_APPROVAL";
/// 销售领导唯一处理人解析器。
pub const SALES_MANAGER_RESOLVER: &str = "card_sales_unique_sales_manager";
/// 卡券运营责任池解析器。
pub const OPERATIONS_POOL_RESOLVER: &str = "card_sales_operations_pool";
/// 销售领导工作面处理器。
pub const SALES_MANAGER_HANDLER: &str = "card_sales_manager_approval";
/// 运营工作面处理器。
pub const OPERATIONS_HANDLER: &str = "card_sales_operations_approval";

const ALL_DECISIONS: &[ApprovalDecision] = &[
    ApprovalDecision::Approve,
    ApprovalDecision::RejectToApplicant,
    ApprovalDecision::TerminateApproval,
];

/// 编译期定义的一条人工审批步骤。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisteredApprovalStep {
    /// 稳定步骤编码。
    pub step_key: &'static str,
    /// 从一开始的严格串行序号。
    pub sequence_no: u32,
    /// 固定待办类型。
    pub work_item_type: WorkItemType,
    /// 固定工作面处理器。
    pub handler_key: &'static str,
    /// 固定责任模式。
    pub assignment_mode: ApprovalAssignmentMode,
    /// 固定服务端解析器。
    pub resolver_key: &'static str,
    /// 本步骤允许形成的决定。
    pub allowed_decisions: &'static [ApprovalDecision],
}

/// 编译期发布的一套审批定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisteredApprovalDefinition {
    /// 稳定定义编码。
    pub definition_key: &'static str,
    /// 显式业务版本。
    pub version: u32,
    /// 管理与审计名称。
    pub name: &'static str,
    /// 严格串行步骤。
    pub steps: &'static [RegisteredApprovalStep],
}

/// 审批运行时可调用的强类型领域动作。
///
/// 每个变体都由代码注册的定义、步骤和决定唯一确定；客户端不能提交该枚举，
/// 运行时也不会根据任意字符串动态分派。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalBusinessAction {
    /// 冻结卡券销售提交并进入审批。
    SubmitCardSalesApproval,
    /// 记录销售领导通过决定。
    RecordSalesManagerApproval,
    /// 记录销售领导驳回并退回申请人。
    RejectCardSalesBySalesManager,
    /// 记录销售领导终止审批。
    TerminateCardSalesBySalesManager,
    /// 记录运营通过并原子形成生效、应收和执行投影事实。
    ApproveAndActivateCardSales,
    /// 记录运营驳回并退回申请人。
    RejectCardSalesByOperations,
    /// 记录运营终止审批。
    TerminateCardSalesByOperations,
    /// 撤回仍可取消的卡券销售审批。
    CancelCardSalesApproval,
    /// 恢复阻塞前只读重验卡券销售对象与冻结提交版本。
    ValidateCardSalesApprovalRecovery,
}

const CARD_SALES_STEPS: &[RegisteredApprovalStep] = &[
    RegisteredApprovalStep {
        step_key: SALES_MANAGER_APPROVAL,
        sequence_no: 1,
        work_item_type: WorkItemType::CardSalesManagerApproval,
        handler_key: SALES_MANAGER_HANDLER,
        assignment_mode: ApprovalAssignmentMode::Direct,
        resolver_key: SALES_MANAGER_RESOLVER,
        allowed_decisions: ALL_DECISIONS,
    },
    RegisteredApprovalStep {
        step_key: OPERATIONS_APPROVAL,
        sequence_no: 2,
        work_item_type: WorkItemType::CardSalesOperationApproval,
        handler_key: OPERATIONS_HANDLER,
        assignment_mode: ApprovalAssignmentMode::Pool,
        resolver_key: OPERATIONS_POOL_RESOLVER,
        allowed_decisions: ALL_DECISIONS,
    },
];

const CARD_SALES_DEFINITION: RegisteredApprovalDefinition = RegisteredApprovalDefinition {
    definition_key: CARD_SALES_APPROVAL,
    version: CARD_SALES_APPROVAL_VERSION,
    name: "卡券销售审批",
    steps: CARD_SALES_STEPS,
};

/// 返回当前部署必须确定性写入的全部审批定义。
///
/// # 返回
/// 返回按定义编码、版本固定排序的静态定义切片。
pub fn registered_definitions() -> &'static [RegisteredApprovalDefinition] {
    &[CARD_SALES_DEFINITION]
}

/// 按稳定编码解析当前发布定义。
///
/// # 返回
/// 已注册时返回定义；未知编码返回 `None`，调用方必须失败关闭。
pub fn definition(definition_key: &str) -> Option<&'static RegisteredApprovalDefinition> {
    registered_definitions()
        .iter()
        .find(|definition| definition.definition_key == definition_key)
}

/// 校验处理器、解析器和决定均来自编译期注册表。
///
/// # 返回
/// 完整匹配已注册步骤时返回 `true`。
pub fn step_is_registered(step: &RegisteredApprovalStep) -> bool {
    let handler_and_resolver_are_registered = matches!(
        (
            step.step_key,
            step.work_item_type,
            step.handler_key,
            step.assignment_mode,
            step.resolver_key,
        ),
        (
            SALES_MANAGER_APPROVAL,
            WorkItemType::CardSalesManagerApproval,
            SALES_MANAGER_HANDLER,
            ApprovalAssignmentMode::Direct,
            SALES_MANAGER_RESOLVER
        ) | (
            OPERATIONS_APPROVAL,
            WorkItemType::CardSalesOperationApproval,
            OPERATIONS_HANDLER,
            ApprovalAssignmentMode::Pool,
            OPERATIONS_POOL_RESOLVER
        )
    );
    handler_and_resolver_are_registered
        && step.allowed_decisions == ALL_DECISIONS
        && step
            .allowed_decisions
            .iter()
            .all(|decision| decision_action(CARD_SALES_APPROVAL, step.step_key, *decision).is_some())
}

/// 将冻结步骤与正式决定映射为唯一强类型领域动作。
///
/// # 返回
/// 注册组合返回动作；未知步骤或不允许的决定返回 `None`，运行时必须失败关闭。
pub fn decision_action(
    definition_key: &str,
    step_key: &str,
    decision: ApprovalDecision,
) -> Option<ApprovalBusinessAction> {
    if definition_key != CARD_SALES_APPROVAL {
        return None;
    }
    match (step_key, decision) {
        (SALES_MANAGER_APPROVAL, ApprovalDecision::Approve) => {
            Some(ApprovalBusinessAction::RecordSalesManagerApproval)
        }
        (SALES_MANAGER_APPROVAL, ApprovalDecision::RejectToApplicant) => {
            Some(ApprovalBusinessAction::RejectCardSalesBySalesManager)
        }
        (SALES_MANAGER_APPROVAL, ApprovalDecision::TerminateApproval) => {
            Some(ApprovalBusinessAction::TerminateCardSalesBySalesManager)
        }
        (OPERATIONS_APPROVAL, ApprovalDecision::Approve) => {
            Some(ApprovalBusinessAction::ApproveAndActivateCardSales)
        }
        (OPERATIONS_APPROVAL, ApprovalDecision::RejectToApplicant) => {
            Some(ApprovalBusinessAction::RejectCardSalesByOperations)
        }
        (OPERATIONS_APPROVAL, ApprovalDecision::TerminateApproval) => {
            Some(ApprovalBusinessAction::TerminateCardSalesByOperations)
        }
        _ => None,
    }
}

/// 将启动请求映射为唯一强类型业务提交动作。
pub fn start_action(definition_key: &str) -> Option<ApprovalBusinessAction> {
    (definition_key == CARD_SALES_APPROVAL).then_some(ApprovalBusinessAction::SubmitCardSalesApproval)
}

/// 将取消请求映射为唯一强类型业务撤回动作。
pub fn cancel_action(definition_key: &str) -> Option<ApprovalBusinessAction> {
    (definition_key == CARD_SALES_APPROVAL).then_some(ApprovalBusinessAction::CancelCardSalesApproval)
}

/// 将阻塞恢复映射为唯一强类型业务只读重验动作。
pub fn recovery_validation_action(definition_key: &str) -> Option<ApprovalBusinessAction> {
    (definition_key == CARD_SALES_APPROVAL)
        .then_some(ApprovalBusinessAction::ValidateCardSalesApprovalRecovery)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_sales_definition_is_strictly_serial_and_fully_registered() {
        let definition = definition(CARD_SALES_APPROVAL).unwrap();
        assert_eq!(definition.version, 1);
        assert_eq!(definition.steps.len(), 2);
        for (index, step) in definition.steps.iter().enumerate() {
            assert_eq!(step.sequence_no, u32::try_from(index + 1).unwrap());
            assert!(step_is_registered(step));
        }
        assert_eq!(
            definition.steps[0].assignment_mode,
            ApprovalAssignmentMode::Direct
        );
        assert_eq!(definition.steps[1].assignment_mode, ApprovalAssignmentMode::Pool);
    }

    #[test]
    fn unknown_definition_and_dynamic_decision_combinations_fail_closed() {
        assert!(definition("UNKNOWN").is_none());
        assert!(start_action("UNKNOWN").is_none());
        assert!(cancel_action("UNKNOWN").is_none());
        assert!(recovery_validation_action("UNKNOWN").is_none());
        assert!(decision_action("UNKNOWN", SALES_MANAGER_APPROVAL, ApprovalDecision::Approve).is_none());
        assert!(decision_action(CARD_SALES_APPROVAL, "UNKNOWN", ApprovalDecision::Approve).is_none());
    }

    #[test]
    fn mismatched_handler_assignment_and_decision_registry_fail_closed() {
        let mut step = CARD_SALES_STEPS[0];
        step.assignment_mode = ApprovalAssignmentMode::Pool;
        assert!(!step_is_registered(&step));
        step = CARD_SALES_STEPS[0];
        step.step_key = "UNKNOWN";
        assert!(!step_is_registered(&step));
        step = CARD_SALES_STEPS[0];
        step.allowed_decisions = &[ApprovalDecision::Approve];
        assert!(!step_is_registered(&step));
    }
}
