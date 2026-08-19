"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { approvalKeys } from "@/features/approval-workflow/queries"
import {
    ensurePaymentReversalDraft,
    ensureSupplierPaymentDraft,
    ensureSupplierRefundDraft,
    fetchAllocationSession,
    fetchPayableDetail,
    fetchPaymentReversal,
    fetchSupplierAccounts,
    fetchSupplierPayment,
    fetchSupplierRefund,
    resolveUnknownResult,
    reverseInvoice,
    reversePayment,
    saveAllocationDraft,
    submitInvoice,
    submitPayment,
    submitPaymentReversal,
    submitSupplierRefund,
} from "@/features/supplier-payables/api/requests"
import { PAYMENT_REVERSAL_DOCUMENT_TYPE } from "@/features/supplier-payables/lib/payment-reversal-approval"
import { SUPPLIER_PAYMENT_DOCUMENT_TYPE } from "@/features/supplier-payables/lib/supplier-payment-approval"
import { SUPPLIER_REFUND_DOCUMENT_TYPE } from "@/features/supplier-payables/lib/supplier-refund-approval"
import type {
    AllocationTrack,
    SupplierAccountsQuery,
} from "@/features/supplier-payables/types"
import { purchaseOrderKeys } from "@/features/purchase-orders/queries"
import { fulfillmentKeys } from "@/features/fulfillment-operations/queries"
import { workItemKeys } from "@/features/work-items/queries"

const supplierPayablesKeys = {
    all: ["supplier-payables"] as const,
    list: (query: SupplierAccountsQuery) =>
        [...supplierPayablesKeys.all, "list", query] as const,
    detail: (payableAccountId: string) =>
        [...supplierPayablesKeys.all, "detail", payableAccountId] as const,
    payment: (paymentId: string) =>
        [...supplierPayablesKeys.all, "payment", paymentId] as const,
    refund: (refundId: string) =>
        [...supplierPayablesKeys.all, "refund", refundId] as const,
    reversal: (reversalId: string) =>
        [...supplierPayablesKeys.all, "reversal", reversalId] as const,
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

/**
 * 读取供应商付款详情，含只读审批绑定。
 *
 * @param paymentId 付款主键；空值不发请求。
 */
export function useSupplierPaymentQuery(paymentId: string | null) {
    return useQuery({
        queryKey: supplierPayablesKeys.payment(paymentId ?? ""),
        queryFn: () => fetchSupplierPayment(paymentId!),
        enabled: Boolean(paymentId),
    })
}

/**
 * 读取供应商退款详情，含只读审批绑定。
 *
 * @param refundId 退款主键；空值不发请求。
 */
export function useSupplierRefundQuery(refundId: string | null) {
    return useQuery({
        queryKey: supplierPayablesKeys.refund(refundId ?? ""),
        queryFn: () => fetchSupplierRefund(refundId!),
        enabled: Boolean(refundId),
    })
}

/**
 * 读取付款冲正详情，含只读审批绑定。
 *
 * @param reversalId 冲正主键；空值不发请求。
 */
export function usePaymentReversalQuery(reversalId: string | null) {
    return useQuery({
        queryKey: supplierPayablesKeys.reversal(reversalId ?? ""),
        queryFn: () => fetchPaymentReversal(reversalId!),
        enabled: Boolean(reversalId),
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
    paymentId?: string,
) {
    await queryClient.invalidateQueries({ queryKey: supplierPayablesKeys.all })
    await queryClient.invalidateQueries({ queryKey: purchaseOrderKeys.all })
    await queryClient.invalidateQueries({ queryKey: fulfillmentKeys.all })
    await queryClient.invalidateQueries({ queryKey: workItemKeys.all })
    await queryClient.invalidateQueries({ queryKey: approvalKeys.all })
    if (paymentId) {
        await queryClient.invalidateQueries({
            queryKey: approvalKeys.document(
                SUPPLIER_PAYMENT_DOCUMENT_TYPE,
                paymentId,
            ),
        })
    }
}

export function useSubmitPaymentMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitPayment,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await invalidateFinanceAndSources(
                    queryClient,
                    result.existingDocumentId ?? result.documentNo,
                )
            }
        },
    })
}

/**
 * 提交确认前创建或刷新付款草稿，只读带回服务端绑定。
 */
export function useEnsureSupplierPaymentDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: ensureSupplierPaymentDraft,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: supplierPayablesKeys.all,
                })
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

/**
 * 提交确认前创建供应商退款草稿，并刷新只读审批绑定。
 */
export function useEnsureSupplierRefundDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: ensureSupplierRefundDraft,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: approvalKeys.document(
                        SUPPLIER_REFUND_DOCUMENT_TYPE,
                        result.refund.refundId,
                    ),
                })
                await queryClient.invalidateQueries({
                    queryKey: supplierPayablesKeys.refund(
                        result.refund.refundId,
                    ),
                })
            }
        },
    })
}

/**
 * 提交供应商退款审批。成功后刷新退款详情与审批单据缓存。
 */
export function useSubmitSupplierRefundMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitSupplierRefund,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: supplierPayablesKeys.all,
                })
                await queryClient.invalidateQueries({
                    queryKey: approvalKeys.document(
                        SUPPLIER_REFUND_DOCUMENT_TYPE,
                        result.refund.refundId,
                    ),
                })
            }
        },
    })
}

/**
 * 提交确认前创建付款冲正草稿，并刷新只读审批绑定。
 */
export function useEnsurePaymentReversalDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: ensurePaymentReversalDraft,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: approvalKeys.document(
                        PAYMENT_REVERSAL_DOCUMENT_TYPE,
                        result.reversal.reversalId,
                    ),
                })
                await queryClient.invalidateQueries({
                    queryKey: supplierPayablesKeys.reversal(
                        result.reversal.reversalId,
                    ),
                })
            }
        },
    })
}

/**
 * 提交付款冲正审批。成功后刷新冲正详情与审批单据缓存。
 */
export function useSubmitPaymentReversalMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitPaymentReversal,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: supplierPayablesKeys.all,
                })
                await queryClient.invalidateQueries({
                    queryKey: approvalKeys.document(
                        PAYMENT_REVERSAL_DOCUMENT_TYPE,
                        result.reversal.reversalId,
                    ),
                })
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
