import type { WorkspaceFilters } from "@/lib/workspace-search"
import { apiGet, apiPost, getErrorMessage } from "@/lib/api"
import { isApiError } from "@/lib/api/errors"

import {
    mapCommandViewDto,
    mapApprovalRouteDto,
    mapHistoryItemDto,
    mapInstanceListItemDto,
    mapRecoveryOptionsDto,
    mapUpgradeBindingResultViewDto,
    type ApprovalCommandView,
    type ApprovalCommandViewDto,
    type ApprovalHistoryItem,
    type ApprovalHistoryPageDto,
    type ApprovalRoute,
    type ApprovalInstanceListPage,
    type ApprovalInstanceListPageDto,
    type ApprovalInstanceListItemDto,
    type ApprovalInstanceListView,
    type CancelBlockedRequest,
    type RecoveryOptions,
    type RecoveryOptionsDto,
    type ResumeApproverRequest,
    type SubmitDecisionRequest,
    type UpgradeBindingRequest,
    type UpgradeBindingResultView,
    type UpgradeBindingResultViewDto,
} from "./types"

export type ApprovalInstanceListParams = Readonly<{
    filters?: WorkspaceFilters
    page?: number
    view: ApprovalInstanceListView
    documentType?: string
    status?: "RUNNING" | "APPROVED" | "CANCELLED" | "BLOCKED"
    query?: string
    cursor?: string
    limit?: number
}>

export type ApprovalHistoryParams = Readonly<{
    instanceId: string
    cursor?: string
    limit?: number
    order?: "asc" | "desc"
}>

export {
    cancelDocumentApproval,
    type CancelDocumentApprovalParams,
} from "./api/document-cancel"

export type UpgradeDocumentBindingParams = Readonly<{
    documentType: string
    documentId: string
    request: UpgradeBindingRequest
}>

/**
 * 从统一 API 异常中识别 409 责任或版本冲突。
 *
 * @param error 未知异常
 * @returns 是否为 409
 */
export const isApprovalConflict = (error: unknown): boolean =>
    isApiError(error) && error.status === 409

/**
 * 读取冲突后的用户提示。409 不得自动重放决定。
 *
 * 后端按《审批流程错误目录》返回具体冲突原因（哪个版本变了、下一步做什么），
 * 这里只透传；仅在后端无可读文案时使用统一兜底。
 *
 * @param error 未知异常
 * @returns 用户可见下一步
 */
export const approvalConflictMessage = (error: unknown): string =>
    getErrorMessage(error, "提交失败，请刷新后重试。")

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
            page: params.page,
            filters: params.filters
                ? JSON.stringify(params.filters)
                : undefined,
            document_type: params.documentType,
            status: params.status,
            q: params.query,
            cursor: params.cursor,
            limit: params.limit ?? 20,
        },
    )
    return {
        items: page.items.map(mapInstanceListItemDto),
        filterOptions: page.filter_options,
        documentTypeCounts: page.document_type_counts ?? undefined,
        nextCursor: page.next_cursor ?? undefined,
        total: page.total ?? undefined,
    }
}

/** 按实例 ID 独立读取审批状态，授权由实例接口校验。 */
export const getApprovalInstance = async (instanceId: string) => {
    const dto = await apiGet<
        ApprovalInstanceListItemDto & { rework_task_id?: string | null }
    >(`/admin/approval-instances/${encodeURIComponent(instanceId)}`)
    return {
        ...mapInstanceListItemDto(dto),
        reworkTaskId: dto.rework_task_id ?? undefined,
    }
}

/** 导航提醒只统计有未读进展的审批单。 */
export const getStartedApprovalUnreadCount = async (): Promise<number> => {
    const result = await apiGet<{ unread_count: number }>(
        "/admin/approval-instances/started-unread-count",
    )
    return result.unread_count
}

/** 显式确认已展示版本；后续进展由服务端保留未读。 */
export const markApprovalProgressRead = ({
    instanceId,
    observedVersion,
}: {
    instanceId: string
    observedVersion: string
}): Promise<void> =>
    apiPost(
        `/admin/approval-instances/${encodeURIComponent(instanceId)}/progress-read`,
        {
            observed_instance_version: observedVersion,
        },
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
    route?: ApprovalRoute
    observedInstanceVersion?: string
}> => {
    const page = await apiGet<ApprovalHistoryPageDto>(
        `/admin/approval-instances/${encodeURIComponent(params.instanceId)}/history`,
        {
            cursor: params.cursor,
            limit: params.limit ?? 50,
            order: params.order,
        },
    )
    return {
        items: page.items.map(mapHistoryItemDto),
        nextCursor: page.next_cursor ?? undefined,
        hasMore: page.has_more,
        route: page.route ? mapApprovalRouteDto(page.route) : undefined,
        observedInstanceVersion: page.observed_instance_version ?? undefined,
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
): Promise<UpgradeBindingResultView> =>
    apiPost<UpgradeBindingResultViewDto>(
        `/admin/business-documents/${encodeURIComponent(params.documentType)}/${encodeURIComponent(params.documentId)}/approval-definition/upgrade`,
        params.request,
    ).then(mapUpgradeBindingResultViewDto)

/** 只查询当前账号的组织负责人，服务端同时校验审批资格。 */
export function fetchOwnApprovalLeader(kind: "team" | "department") {
    return apiGet<{ name: string; organization_name: string }>(
        `/account/approval-leader?kind=${kind}`,
    )
}
