"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { approvalKeys } from "@/features/approval-workflow/queries"
import { workItemKeys } from "@/features/work-items/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { fetchHasEligibleAcceptance } from "@/features/sales-orders/api/acceptance"
import {
    cancelSalesOrderApproval,
    createSalesOrder,
    createSalesOrderExportJob,
    fetchSalesOrderDetail,
    fetchSalesChangeOrderDetail,
    fetchSalesOrderDraftForResume,
    fetchSalesOrders,
    saveSalesOrderDraft,
    startSalesChangeOrder,
    submitSalesChangeOrder,
    submitSalesChangeReviewDecision,
    submitSalesOrder,
    type SalesOrdersListQuery,
} from "@/features/sales-orders/api/sales-orders"

export const salesOrderKeys = {
    all: queryKeyRoots.salesOrders,
    list: (query: SalesOrdersListQuery) =>
        [...salesOrderKeys.all, "list", query] as const,
    detail: (id: string) => [...salesOrderKeys.all, "detail", id] as const,
    changes: (salesOrderId: string, scopeVersion?: string) =>
        [
            ...salesOrderKeys.detail(salesOrderId),
            "changes",
            { scopeVersion },
        ] as const,
    acceptanceRoot: (id: string) =>
        [...salesOrderKeys.all, "acceptance", id] as const,
    acceptance: (
        id: string,
        filters: {
            workItemId?: string | null
            expectedTaskVersion?: string
        },
    ) => [...salesOrderKeys.acceptanceRoot(id), filters] as const,
    acceptanceEligibility: (id: string) =>
        [...salesOrderKeys.acceptanceRoot(id), "eligibility"] as const,
}

export function useSalesOrdersQuery(
    query: SalesOrdersListQuery,
    enabled = true,
) {
    const client = useQueryClient()
    const firstPage = { ...query, page: 1, scopeVersion: undefined }
    const baseline = client.getQueryData<
        import("@/features/sales-orders/api/contracts").SalesOrderListView
    >(salesOrderKeys.list(firstPage))
    const scopeVersion =
        query.scopeVersion ??
        (query.page > 1 ? baseline?.scopeVersion : undefined)
    const scoped = { ...query, scopeVersion }
    return useQuery({
        queryKey: salesOrderKeys.list(scoped),
        queryFn: async () => {
            if (query.page === 1 || scopeVersion)
                return fetchSalesOrders(scoped)
            // 直接打开后续页时先建立本次查询的第一页授权快照。
            const first = await client.fetchQuery({
                queryKey: salesOrderKeys.list(firstPage),
                queryFn: () => fetchSalesOrders(firstPage),
            })
            return fetchSalesOrders({
                ...query,
                scopeVersion: first.scopeVersion,
            })
        },
        enabled,
    })
}

export function useSalesOrderDetailQuery(
    salesOrderId: string,
    refreshOnMount = false,
) {
    const client = useQueryClient()
    return useQuery({
        queryKey: salesOrderKeys.detail(salesOrderId),
        refetchOnMount: refreshOnMount ? "always" : true,
        queryFn: () => {
            const previous = client.getQueryData<
                | import("@/features/sales-orders/api/contracts").SalesOrderDetailView
                | null
            >(salesOrderKeys.detail(salesOrderId))
            return fetchSalesOrderDetail(
                salesOrderId,
                previous?.changeOrderScopeVersion,
            )
        },
        enabled: Boolean(salesOrderId),
    })
}

/** 按指定变更单读取，历史入口不得回退到当前活动变更单。 */
export function useSalesChangeOrderDetailQuery(
    salesOrderId: string,
    changeOrderId: string,
    nature: Parameters<typeof fetchSalesChangeOrderDetail>[1] | undefined,
) {
    return useQuery({
        queryKey: [
            ...salesOrderKeys.detail(salesOrderId),
            "change",
            changeOrderId,
            nature,
        ],
        queryFn: () => {
            if (!nature) throw new Error("销售单业务性质尚未加载")
            return fetchSalesChangeOrderDetail(
                changeOrderId,
                nature,
                salesOrderId,
            )
        },
        enabled: Boolean(changeOrderId && nature),
    })
}

export function useSalesOrderAcceptanceEligibilityQuery(
    salesOrderId: string,
    enabled: boolean,
) {
    return useQuery({
        queryKey: salesOrderKeys.acceptanceEligibility(salesOrderId),
        queryFn: () => fetchHasEligibleAcceptance(salesOrderId),
        enabled: enabled && Boolean(salesOrderId),
    })
}

export function useCancelSalesOrderApprovalMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: cancelSalesOrderApproval,
        onSuccess: async (_data, variables) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(variables.salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                }),
            ])
        },
    })
}

export function useSalesOrderDraftResumeQuery(salesOrderId: string) {
    return useQuery({
        queryKey: [
            ...salesOrderKeys.detail(salesOrderId),
            "draft-resume",
        ] as const,
        queryFn: () => fetchSalesOrderDraftForResume(salesOrderId),
        enabled: salesOrderId.length > 0,
    })
}

export function useCreateSalesOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: createSalesOrder,
        onSuccess: async (data) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(data.salesOrderId),
                }),
                queryClient.invalidateQueries({ queryKey: ["contracts"] }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
            ])
        },
    })
}

export function useSaveSalesOrderDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: saveSalesOrderDraft,
        onSuccess: async (_data, variables) => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(variables.salesOrderId),
            })
        },
    })
}

export function useSubmitSalesOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitSalesOrder,
        onSuccess: async (data) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(data.salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                }),
            ])
        },
    })
}

export function useStartSalesChangeOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: startSalesChangeOrder,
        onSuccess: async (_data, variables) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(variables.salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
            ])
        },
    })
}

/**
 * 提交销售变更审批。成功后刷新销售单、审批实例与任务。
 */
export function useSubmitSalesChangeOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitSalesChangeOrder,
        onSuccess: async (_data, variables) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(variables.salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                }),
            ])
        },
    })
}

export function useSalesChangeReviewDecisionMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitSalesChangeReviewDecision,
        onSuccess: async () => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: ["work-items"],
                }),
            ])
        },
    })
}

export function useCreateSalesOrderExportJobMutation() {
    return useMutation({
        mutationFn: createSalesOrderExportJob,
    })
}
