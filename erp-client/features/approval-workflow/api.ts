import { apiGet, apiPost, type ApiError } from "@/lib/api"

import {
    mapCommandViewDto,
    mapDocumentApprovalViewDto,
    mapHistoryItemDto,
    mapInstanceListItemDto,
    mapReassigneeCandidateDto,
    mapRecoveryOptionsDto,
    type ApprovalCommandView,
    type ApprovalCommandViewDto,
    type ApprovalHistoryItem,
    type ApprovalHistoryItemDto,
    type ApprovalHistoryPageDto,
    type ApprovalInstanceListItemDto,
    type ApprovalInstanceListPage,
    type ApprovalInstanceListPageDto,
    type ApprovalInstanceListView,
    type CancelApprovalRequest,
    type CancelBlockedRequest,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
    type ReassignApproverRequest,
    type ReassigneeCandidate,
    type ReassigneeCandidateDto,
    type RecoveryOptions,
    type RecoveryOptionsDto,
    type ResumeApproverRequest,
    type SubmitDecisionRequest,
    type UpgradeBindingRequest,
} from "./types"

export type ApprovalInstanceListParams = Readonly<{
    view: ApprovalInstanceListView
    documentType?: string
    status?: "RUNNING" | "APPROVED" | "CANCELLED" | "BLOCKED"
    cursor?: string
    limit?: number
}>

export type ApprovalHistoryParams = Readonly<{
    instanceId: string
    cursor?: string
    limit?: number
}>

export type EligibleReassigneesParams = Readonly<{
    instanceId: string
    search?: string
    cursor?: string
    limit?: number
}>

export type CancelDocumentApprovalParams = Readonly<{
    documentType: string
    documentId: string
    request: CancelApprovalRequest
}>

export type UpgradeDocumentBindingParams = Readonly<{
    documentType: string
    documentId: string
    request: UpgradeBindingRequest
}>

/** 判断未知值是否为可安全读取的对象。 */
const isRecord = (value: unknown): value is Record<string, unknown> =>
    typeof value === "object" && value !== null

/**
 * 从统一 API 异常中识别 409 责任或版本冲突。
 *
 * @param error 未知异常
 * @returns 是否为 409
 */
export const isApprovalConflict = (error: unknown): boolean => {
    if (!isRecord(error)) return false
    return error.status === 409
}

/**
 * 读取冲突后的用户提示。409 不得自动重放决定。
 *
 * @param error 未知异常
 * @returns 用户可见下一步
 */
export const approvalConflictMessage = (error: unknown): string => {
    if (!isApprovalConflict(error)) {
        if (isRecord(error) && typeof error.message === "string") {
            return error.message
        }
        return "提交失败，请刷新后重试"
    }
    const apiError = error as Partial<ApiError>
    if (apiError.code === "APPROVAL_CURRENT_APPROVER_RECOVERED") {
        return "原审批人已恢复有效，请改用恢复当前审批人"
    }
    return "责任或版本已变化，请刷新后重新确认"
}

/**
 * 提交当前开放审批任务的通过或驳回。
 *
 * 请求体只含 `work_item_id`、`APPROVE|REJECT`、原因、任务版本和幂等键。
 */
export const submitDecision = (
    request: SubmitDecisionRequest,
): Promise<ApprovalCommandView> =>
    apiPost<ApprovalCommandViewDto>("/admin/approval-decisions", request).then(
        mapCommandViewDto,
    )

/**
 * 按固定 view 查询审批实例摘要。
 */
export const listApprovalInstances = async (
    params: ApprovalInstanceListParams,
): Promise<ApprovalInstanceListPage> => {
    const page = await apiGet<ApprovalInstanceListPageDto>(
        "/admin/approval-instances",
        {
            view: params.view,
            document_type: params.documentType,
            status: params.status,
            cursor: params.cursor,
            limit: params.limit ?? 20,
        },
    )
    return {
        items: (page.items ?? []).map(mapInstanceListItemDto),
        nextCursor: page.next_cursor ?? undefined,
        total: page.total ?? undefined,
    }
}

/**
 * 查询实例详情。首屏历史由调用方改走 `recent_history` 或独立游标接口。
 */
export const getApprovalInstance = async (
    instanceId: string,
): Promise<ApprovalInstanceListItemDto> =>
    apiGet<ApprovalInstanceListItemDto>(
        `/admin/approval-instances/${encodeURIComponent(instanceId)}`,
    )

