import type {
    CandidateFilters,
    CandidateListQuery,
} from "../lib/candidate-list-query"
import type { RequestListQuery } from "../lib/list-query"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    commandRequest,
    commitPayment,
    downloadAttachment,
    fetchCandidates,
    fetchCandidatePage,
    fetchCandidateIds,
    fetchMaterials,
    fetchRequest,
    fetchRequests,
    fetchSubmission,
    revealAccount,
    saveRequest,
    submitRequestBatch,
} from "../api"

export const paymentRequestKeys = {
    all: ["purchase-payment-requests"] as const,
    list: ["purchase-payment-requests", "list"] as const,
    candidates: ["purchase-payment-requests", "candidates"] as const,
    detail: (id: string, workItemId?: string) =>
        ["purchase-payment-requests", "detail", id, workItemId ?? ""] as const,
}
export function usePaymentRequestsQuery(
    query: RequestListQuery,
    enabled = true,
) {
    return useQuery({
        queryKey: [...paymentRequestKeys.list, query],
        queryFn: () => fetchRequests(query),
        enabled,
    })
}
export function usePaymentCandidatesQuery(enabled = true) {
    return useQuery({
        queryKey: paymentRequestKeys.candidates,
        queryFn: fetchCandidates,
        enabled,
    })
}
export function usePaymentCandidatePageQuery(
    query: CandidateListQuery,
    enabled: boolean,
) {
    return useQuery({
        queryKey: [...paymentRequestKeys.candidates, "page", query],
        queryFn: () => fetchCandidatePage(query),
        enabled,
    })
}
export function usePaymentCandidateIdsQuery(
    filters: Partial<CandidateFilters>,
) {
    return useQuery({
        queryKey: [...paymentRequestKeys.candidates, "ids", filters],
        queryFn: () => fetchCandidateIds(filters),
        enabled: false,
        staleTime: 0,
    })
}
export function usePaymentRequestQuery(
    id: string,
    workItemId?: string,
    enabled = true,
) {
    return useQuery({
        queryKey: paymentRequestKeys.detail(id, workItemId),
        queryFn: () => fetchRequest(id, workItemId),
        enabled: enabled && Boolean(id),
    })
}
export function usePaymentSubmissionQuery(
    id: string,
    submissionVersion: number | null,
) {
    return useQuery({
        queryKey: [
            ...paymentRequestKeys.all,
            "submission",
            id,
            submissionVersion,
        ],
        queryFn: () => fetchSubmission(id, submissionVersion!),
        enabled: Boolean(id) && submissionVersion != null,
    })
}
export function usePaymentMaterialsQuery(
    id: string,
    workItemId?: string,
    submissionVersion?: number,
) {
    return useQuery({
        queryKey: [
            ...paymentRequestKeys.detail(id, workItemId),
            "materials",
            submissionVersion ?? "current",
        ],
        queryFn: () => fetchMaterials(id, workItemId, submissionVersion),
        enabled: Boolean(id),
    })
}

function useInvalidate() {
    const client = useQueryClient()
    return async () => {
        await Promise.all([
            client.invalidateQueries({ queryKey: paymentRequestKeys.all }),
            client.invalidateQueries({ queryKey: ["work-items"] }),
            client.invalidateQueries({ queryKey: ["workspace-home"] }),
            client.invalidateQueries({ queryKey: ["supplier-payables"] }),
            client.invalidateQueries({ queryKey: ["purchase-orders"] }),
        ])
    }
}
export function useSaveRequestMutation() {
    const invalidate = useInvalidate()
    return useMutation({
        mutationFn: (input: {
            data: Parameters<typeof saveRequest>[0]
            id?: string
            version?: number
        }) => saveRequest(input.data, input.id, input.version),
        onSuccess: invalidate,
    })
}
export function useRequestCommandMutation() {
    const invalidate = useInvalidate()
    return useMutation({
        mutationFn: (input: {
            id: string
            action: Parameters<typeof commandRequest>[1]
            command: Parameters<typeof commandRequest>[2]
        }) => commandRequest(input.id, input.action, input.command),
        onSuccess: invalidate,
    })
}
export function useRequestPaymentMutation() {
    const invalidate = useInvalidate()
    const client = useQueryClient()
    return useMutation({
        mutationFn: (input: {
            payment: Parameters<typeof commitPayment>[0]
            recipient: Parameters<typeof commitPayment>[1]
        }) => commitPayment(input.payment, input.recipient),
        onSuccess: async () => {
            await Promise.all([
                invalidate(),
                client.invalidateQueries({
                    queryKey: ["fulfillment-operations"],
                }),
            ])
        },
    })
}
export function useRequestDownloadMutation() {
    return useMutation({
        mutationFn: (input: {
            requestId: string
            attachment: Parameters<typeof downloadAttachment>[1]
            workItemId?: string
            submissionVersion?: number
        }) =>
            downloadAttachment(
                input.requestId,
                input.attachment,
                input.workItemId,
                input.submissionVersion,
            ),
    })
}
export function useAccountRevealMutation() {
    return useMutation({
        mutationFn: (input: {
            request: Parameters<typeof revealAccount>[0]
            task: Parameters<typeof revealAccount>[1]
        }) => revealAccount(input.request, input.task),
    })
}

export function useSubmitRequestBatchMutation() {
    const invalidate = useInvalidate()
    return useMutation({
        mutationFn: submitRequestBatch,
        onSuccess: invalidate,
    })
}
