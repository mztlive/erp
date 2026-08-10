"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    fetchAllocationSession,
    fetchPayableDetail,
    fetchSupplierAccounts,
    resolveUnknownResult,
    reverseInvoice,
    reversePayment,
    saveAllocationDraft,
    submitInvoice,
    submitPayment,
} from "@/features/supplier-payables/api"
import type {
    AllocationTrack,
    SupplierAccountsQuery,
} from "@/features/supplier-payables/types"
import { purchaseOrderKeys } from "@/features/purchase-orders/queries"
import { fulfillmentKeys } from "@/features/fulfillment-operations/queries"

const supplierPayablesKeys = {
    all: ["supplier-payables"] as const,
    list: (query: SupplierAccountsQuery) =>
        [...supplierPayablesKeys.all, "list", query] as const,
    detail: (payableAccountId: string) =>
        [...supplierPayablesKeys.all, "detail", payableAccountId] as const,
    session: (params: {
        track: AllocationTrack
        supplierId: string
        draftSessionId?: string
        purchaseOrderId?: string
        existingPaymentId?: string
        existingInvoiceId?: string
    }) => [...supplierPayablesKeys.all, "session", params] as const,
}

export function useSupplierAccountsQuery(query: SupplierAccountsQuery) {
    return useQuery({
        queryKey: supplierPayablesKeys.list(query),
        queryFn: () => fetchSupplierAccounts(query),
    })
}

export function usePayableDetailQuery(payableAccountId: string | null) {
    return useQuery({
        queryKey: supplierPayablesKeys.detail(payableAccountId ?? ""),
        queryFn: () => fetchPayableDetail(payableAccountId!),
        enabled: Boolean(payableAccountId),
    })
}

export function useAllocationSessionQuery(
    params: {
        track: AllocationTrack
        supplierId: string
        draftSessionId?: string
        purchaseOrderId?: string
        returnTo?: string
        fromWorkspace?: string
        existingPaymentId?: string
        existingInvoiceId?: string
        preselectPayableAccountId?: string
    } | null,
) {
    return useQuery({
        queryKey: supplierPayablesKeys.session(
            params
                ? {
                      track: params.track,
                      supplierId: params.supplierId,
                      draftSessionId: params.draftSessionId,
                      purchaseOrderId: params.purchaseOrderId,
                      existingPaymentId: params.existingPaymentId,
                      existingInvoiceId: params.existingInvoiceId,
                  }
                : { track: "payment", supplierId: "" },
        ),
        queryFn: () => fetchAllocationSession(params!),
        enabled: Boolean(params?.supplierId && params.track),
    })
}

async function invalidateFinanceAndSources(
    queryClient: ReturnType<typeof useQueryClient>,
) {
    await queryClient.invalidateQueries({ queryKey: supplierPayablesKeys.all })
    await queryClient.invalidateQueries({ queryKey: purchaseOrderKeys.all })
    await queryClient.invalidateQueries({ queryKey: fulfillmentKeys.all })
}

export function useSubmitPaymentMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitPayment,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await invalidateFinanceAndSources(queryClient)
            }
        },
    })
}

export function useSubmitInvoiceMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitInvoice,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await invalidateFinanceAndSources(queryClient)
            }
        },
    })
}

export function useReversePaymentMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: reversePayment,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await invalidateFinanceAndSources(queryClient)
            }
        },
    })
}

export function useReverseInvoiceMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: reverseInvoice,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await invalidateFinanceAndSources(queryClient)
            }
        },
    })
}

export function useSaveAllocationDraftMutation() {
    return useMutation({
        mutationFn: saveAllocationDraft,
    })
}

export function useResolveUnknownMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: resolveUnknownResult,
        onSuccess: async (result) => {
            if (result?.status === "succeeded") {
                await invalidateFinanceAndSources(queryClient)
            }
        },
    })
}
