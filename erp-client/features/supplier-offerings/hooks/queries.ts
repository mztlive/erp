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
    DeliveryTemplateInput,
    CreateSupplierOfferingInput,
    ReviseSupplierOfferingInput,
    SupplierOfferingListQuery,
    UpdateOfferingAvailabilityInput,
} from "@/features/supplier-offerings/types"
import { workItemKeys } from "@/features/work-items/queries"
import {
    assignDeliveryTemplate,
    createDeliveryTemplate,
    deleteDeliveryTemplate,
    fetchSupplierDeliveryTemplates,
    updateDeliveryTemplate,
} from "@/features/supplier-offerings/api/delivery-templates"

const supplierOfferingKeys = {
    all: ["supplier-offerings"] as const,
    deliveryTemplates: (supplierId: string) =>
        [
            ...supplierOfferingKeys.all,
            "delivery-templates",
            supplierId,
        ] as const,
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

/** 查询供应商全部配送模板。 */
export function useSupplierDeliveryTemplatesQuery(supplierId: string) {
    return useQuery({
        queryKey: supplierOfferingKeys.deliveryTemplates(supplierId),
        queryFn: () => fetchSupplierDeliveryTemplates(supplierId),
        enabled: Boolean(supplierId),
    })
}

/** 新建或保存配送模板；成功后刷新模板与供给列表。 */
export function useSaveDeliveryTemplateMutation(supplierId: string) {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: { id?: string; value: DeliveryTemplateInput }) =>
            input.id
                ? updateDeliveryTemplate(input.id, input.value)
                : createDeliveryTemplate(supplierId, input.value),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: supplierOfferingKeys.all,
            })
        },
    })
}

/** 删除配送模板。 */
export function useDeleteDeliveryTemplateMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: { id: string; expectedVersion: number }) =>
            deleteDeliveryTemplate(input.id, input.expectedVersion),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: supplierOfferingKeys.all,
            })
        },
    })
}

/** 为供给指定配送模板或改回跟随供应商默认。 */
export function useAssignDeliveryTemplateMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: {
            offeringIds: readonly string[]
            templateId: string | null
        }) => assignDeliveryTemplate(input.offeringIds, input.templateId),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: supplierOfferingKeys.all,
            })
        },
    })
}