/**
 * 按轮次与执行序号读取完整历史。
 */
export const listApprovalHistory = async (
    params: ApprovalHistoryParams,
): Promise<{
    items: readonly ApprovalHistoryItem[]
    nextCursor?: string
    hasMore: boolean
}> => {
    const page = await apiGet<
        ApprovalHistoryPageDto | readonly ApprovalHistoryItemDto[]
    >(
        `/admin/approval-instances/${encodeURIComponent(params.instanceId)}/history`,
        {
            cursor: params.cursor,
            limit: params.limit ?? 50,
        },
    )
    if (Array.isArray(page)) {
        return {
            items: page.map(mapHistoryItemDto),
            hasMore: false,
        }
    }
    const historyPage = page as ApprovalHistoryPageDto
    return {
        items: (historyPage.items ?? []).map(mapHistoryItemDto),
        nextCursor: historyPage.next_cursor ?? undefined,
        hasMore: Boolean(historyPage.has_more),
    }
}

/**
 * 查询当前 blocker 的唯一合法恢复方式。
 */
export const getRecoveryOptions = (
    instanceId: string,
): Promise<RecoveryOptions> =>
    apiGet<RecoveryOptionsDto>(
        `/admin/approval-instances/${encodeURIComponent(instanceId)}/recovery-options`,
    ).then(mapRecoveryOptionsDto)

/**
 * 按当前单据与岗位分离搜索改派候选人。
 */
export const searchEligibleReassignees = async (
    params: EligibleReassigneesParams,
): Promise<readonly ReassigneeCandidate[]> => {
    const items = await apiGet<readonly ReassigneeCandidateDto[]>(
        `/admin/approval-instances/${encodeURIComponent(params.instanceId)}/eligible-reassignees`,
        {
            search: params.search,
            cursor: params.cursor,
            limit: params.limit ?? 20,
        },
    )
    return items.map(mapReassigneeCandidateDto)
}

/**
 * 原审批人重新合格后恢复当前节点，创建新执行和新任务。
 */
export const resumeCurrentApprover = (
    instanceId: string,
    request: ResumeApproverRequest,
): Promise<ApprovalCommandView> =>
    apiPost<ApprovalCommandViewDto>(
        `/admin/approval-instances/${encodeURIComponent(instanceId)}/resume-current-approver`,
        request,
    ).then(mapCommandViewDto)

/**
 * 仅对人员失效 blocker 改派当前审批人。
 */
export const reassignCurrentApprover = (
    instanceId: string,
    request: ReassignApproverRequest,
): Promise<ApprovalCommandView> =>
    apiPost<ApprovalCommandViewDto>(
        `/admin/approval-instances/${encodeURIComponent(instanceId)}/reassign-current-approver`,
        request,
    ).then(mapCommandViewDto)

/**
 * 取消非人员一致性 blocker。不得调用通用 WorkItem close。
 */
export const cancelBlockedApproval = (
    instanceId: string,
    request: CancelBlockedRequest,
): Promise<ApprovalCommandView> =>
    apiPost<ApprovalCommandViewDto>(
        `/admin/approval-instances/${encodeURIComponent(instanceId)}/cancel-blocked`,
        request,
    ).then(mapCommandViewDto)

/**
 * 升级未提交单据绑定到当前发布版本。目标定义不得由客户端提交。
 */
export const upgradeUnsubmittedBinding = (
    params: UpgradeDocumentBindingParams,
): Promise<DocumentApprovalView> =>
    apiPost<DocumentApprovalViewDto>(
        `/admin/business-documents/${encodeURIComponent(params.documentType)}/${encodeURIComponent(params.documentId)}/approval-definition/upgrade`,
        params.request,
    ).then(mapDocumentApprovalViewDto)

/**
 * 撤回运行中或人员失效受阻的审批。只走业务单据资源接口。
 */
export const cancelDocumentApproval = (
    params: CancelDocumentApprovalParams,
): Promise<ApprovalCommandView> =>
    apiPost<ApprovalCommandViewDto>(
        `/admin/business-documents/${encodeURIComponent(params.documentType)}/${encodeURIComponent(params.documentId)}/approval/cancel`,
        params.request,
    ).then(mapCommandViewDto)
