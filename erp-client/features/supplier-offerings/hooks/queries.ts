"use client"

import {
    useInfiniteQuery,
    useMutation,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query"

import {
    completeSupplierSupplyExceptionTask,
    createSupplierOffering,
    fetchSupplierSupplyExceptionWorkItem,
    fetchSupplierOfferings,
    fetchSupplierOffering,
    fetchOfferingHistory,
    fetchSupplierOfferingsForSkus,
    reviseSupplierOffering,
    updateSupplierOfferingAvailability,
} from "@/features/supplier-offerings/api/offerings"
import type {
    CompleteSupplierSupplyExceptionTaskInput,
    CreateSupplierOfferingInput,
    ReviseSupplierOfferingInput,
    SupplierOfferingListQuery,
    UpdateOfferingAvailabilityInput,
} from "@/features/supplier-offerings/types"
import { workItemKeys } from "@/features/work-items/queries"

const supplierOfferingKeys = {
    all: ["supplier-offerings"] as const,
    detail: (id: string) =>
        [...supplierOfferingKeys.all, "detail", id] as const,
    history: (id: string) =>
        [...supplierOfferingKeys.all, "history", id] as const,
    list: (query: SupplierOfferingListQuery) =>
        [...supplierOfferingKeys.all, "list", query] as const,
}

export function useSupplierOfferingsQuery(query: SupplierOfferingListQuery) {
    return useQuery({
        queryKey: supplierOfferingKeys.list(query),
        queryFn: () => fetchSupplierOfferings(query),
    })
}

/** 读取并校验 W21 唯一已注册的供应停止任务。 */
export function useSupplierSupplyExceptionWorkItemQuery(workItemId?: string) {
    return useQuery({
        queryKey: [...supplierOfferingKeys.all, "supply-exception", workItemId],
        queryFn: () => fetchSupplierSupplyExceptionWorkItem(workItemId ?? ""),
        enabled: Boolean(workItemId),
    })
}

export function useCompleteSupplierSupplyExceptionTaskMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: CompleteSupplierSupplyExceptionTaskInput) =>
            completeSupplierSupplyExceptionTask(input),
        onSuccess: async () => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: supplierOfferingKeys.all,
                }),
                queryClient.invalidateQueries({ queryKey: workItemKeys.all }),
            ])
        },
    })
}

/** 商品列表当前页 SKU 的供给明细；列表状态与明细弹窗共用同一份查询结果。 */
export function useSupplierOfferingsForSkusQuery(skuIds: readonly string[]) {
    const normalized = [...new Set(skuIds.filter(Boolean))].sort()
    return useQuery({
        queryKey: [...supplierOfferingKeys.all, "sku-details", normalized],
        queryFn: () => fetchSupplierOfferingsForSkus(normalized),
        enabled: normalized.length > 0,
    })
}

function useInvalidateOfferingData() {
    const queryClient = useQueryClient()
    return async () => {
        await Promise.all([
            queryClient.invalidateQueries({
                queryKey: supplierOfferingKeys.all,
            }),
            queryClient.invalidateQueries({ queryKey: ["master-data"] }),
        ])
    }
}

export function useCreateSupplierOfferingMutation() {
    const invalidate = useInvalidateOfferingData()
    return useMutation({
        mutationFn: (input: CreateSupplierOfferingInput) =>
            createSupplierOffering(input),
        meta: { affectsDataScope: true },
        onSuccess: invalidate,
    })
}

export function useReviseSupplierOfferingMutation() {
    const invalidate = useInvalidateOfferingData()
    return useMutation({
        mutationFn: (input: ReviseSupplierOfferingInput) =>
            reviseSupplierOffering(input),
        meta: { affectsDataScope: true },
        onSuccess: invalidate,
    })
}

export function useUpdateOfferingAvailabilityMutation() {
    const invalidate = useInvalidateOfferingData()
    return useMutation({
        mutationFn: (input: UpdateOfferingAvailabilityInput) =>
            updateSupplierOfferingAvailability(input),
        meta: { affectsDataScope: true },
        onSuccess: invalidate,
    })
}

/** 供给资料使用独立查询，修改供给后与列表一起失效。 */
export function useSupplierOfferingQuery(id: string) {
    return useQuery({
        queryKey: supplierOfferingKeys.detail(id),
        queryFn: () => fetchSupplierOffering(id),
        enabled: Boolean(id),
    })
}

/** 历史页打开时才取数，每页最多 20 条。 */
export function useOfferingHistoryQuery(id: string) {
    return useInfiniteQuery({
        queryKey: supplierOfferingKeys.history(id),
        queryFn: ({ pageParam }) => fetchOfferingHistory(id, pageParam),
        initialPageParam: undefined as number | undefined,
        getNextPageParam: (page) => page.next_before_revision_no ?? undefined,
    })
}
