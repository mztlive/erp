"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { approvalKeys } from "@/features/approval-workflow/queries"
import {
    createAllocationSession,
    ensureCustomerReceiptDraft,
    ensureCustomerRefundDraft,
    fetchAllocationSession,
    fetchCustomerAccountsDetail,
    fetchCustomerAccountsList,
    postAllocation,
    resolvePostUnknown,
    reverseFact,
    saveAllocationDraft,
    submitCustomerRefund,
} from "@/features/customer-receivables/api"
import { CUSTOMER_RECEIPT_DOCUMENT_TYPE } from "@/features/customer-receivables/lib/customer-receipt-approval"
import { CUSTOMER_REFUND_DOCUMENT_TYPE } from "@/features/customer-receivables/lib/customer-refund-approval"
import type { CustomerAccountsQuery } from "@/features/customer-receivables/types"

const customerReceivableKeys = {
    all: ["customer-receivables"] as const,
    list: (query: CustomerAccountsQuery) =>
        [...customerReceivableKeys.all, "list", query] as const,
    detail: (kind: string, id: string) =>
        [...customerReceivableKeys.all, "detail", kind, id] as const,
    session: (draftSessionId: string) =>
        [...customerReceivableKeys.all, "session", draftSessionId] as const,
}

export function useCustomerAccountsListQuery(query: CustomerAccountsQuery) {
    return useQuery({
        queryKey: customerReceivableKeys.list(query),
        queryFn: () => fetchCustomerAccountsList(query),
    })
}

export function useCustomerAccountsDetailQuery(
    kind: "receivable" | "receipt" | "invoice" | "refund" | null,
    id: string | null,
) {
    return useQuery({
        queryKey: customerReceivableKeys.detail(kind ?? "", id ?? ""),
        queryFn: () => fetchCustomerAccountsDetail(kind!, id!),
        enabled: Boolean(kind && id),
    })
}

export function useAllocationSessionQuery(draftSessionId: string | null) {
    return useQuery({
        queryKey: customerReceivableKeys.session(draftSessionId ?? ""),
        queryFn: () => fetchAllocationSession(draftSessionId!),
        enabled: Boolean(draftSessionId),
    })
}

export function useCreateAllocationSessionMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: createAllocationSession,
        onSuccess: async (session) => {
            await queryClient.invalidateQueries({
                queryKey: customerReceivableKeys.session(
                    session.draftSessionId,
                ),
            })
        },
    })
}

export function useSaveAllocationDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: saveAllocationDraft,
        onSuccess: async (session) => {
            await queryClient.invalidateQueries({
                queryKey: customerReceivableKeys.session(
                    session.draftSessionId,
                ),
            })
        },
    })
}

/**
 * 提交确认前创建回款草稿，并刷新会话上的只读审批绑定。
 */
export function useEnsureCustomerReceiptDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: ensureCustomerReceiptDraft,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: customerReceivableKeys.session(
                        result.session.draftSessionId,
                    ),
                })
                if (result.session.existingFactId) {
                    await queryClient.invalidateQueries({
                        queryKey: approvalKeys.document(
                            CUSTOMER_RECEIPT_DOCUMENT_TYPE,
                            result.session.existingFactId,
                        ),
                    })
                }
            }
        },
    })
}

/**
 * 提交核销。回款成功后刷新审批单据缓存；发票为 NO_APPROVAL，不触达审批键。
 */
export function usePostAllocationMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: postAllocation,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: customerReceivableKeys.all,
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
        },
    })
}

export function useResolvePostUnknownMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: resolvePostUnknown,
        onSuccess: async (result) => {
            if (result?.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: customerReceivableKeys.all,
                })
            }
        },
    })
}

/**
 * 提交确认前创建客户退款草稿，并刷新只读审批绑定。
 */
export function useEnsureCustomerRefundDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: ensureCustomerRefundDraft,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: approvalKeys.document(
                        CUSTOMER_REFUND_DOCUMENT_TYPE,
                        result.refund.refundId,
                    ),
                })
                await queryClient.invalidateQueries({
                    queryKey: customerReceivableKeys.detail(
                        "refund",
                        result.refund.refundId,
                    ),
                })
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
 * 冲正/红票一次提交；退款成功后还会刷新审批单据缓存。
 */
export function useReverseFactMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: reverseFact,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: customerReceivableKeys.all,
                })
                if (result.approval) {
                    await queryClient.invalidateQueries({
                        queryKey: approvalKeys.document(
                            CUSTOMER_REFUND_DOCUMENT_TYPE,
                            result.reverseFactId,
                        ),
                    })
                }
            }
        },
    })
}
