"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    addCollaborationNote,
    completeSupplierOrderTask,
    createSupplierOrderExportJob,
    fetchSupplierOrderDetail,
    fetchFulfillmentHandoverCandidates,
    fetchSupplierOrders,
    handoverFulfillmentOrder,
    querySupplierResult,
    replaySupplierOrder,
    revealSupplierOrderAddress,
    submitAfterSalesAction,
} from "@/features/supplier-orders/api/index"
import type {
    ExportCommand,
    SupplierOrderListQuery,
} from "@/features/supplier-orders/types"

const supplierOrderKeys = {
    all: ["supplier-orders"] as const,
    list: (query: SupplierOrderListQuery) =>
        [...supplierOrderKeys.all, "list", query] as const,
    detail: (orderId: string) =>
        [...supplierOrderKeys.all, "detail", orderId] as const,
}

export function useSupplierOrdersQuery(query: SupplierOrderListQuery) {
    const client = useQueryClient()
    const firstPage = { ...query, page: 1, scopeVersion: undefined }
    const baseline = client.getQueryData<
        Awaited<ReturnType<typeof fetchSupplierOrders>>
    >(supplierOrderKeys.list(firstPage))
    const scopeVersion =
        query.scopeVersion ??
        (query.page > 1 ? baseline?.scopeVersion : undefined)
    const scoped = { ...query, scopeVersion }
    return useQuery({
        queryKey: supplierOrderKeys.list(scoped),
        queryFn: async () => {
            if (query.page === 1 || scopeVersion)
                return fetchSupplierOrders(scoped)
            const first = await client.fetchQuery({
                queryKey: supplierOrderKeys.list(firstPage),
                queryFn: () => fetchSupplierOrders(firstPage),
            })
            return fetchSupplierOrders({
                ...query,
                scopeVersion: first.scopeVersion,
            })
        },
    })
}

export function useSupplierOrderDetailQuery(input: {
    orderId: string
    workItemId?: string
    enabled?: boolean
}) {
    return useQuery({
        queryKey: [
            ...supplierOrderKeys.detail(input.orderId),
            input.workItemId ?? null,
        ],
        queryFn: () =>
            fetchSupplierOrderDetail({
                orderId: input.orderId,
                workItemId: input.workItemId,
            }),
        enabled: input.enabled !== false && Boolean(input.orderId),
    })
}

function useInvalidateOrders() {
    const queryClient = useQueryClient()
    return async () => {
        await queryClient.invalidateQueries({
            queryKey: supplierOrderKeys.all,
        })
    }
}

export function useQueryResultMutation() {
    const invalidate = useInvalidateOrders()
    return useMutation({
        mutationFn: querySupplierResult,
        onSuccess: async (result) => {
            if (result.status === "succeeded" || result.status === "unknown") {
                await invalidate()
            }
        },
    })
}

export function useReplayOrderMutation() {
    const invalidate = useInvalidateOrders()
    return useMutation({
        mutationFn: replaySupplierOrder,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}

export function useCompleteOrderTaskMutation() {
    const invalidate = useInvalidateOrders()
    return useMutation({
        mutationFn: completeSupplierOrderTask,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}

export function useAfterSalesActionMutation() {
    const invalidate = useInvalidateOrders()
    return useMutation({
        mutationFn: submitAfterSalesAction,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}

export function useRevealAddressMutation() {
    const invalidate = useInvalidateOrders()
    return useMutation({
        mutationFn: revealSupplierOrderAddress,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}

export function useAddNoteMutation() {
    const invalidate = useInvalidateOrders()
    return useMutation({
        mutationFn: addCollaborationNote,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}

export function useFulfillmentHandoverCandidatesQuery(orderId: string) {
    return useQuery({
        queryKey: [...supplierOrderKeys.all, "handover-candidates", orderId],
        queryFn: () => fetchFulfillmentHandoverCandidates(orderId),
        enabled: Boolean(orderId),
    })
}

export function useHandoverFulfillmentOrderMutation() {
    const invalidate = useInvalidateOrders()
    return useMutation({
        mutationFn: (input: {
            orderId: string
            targetUserId: string
            targetOrgUnitId?: string
            reason: string
            expectedVersion: number
            idempotencyKey: string
            transferOpenExceptionTasks?: boolean
        }) => handoverFulfillmentOrder(input.orderId, input),
        meta: { affectsDataScope: true },
        onSuccess: async () => {
            await invalidate()
        },
    })
}

export function useSupplierOrderExportMutation() {
    const invalidate = useInvalidateOrders()
    return useMutation({
        mutationFn: (command: ExportCommand) =>
            createSupplierOrderExportJob(command),
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}
