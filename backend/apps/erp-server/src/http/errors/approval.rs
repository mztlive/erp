use erp_workflow::ErrorCode;

/// 将审批稳定码转换为各 HTTP 入口共用的公开说明。
///
/// # 参数
/// `code` 为工作流领域声明的稳定码。
/// # 返回
/// 不含内部数据、可指导下一步操作的说明。
/// # 错误
/// 无。
pub(crate) fn approval_message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::ApprovalPolicyNotRegistered => {
            "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员"
        },
        ErrorCode::ApprovalProcessNotConfigured => {
            "该单据类型尚未配置可用的审批流程，请联系管理员发布流程后重试"
        },
        ErrorCode::ApprovalDraftSourceNotAvailable => {
            "当前没有可复制的已发布审批流程，请先发布流程后再创建草稿"
        },
        ErrorCode::ApprovalDefinitionNotDraft => "只能修改草稿流程，请复制为新草稿后再修改",
        ErrorCode::ApprovalDefinitionVersionConflict => "审批流程已被他人更新，请刷新后重试",
        ErrorCode::ApprovalDefinitionInvalid => "审批流程内容不符合要求，请修改后重试",
        ErrorCode::ApprovalDefinitionBindingCorrupted => "单据审批关系异常，请联系支持人员处理",
        ErrorCode::ApprovalAlreadyStarted => "该版本已有未完成的审批，请先查看当前审批进度",
        ErrorCode::ApprovalTaskNotOpen => "审批任务已完成或关闭，请刷新后查看当前状态",
        ErrorCode::ApprovalTaskNotAssignedToActor => {
            "当前账号没有执行此操作的权限，请联系管理员或有权限的同事"
        },
        ErrorCode::ApprovalTaskVersionConflict => "审批任务状态已变化，请刷新后重试",
        ErrorCode::ApprovalInstanceVersionConflict => "审批进度已变化，请刷新后重试",
        ErrorCode::ApprovalExecutionVersionConflict => "当前审批步骤已变化，请刷新后重试",
        ErrorCode::ApprovalSubjectVersionConflict => "单据内容已更新，请刷新后重试",
        ErrorCode::ApprovalRejectReasonRequired => "请填写驳回原因后再提交",
        ErrorCode::ApprovalInstanceBlocked => "当前审批已暂停，请先处理暂停原因",
        ErrorCode::ApprovalResumeNotAllowedForBlocker => {
            "当前暂停原因不允许恢复原审批人，请改用其他可用处理方式"
        },
        ErrorCode::ApprovalCurrentApproverNotRecovered => "原审批人仍不具备审批资格，请先恢复全部资格后重试",
        ErrorCode::ApprovalBlockedCancelNotAllowed => "当前暂停原因不允许取消审批，请恢复原审批人后继续审批",
        ErrorCode::ApprovalGenericWorkItemMutationForbidden => "请在审批任务页面处理该任务",
        ErrorCode::ApprovalIdempotencyPayloadConflict => "该任务号已用于其他请求，请关闭弹窗后重新发起操作",
    }
}
