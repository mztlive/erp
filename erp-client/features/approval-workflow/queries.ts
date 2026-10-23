"use client"

import {
    useInfiniteQuery,
    useMutation,
    useQuery,
    useQueryClient,
    type Query,
} from "@tanstack/react-query"

import { workItemKeys } from "@/features/work-items/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"

import {
    cancelBlockedApproval,
    cancelDocumentApproval,
    getRecoveryOptions,
    listApprovalHistory,
    resumeCurrentApprover,
    submitDecision,
    upgradeUnsubmittedBinding,
    type ApprovalHistoryParams,
    type CancelDocumentApprovalParams,
    type UpgradeDocumentBindingParams,
} from "./api"
import type {
    ApprovalCommandView,
    CancelBlockedRequest,
    ResumeApproverRequest,
} from "./types"

export const approvalKeys = {
    all: ["approval"] as const,
    instance: (instanceId: string) =>
        [...approvalKeys.all, "instance", instanceId] as const,
    document: (documentType: string, documentId: string) =>
        [...approvalKeys.all, "document", documentType, documentId] as const,
    history: (instanceId: string) =>
        [...approvalKeys.all, "history", instanceId] as const,
    recoveryOptions: (instanceId: string) =>
        [...approvalKeys.all, "recovery-options", instanceId] as const,
}

/** 已完成任务的工作台详情和付款材料不再自动读取，避免旧任务权限变化打断跳转。 */
const isCompletedApprovalTask = (query: Query, workItemId?: string) =>
    Boolean(workItemId) &&
    ((query.queryKey[0] === "purchase-payment-requests" &&
        query.queryKey[1] === "detail" &&
        query.queryKey[3] === workItemId) ||
        (query.queryKey[0] === queryKeyRoots.workspaceHome[0] &&
            query.queryKey[1] === "task-detail" &&
            query.queryKey[2] === workItemId))

/**
 * 命令成功后精确失效任务、实例和对应业务单据。
 */
const invalidateApprovalCaches = async (
    queryClient: ReturnType<typeof useQueryClient>,
    input?: {
        instanceId?: string
        documentType?: string
        documentId?: string
        completedWorkItemId?: string
    },
) => {
    await Promise.all([
        queryClient.invalidateQueries({ queryKey: workItemKeys.all }),
        queryClient.invalidateQueries({ queryKey: approvalKeys.all }),
        queryClient.invalidateQueries({
            queryKey: queryKeyRoots.invoiceRequests,
        }),
        queryClient.invalidateQueries({
            queryKey: queryKeyRoots.workspaceHome,
            predicate: (query) =>
                !isCompletedApprovalTask(query, input?.completedWorkItemId),
        }),
        ...[
            "sales-orders",
            "sales-change-orders",
            "purchase-orders",
            "purchase-change-orders",
            "purchase-payment-requests",
            "customer-receivables",
            "supplier-payables",
            "stock-adjustments",
        ].map((root) =>
            queryClient.invalidateQueries({
                queryKey: [root],
                predicate: (query) =>
                    !isCompletedApprovalTask(query, input?.completedWorkItemId),
            }),
        ),
        input?.instanceId
            ? queryClient.invalidateQueries({
                  queryKey: approvalKeys.instance(input.instanceId),
              })
            : Promise.resolve(),
        input?.documentType && input.documentId
            ? queryClient.invalidateQueries({
                  queryKey: approvalKeys.document(
                      input.documentType,
                      input.documentId,
                  ),
              })
            : Promise.resolve(),
    ])
}

/**
 * 游标分页读取执行历史。按 round_no 分组、execution_no 排序由调用方完成。
 */
export const useApprovalHistoryInfiniteQuery = (
    params: Omit<ApprovalHistoryParams, "cursor">,
    enabled = true,
) =>
    useInfiniteQuery({
        queryKey: approvalKeys.history(params.instanceId),
        queryFn: ({ pageParam }) =>
            listApprovalHistory({
                ...params,
                cursor: pageParam,
            }),
        initialPageParam: undefined as string | undefined,
        getNextPageParam: (lastPage) => lastPage.nextCursor,
        enabled: enabled && params.instanceId.trim().length > 0,
    })

/**
 * 查询当前 blocker 的恢复选项。无实例时不请求。
 */
export const useRecoveryOptionsQuery = (
    instanceId: string | undefined,
    enabled = true,
) =>
    useQuery({
        queryKey: approvalKeys.recoveryOptions(instanceId ?? ""),
        queryFn: () => getRecoveryOptions(instanceId!),
        enabled: enabled && Boolean(instanceId?.trim()),
    })

/**
 * 提交通过或驳回。成功后刷新任务、实例和单据缓存。
 */
export const useSubmitDecisionMutation = (
    onApplied?: (view: ApprovalCommandView) => void,
) => {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitDecision,
        onSuccess: async (view, command) => {
            const completedTask = {
                predicate: (query: Query) =>
                    isCompletedApprovalTask(query, command.work_item_id),
            }
            // 取消在途材料读取，并仅标记过期；不能再用已完成任务触发重查。
            await queryClient.cancelQueries(completedTask)
            await queryClient.invalidateQueries({
                ...completedTask,
                refetchType: "none",
            })
            // 刷新可能撤销旧任务读取资格并卸载作业面，先交付完成通知。
            onApplied?.(view)
            await invalidateApprovalCaches(queryClient, {
                instanceId: view.instanceId,
                completedWorkItemId: command.work_item_id,
            })
        },
    })
}

/**
 * 恢复当前审批人。
 */
export const useResumeApproverMutation = (instanceId: string) => {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (request: ResumeApproverRequest) =>
            resumeCurrentApprover(instanceId, request),
        onSuccess: async () => {
            await invalidateApprovalCaches(queryClient, { instanceId })
        },
    })
}

/**
 * 取消非人员一致性受阻审批。
 */
export const useCancelBlockedMutation = (instanceId: string) => {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (request: CancelBlockedRequest) =>
            cancelBlockedApproval(instanceId, request),
        onSuccess: async () => {
            await invalidateApprovalCaches(queryClient, { instanceId })
        },
    })
}

/**
 * 升级未提交单据绑定。
 */
export const useUpgradeBindingMutation = () => {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: upgradeUnsubmittedBinding,
        onSuccess: async (_view, variables: UpgradeDocumentBindingParams) => {
            await invalidateApprovalCaches(queryClient, {
                documentType: variables.documentType,
                documentId: variables.documentId,
            })
        },
    })
}

/**
 * 撤回审批。只调用业务单据资源接口。
 */
export const useCancelApprovalMutation = () => {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: cancelDocumentApproval,
        onSuccess: async (
            view: ApprovalCommandView,
            variables: CancelDocumentApprovalParams,
        ) => {
            await invalidateApprovalCaches(queryClient, {
                instanceId: view.instanceId,
                documentType: variables.documentType,
                documentId: variables.documentId,
            })
        },
    })
}
