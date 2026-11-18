use erp_workflow::ErrorCode;

// 将审批稳定码转换为各 HTTP 入口共用的公开说明。
// `code` 为工作流领域声明的稳定码。
// 不含内部数据、可指导下一步操作的说明。
pub(crate) fn approval_message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::ResourceAuthorizationNotReady => "资料授权正在处理，请稍后刷新",
        ErrorCode::ApprovalPolicyNotRegistered => {
            "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员"
        },
        ErrorCode::ApprovalProcessNotConfigured => {
            "该单据类型尚未配置可用的审批流程，请联系管理员发布流程后重试"
        },
        ErrorCode::ApprovalDraftSourceNotAvailable => {
            "找不到可复制的已发布流程，请新建审批流程，或选择其他已发布流程复制"
        },
        ErrorCode::ApprovalDefinitionNotDraft => {
            "此流程已离开草稿状态，请刷新列表；如需调整已发布流程，请先复制为草稿"
        },
        ErrorCode::ApprovalDefinitionVersionConflict => "审批流程已被他人更新，请刷新后重试",
        ErrorCode::ApprovalDefinitionInvalid => "审批流程设置有误，请检查审批步骤和人员安排后再提交",
        ErrorCode::ApprovalDefinitionBindingCorrupted => "暂时无法打开这张单据的审批流程，请联系支持人员",
        ErrorCode::ApprovalAlreadyStarted => "这张单据已有一笔审批在进行，请先查看审批进度",
        ErrorCode::ApprovalTaskNotOpen => "审批任务已完成或关闭，请刷新后查看当前状态",
        ErrorCode::ApprovalTaskNotAssignedToActor => "这项审批任务由其他人员处理，请联系当前审批人或管理员",
        ErrorCode::ApprovalTaskVersionConflict => "审批任务状态已变化，请刷新后重试",
        ErrorCode::ApprovalInstanceVersionConflict => "审批进度已变化，请刷新后重试",
        ErrorCode::ApprovalExecutionVersionConflict => "当前审批步骤已变化，请刷新后重试",
        ErrorCode::ApprovalSubjectVersionConflict => "单据内容已更新，请刷新后重试",
        ErrorCode::ApprovalRejectReasonRequired => "请填写驳回原因后再提交",
        ErrorCode::ApprovalInstanceBlocked => "当前审批已暂停，请先处理暂停原因",
        ErrorCode::ApprovalResumeNotAllowedForBlocker => {
            "当前无法由原审批人继续处理，请查看审批详情中可用的处理方式"
        },
        ErrorCode::ApprovalCurrentApproverNotRecovered => {
            "原审批人目前仍无法办理这项审批，请检查账号状态和审批权限后再试"
        },
        ErrorCode::ApprovalBlockedCancelNotAllowed => {
            "这笔暂停的审批暂时无法取消，请先恢复原审批人的办理资格"
        },
        ErrorCode::ApprovalGenericWorkItemMutationForbidden => "请在审批任务页面处理该任务",
        ErrorCode::ApprovalIdempotencyPayloadConflict => {
            "本次提交与之前的操作内容不一致，请刷新页面确认当前状态，再重新提交"
        },
    }
}
