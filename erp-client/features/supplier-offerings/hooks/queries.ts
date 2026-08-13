"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    createSupplierOffering,
    fetchSupplierOfferings,
    fetchSupplierOfferingsForSkus,
    reviseSupplierOffering,
    updateSupplierOfferingAvailability,
} from "@/features/supplier-offerings/api"
import type {
    CreateSupplierOfferingInput,
    ReviseSupplierOfferingInput,
    SupplierOfferingListQuery,
    UpdateOfferingAvailabilityInput,
} from "@/features/supplier-offerings/types"

const supplierOfferingKeys = {
    all: ["supplier-offerings"] as const,
    list: (query: SupplierOfferingListQuery) =>
        [...supplierOfferingKeys.all, "list", query] as const,
}

export function useSupplierOfferingsQuery(query: SupplierOfferingListQuery) {
    return useQuery({
        queryKey: supplierOfferingKeys.list(query),
        queryFn: () => fetchSupplierOfferings(query),
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
        onSuccess: invalidate,
    })
}

export function useReviseSupplierOfferingMutation() {
    const invalidate = useInvalidateOfferingData()
    return useMutation({
        mutationFn: (input: ReviseSupplierOfferingInput) =>
            reviseSupplierOffering(input),
        onSuccess: invalidate,
    })
}

export function useUpdateOfferingAvailabilityMutation() {
    const invalidate = useInvalidateOfferingData()
    return useMutation({
        mutationFn: (input: UpdateOfferingAvailabilityInput) =>
            updateSupplierOfferingAvailability(input),
        onSuccess: invalidate,
    })
}
