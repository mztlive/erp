"use client"

import {
    useMutation,
    useQueries,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query"
import type { QueryClient } from "@tanstack/react-query"

import { restoreReceiptSession } from "../api/receipt-draft"
import { writeReceiptSession } from "../lib/receipt-session-storage"

import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { hasPermission } from "@/lib/permissions"
import { fetchAllocationSource } from "../api/allocation-source"
import { approvalKeys } from "@/features/approval-workflow/queries"
import {
    createAllocationSession,
    fetchCustomerAccountsDetail,
    fetchCustomerAccountsList,
    postAllocation,
    resolvePostUnknown,
    reverseFact,
    saveAllocationDraft,
    submitCustomerRefund,
    submitReceiptReversal,
} from "@/features/customer-receivables/api"
import { CUSTOMER_RECEIPT_DOCUMENT_TYPE } from "@/features/customer-receivables/lib/customer-receipt-approval"
import { workItemKeys } from "@/features/work-items/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { CUSTOMER_REFUND_DOCUMENT_TYPE } from "@/features/customer-receivables/lib/customer-refund-approval"
import { RECEIPT_REVERSAL_DOCUMENT_TYPE } from "@/features/customer-receivables/lib/receipt-reversal-approval"
import type {
    AllocationSessionView,
    AllocationTarget,
    CustomerAccountsDetailKind,
    CustomerAccountsQuery,
    PostAllocationInput,
    PostAllocationResult,
} from "@/features/customer-receivables/types"

export const customerReceivableKeys = {
    all: ["customer-receivables"] as const,
    allocationSource: (target: AllocationTarget) =>
        [
            ...customerReceivableKeys.all,
            "allocation-source",
            target.salesOrderId,
            target.salesOrderRevisionId ?? "current",
        ] as const,
    list: (query: CustomerAccountsQuery) =>
        [...customerReceivableKeys.all, "list", query] as const,
    detail: (kind: string, id: string) =>
        [...customerReceivableKeys.all, "detail", kind, id] as const,
    session: (draftSessionId: string, ownerId?: string) =>
        [
            ...customerReceivableKeys.all,
            "session",
            ownerId ?? "",
            draftSessionId,
        ] as const,
}

/** 首次提交与结果核对成功使用相同的草稿状态和相关业务缓存更新。 */
async function refreshPostedAllocation(
    queryClient: QueryClient,
    result: Extract<PostAllocationResult, { status: "succeeded" }>,
    input: PostAllocationInput,
    ownerId?: string,
) {
    await queryClient.invalidateQueries({
        queryKey: queryKeyRoots.invoiceRequests,
    })
    queryClient.setQueryData<AllocationSessionView>(
        customerReceivableKeys.session(input.draftSessionId, ownerId),
        (current) =>
            current
                ? {
                      ...current,
                      status: "posted",
                      existingFactId: result.factId,
                      existingFactNo: result.factNo,
                      approval: result.approval,
                      serverStatus: result.subjectStatus,
                      recoveryPending: undefined,
                  }
                : current,
    )
    await queryClient.invalidateQueries({
        queryKey: customerReceivableKeys.all,
        predicate: (query) => query.queryKey[1] !== "session",
    })
    if (result.mode === "invoice") {
        await queryClient.invalidateQueries({
            queryKey: queryKeyRoots.salesOrders,
        })
    }
    await queryClient.invalidateQueries({
        queryKey: workItemKeys.all,
    })
    await queryClient.invalidateQueries({
        queryKey: queryKeyRoots.workspaceHome,
    })
    if (result.mode === "receipt") {
        await queryClient.invalidateQueries({
            queryKey: approvalKeys.document(
                CUSTOMER_RECEIPT_DOCUMENT_TYPE,
                result.factId,
            ),
        })
    }
}

export function useCustomerAccountsListQuery(query: CustomerAccountsQuery) {
    return useQuery({
        queryKey: customerReceivableKeys.list(query),
        queryFn: () => fetchCustomerAccountsList(query),
    })
}

export function useCustomerAccountsDetailQuery(
    kind: CustomerAccountsDetailKind | null,
    id: string | null,
    refreshOnMount = false,
) {
    return useQuery({
        queryKey: customerReceivableKeys.detail(kind ?? "", id ?? ""),
        staleTime: refreshOnMount ? 0 : undefined,
        queryFn: () => fetchCustomerAccountsDetail(kind!, id!),
        enabled: Boolean(kind && id),
    })
}

export function useAllocationSessionQuery(draftSessionId: string | null) {
    const queryClient = useQueryClient()
    const profile = useAccountProfileQuery()
    const ownerId = profile.data?.userid
    const query = useQuery({
        queryKey: customerReceivableKeys.session(draftSessionId ?? "", ownerId),
        queryFn: () => {
            const cached = queryClient.getQueryData<AllocationSessionView>(
                customerReceivableKeys.session(draftSessionId!, ownerId),
            )
            if (cached?.mode === "invoice") return cached
            return restoreReceiptSession(ownerId!, draftSessionId!)
        },
        enabled: Boolean(ownerId && draftSessionId),
        staleTime: Number.POSITIVE_INFINITY,
        retry: false,
    })
    return {
        ...query,
        error: profile.error ?? query.error,
        isPending: !profile.isError && query.isPending,
    }
}

export function useCreateAllocationSessionMutation() {
    const profile = useAccountProfileQuery()
    const ownerId = profile.data?.userid
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async (
            input: Parameters<typeof createAllocationSession>[0],
        ) => {
            if (!ownerId) throw new Error("当前账号尚未加载，请稍后重试。")
            const session = {
                ...(await createAllocationSession(input)),
                ownerId,
            }
            if (session.mode === "receipt") writeReceiptSession(session, null)
            return session
        },
        onSuccess: (session) => {
            queryClient.setQueryData(
                customerReceivableKeys.session(session.draftSessionId, ownerId),
                session,
            )
        },
    })
}

