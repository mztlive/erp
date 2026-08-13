"use client"

import {
    keepPreviousData,
    useMutation,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query"

import {
    appendDifferenceEvidence,
    claimSettlementReview,
    createSettlementDraft,
    decideSettlementReview,
    fetchSettlementDetail,
    fetchSettlementList,
    refreshSettlementTrial,
    resolveDifference,
    submitSettlementReview,
    type ListQueryInput,
} from "@/features/supplier-settlements/api/settlements"

const settlementKeys = {
    all: ["supplier-settlements"] as const,
    list: (input: ListQueryInput) =>
        [...settlementKeys.all, "list", input] as const,
    detail: (statementId: string) =>
        [...settlementKeys.all, "detail", statementId] as const,
}

export function useSettlementListQuery(input: ListQueryInput) {
    return useQuery({
        queryKey: settlementKeys.list(input),
        queryFn: () => fetchSettlementList(input),
        placeholderData: keepPreviousData,
    })
}

export function useSettlementDetailQuery(statementId: string | undefined) {
    return useQuery({
        queryKey: settlementKeys.detail(statementId ?? ""),
        queryFn: () => fetchSettlementDetail({ statementId: statementId! }),
        enabled: Boolean(statementId),
        placeholderData: keepPreviousData,
    })
}

function useInvalidateAll() {
    const queryClient = useQueryClient()
    return async () => {
        await queryClient.invalidateQueries({ queryKey: settlementKeys.all })
    }
}

export function useCreateDraftMutation() {
    const invalidate = useInvalidateAll()
    return useMutation({
        mutationFn: createSettlementDraft,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}

export function useRefreshTrialMutation() {
    const invalidate = useInvalidateAll()
    return useMutation({
        mutationFn: refreshSettlementTrial,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}

export function useAppendEvidenceMutation() {
    const invalidate = useInvalidateAll()
    return useMutation({
        mutationFn: appendDifferenceEvidence,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}

export function useResolveDifferenceMutation() {
    const invalidate = useInvalidateAll()
    return useMutation({
        mutationFn: resolveDifference,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}

export function useSubmitReviewMutation() {
    const invalidate = useInvalidateAll()
    return useMutation({
        mutationFn: submitSettlementReview,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}

export function useReviewDecisionMutation() {
    const invalidate = useInvalidateAll()
    return useMutation({
        mutationFn: decideSettlementReview,
        onSuccess: async (result) => {
            if (
                result.status === "succeeded" ||
                result.status === "rejected" ||
                result.status === "unknown"
            ) {
                await invalidate()
            }
        },
    })
}

export function useClaimReviewMutation() {
    const invalidate = useInvalidateAll()
    return useMutation({
        mutationFn: claimSettlementReview,
        onSuccess: async (result) => {
            if (result.status === "succeeded") await invalidate()
        },
    })
}
