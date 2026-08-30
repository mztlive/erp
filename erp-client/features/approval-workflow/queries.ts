"use client"

import {
    useInfiniteQuery,
    useMutation,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query"

import { workItemKeys } from "@/features/work-items/queries"

import {
    cancelBlockedApproval,
    cancelDocumentApproval,
    getRecoveryOptions,
    listApprovalHistory,
    listApprovalInstances,
    resumeCurrentApprover,
    submitDecision,
    upgradeUnsubmittedBinding,
    type ApprovalHistoryParams,
    type ApprovalInstanceListParams,
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
    instances: (
        view: ApprovalInstanceListParams["view"],
        filters: Omit<ApprovalInstanceListParams, "cursor">,
    ) => [...approvalKeys.all, "instances", view, filters] as const,
    recoveryOptions: (instanceId: string) =>
        [...approvalKeys.all, "recovery-options", instanceId] as const,
}

/**
 * 命令成功后精确失效任务、实例和对应业务单据。
 */
const invalidateApprovalCaches = async (
    queryClient: ReturnType<typeof useQueryClient>,
    input?: { instanceId?: string; documentType?: string; documentId?: string },
) => {
    await Promise.all([
        queryClient.invalidateQueries({ queryKey: workItemKeys.all }),
        queryClient.invalidateQueries({ queryKey: approvalKeys.all }),
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
 * 按固定 view 查询实例摘要。
 */
export const useApprovalInstancesQuery = (
    params: ApprovalInstanceListParams,
    enabled = true,
) =>
    useQuery({
        queryKey: approvalKeys.instances(params.view, {
            view: params.view,
            documentType: params.documentType,
            status: params.status,
            query: params.query,
            limit: params.limit,
        }),
        queryFn: () => listApprovalInstances(params),
        enabled,
        placeholderData: (previous) => previous,
    })

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
export const useSubmitDecisionMutation = () => {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitDecision,
        onSuccess: async (view) => {
            await invalidateApprovalCaches(queryClient, {
                instanceId: view.instanceId,
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