export function useSaveAllocationDraftMutation() {
    const profile = useAccountProfileQuery()
    const ownerId = profile.data?.userid
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: Parameters<typeof saveAllocationDraft>[1]) =>
            saveAllocationDraft(
                queryClient.getQueryData<AllocationSessionView>(
                    customerReceivableKeys.session(
                        input.draftSessionId,
                        ownerId,
                    ),
                ) ?? null,
                input,
            ),
        onSuccess: async (session) => {
            queryClient.setQueryData(
                customerReceivableKeys.session(session.draftSessionId, ownerId),
                session,
            )
            if (session.mode === "receipt") {
                await queryClient.invalidateQueries({
                    queryKey: customerReceivableKeys.all,
                    predicate: (query) => query.queryKey[1] !== "session",
                })
            }
        },
    })
}

/**
 * 提交核销。回款成功后刷新审批单据缓存；发票为 NO_APPROVAL，不触达审批键。
 */
export function usePostAllocationMutation() {
    const profile = useAccountProfileQuery()
    const ownerId = profile.data?.userid
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: PostAllocationInput) =>
            postAllocation(
                input,
                queryClient.getQueryData<AllocationSessionView>(
                    customerReceivableKeys.session(
                        input.draftSessionId,
                        ownerId,
                    ),
                ) ?? null,
            ),
        onSuccess: async (result, input) => {
            if (result.status === "succeeded") {
                await refreshPostedAllocation(
                    queryClient,
                    result,
                    input,
                    ownerId,
                )
            }
        },
    })
}

export function useResolvePostUnknownMutation() {
    const profile = useAccountProfileQuery()
    const ownerId = profile.data?.userid
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: PostAllocationInput) =>
            resolvePostUnknown(
                input,
                queryClient.getQueryData<AllocationSessionView>(
                    customerReceivableKeys.session(
                        input.draftSessionId,
                        ownerId,
                    ),
                ) ?? null,
            ),
        onSuccess: async (result, input) => {
            if (result?.status === "succeeded") {
                await refreshPostedAllocation(
                    queryClient,
                    result,
                    input,
                    ownerId,
                )
            }
        },
    })
}

/**
 * 提交客户退款审批。成功后刷新退款详情与审批单据缓存。
 */
export function useSubmitCustomerRefundMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitCustomerRefund,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: customerReceivableKeys.all,
                })
                await queryClient.invalidateQueries({
                    queryKey: approvalKeys.document(
                        CUSTOMER_REFUND_DOCUMENT_TYPE,
                        result.refund.refundId,
                    ),
                })
            }
        },
    })
}

/**
 * 提交回款冲正审批。成功后刷新冲正详情与审批单据缓存。
 */
export function useSubmitReceiptReversalMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitReceiptReversal,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: customerReceivableKeys.all,
                })
                await queryClient.invalidateQueries({
                    queryKey: approvalKeys.document(
                        RECEIPT_REVERSAL_DOCUMENT_TYPE,
                        result.reversal.reversalId,
                    ),
                })
            }
        },
    })
}

/**
 * 冲正/红票一次提交；退款或冲正成功后还会刷新对应审批单据缓存。
 */
export function useReverseFactMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: reverseFact,
        onSuccess: async (result, variables) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: customerReceivableKeys.all,
                })
                if (!result.approval) return
                const documentType =
                    variables.kind === "refund"
                        ? CUSTOMER_REFUND_DOCUMENT_TYPE
                        : variables.kind === "receipt_reverse"
                          ? RECEIPT_REVERSAL_DOCUMENT_TYPE
                          : undefined
                if (!documentType) return
                await queryClient.invalidateQueries({
                    queryKey: approvalKeys.document(
                        documentType,
                        result.reverseFactId,
                    ),
                })
            }
        },
    })
}

/** 同一销售单的多笔应收共享来源查询；来源失败不阻断回款表单。 */
export function useAllocationSources(pool: readonly AllocationTarget[]) {
    const profile = useAccountProfileQuery()
    const canRead =
        !profile.isError &&
        hasPermission(profile.data?.permissions, "sales_order:detail")
    const targets = [
        ...new Map(
            pool.map((target) => [allocationSourceKey(target), target]),
        ).values(),
    ]
    const queries = useQueries({
        queries: targets.map((target) => ({
            queryKey: customerReceivableKeys.allocationSource(target),
            queryFn: () =>
                fetchAllocationSource(
                    target.salesOrderId,
                    target.salesOrderRevisionId,
                ),
            enabled: canRead,
            retry: false,
        })),
    })
    return {
        canRead,
        sources: new Map(
            targets.map((target, index) => [
                allocationSourceKey(target),
                queries[index],
            ]),
        ),
    }
}

export function allocationSourceKey(target: AllocationTarget): string {
    return JSON.stringify([
        target.salesOrderId,
        target.salesOrderRevisionId ?? "current",
    ])
}
