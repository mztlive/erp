"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    addCollaborationNote,
    createSupplierOrderExportJob,
    deferSupplierOrderTask,
    fetchSupplierOrderDetail,
    fetchSupplierOrders,
    querySupplierResult,
    replaySupplierOrder,
    revealSupplierOrderAddress,
    submitAfterSalesAction,
} from "@/features/supplier-orders/api"
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
    return useQuery({
        queryKey: supplierOrderKeys.list(query),
        queryFn: () => fetchSupplierOrders(query),
    })
}

export function useSupplierOrderDetailQuery(input: {
    orderId: string
    enabled?: boolean
}) {
    return useQuery({
        queryKey: supplierOrderKeys.detail(input.orderId),
        queryFn: () =>
            fetchSupplierOrderDetail({
                orderId: input.orderId,
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

export function useDeferOrderTaskMutation() {
    const invalidate = useInvalidateOrders()
    return useMutation({
        mutationFn: deferSupplierOrderTask,
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
