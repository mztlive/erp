"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { approvalKeys } from "@/features/approval-workflow/queries"
import {
    commitPaymentReversal,
    commitSupplierRefund,
    fetchAllocationSession,
    fetchPayableDetail,
    fetchPaymentReversal,
    fetchSupplierAccounts,
    fetchSupplierPayment,
    fetchSupplierPaymentBankReceiptBlob,
    fetchSupplierRefund,
    revealPaymentRecipient,
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
import { SUPPLIER_REFUND_DOCUMENT_TYPE } from "@/features/supplier-payables/lib/supplier-refund-approval"
import type {
    AllocationTrack,
    SupplierAccountsQuery,
} from "@/features/supplier-payables/types"
import { purchaseOrderKeys } from "@/features/purchase-orders/queries"
import { fulfillmentKeys } from "@/features/fulfillment-operations/queries"
import { workItemKeys } from "@/features/work-items/queries"
import { workspaceHomeKeys } from "@/features/workspace/hooks/queries"

export const supplierPayablesKeys = {
    all: ["supplier-payables"] as const,
    list: (query: SupplierAccountsQuery) =>
        [...supplierPayablesKeys.all, "list", query] as const,
    detail: (payableAccountId: string) =>
        [...supplierPayablesKeys.all, "detail", payableAccountId] as const,
    payment: (paymentId: string) =>
        [...supplierPayablesKeys.all, "payment", paymentId] as const,
    paymentReceipt: (paymentId: string) =>
        [...supplierPayablesKeys.payment(paymentId), "bank-receipt"] as const,
    refund: (refundId: string) =>
        [...supplierPayablesKeys.all, "refund", refundId] as const,
    reversal: (reversalId: string) =>
        [...supplierPayablesKeys.all, "reversal", reversalId] as const,
    session: (params: {
        track: AllocationTrack
        supplierId: string
        draftSessionId?: string
        purchaseOrderId?: string
        returnTo?: string
        fromWorkspace?: string
        existingInvoiceId?: string
        preselectPayableAccountId?: string
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
 * 读取供应商付款详情。
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

/** 读取付款单归属的银行回单图片；空付款 ID 或无回单时不发请求。 */
export function useSupplierPaymentBankReceiptQuery(
    paymentId: string | null,
    enabled = true,
) {
    return useQuery({
        queryKey: supplierPayablesKeys.paymentReceipt(paymentId ?? ""),
        queryFn: () => fetchSupplierPaymentBankReceiptBlob(paymentId!),
        enabled: Boolean(paymentId) && enabled,
        staleTime: 5 * 60 * 1000,
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
                      returnTo: params.returnTo,
                      fromWorkspace: params.fromWorkspace,
                      existingInvoiceId: params.existingInvoiceId,
                      preselectPayableAccountId:
                          params.preselectPayableAccountId,
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
    await queryClient.invalidateQueries({ queryKey: workItemKeys.all })
    await queryClient.invalidateQueries({ queryKey: workspaceHomeKeys.all })
    await queryClient.invalidateQueries({ queryKey: approvalKeys.all })
}

/** 受控揭示付款任务收款账号；结果只留在组件短时内存，不进入 Query 缓存。 */
export function useRevealPaymentRecipientMutation() {
    return useMutation({
        mutationFn: revealPaymentRecipient,
        gcTime: 0,
    })
}

export function useSubmitPaymentMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitPayment,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: supplierPayablesKeys.all,
                    refetchType: "none",
                }),
                queryClient.invalidateQueries({
                    queryKey: purchaseOrderKeys.all,
                    refetchType: "none",
                }),
                queryClient.invalidateQueries({
                    queryKey: fulfillmentKeys.all,
                    refetchType: "none",
                }),
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                    refetchType: "none",
                }),
                queryClient.invalidateQueries({
                    queryKey: workspaceHomeKeys.all,
                    refetchType: "none",
                }),
            ])
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
 * 一次创建供应商退款并启动审批。
 */
export function useCommitSupplierRefundMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: commitSupplierRefund,
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
 * 一次创建付款冲正并启动审批。
 */
export function useCommitPaymentReversalMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: commitPaymentReversal,
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
