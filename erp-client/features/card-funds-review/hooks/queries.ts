"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    completeCardFundsReview,
    fetchCardFundsReviewQueue,
    fetchFocusedCardFundsReviewTask,
    registerHistoricalInvoice,
    registerHistoricalReceipt,
} from "@/features/card-funds-review/api"
import type { CardFundsReviewQueueQuery } from "@/features/card-funds-review/types"

const cardFundsReviewKeys = {
    all: ["card-funds-review"] as const,
    queue: (query: CardFundsReviewQueueQuery) =>
        [...cardFundsReviewKeys.all, "queue", query] as const,
    focused: (workItemId: string) =>
        [...cardFundsReviewKeys.all, "focused", workItemId] as const,
}

export function useCardFundsReviewQueueQuery(
    query: CardFundsReviewQueueQuery,
    focusedWorkItemId?: string,
) {
    return useQuery({
        queryKey: focusedWorkItemId
            ? cardFundsReviewKeys.focused(focusedWorkItemId)
            : cardFundsReviewKeys.queue(query),
        queryFn: () =>
            focusedWorkItemId
                ? fetchFocusedCardFundsReviewTask(focusedWorkItemId)
                : fetchCardFundsReviewQueue(query),
    })
}

export function useCompleteCardFundsMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: completeCardFundsReview,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: cardFundsReviewKeys.all,
                })
            }
        },
    })
}

export function useRegisterReceiptMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: registerHistoricalReceipt,
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: cardFundsReviewKeys.all,
            })
        },
    })
}

export function useRegisterInvoiceMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: registerHistoricalInvoice,
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: cardFundsReviewKeys.all,
            })
        },
    })
}
