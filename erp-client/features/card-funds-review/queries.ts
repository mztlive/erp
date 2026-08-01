"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  claimCardFundsReviewWorkItem,
  completeCardFundsReview,
  demoDriftCardFundsHash,
  fetchCardFundsReviewQueue,
  holdCardFundsReview,
  registerHistoricalInvoice,
  registerHistoricalReceipt,
  resolveUnknownCardFundsResult,
  saveCardFundsEvidence,
} from "@/features/card-funds-review/api"
import type { CardFundsReviewQueueQuery } from "@/features/card-funds-review/types"

export const cardFundsReviewKeys = {
  all: ["card-funds-review"] as const,
  queue: (query: CardFundsReviewQueueQuery) =>
    [...cardFundsReviewKeys.all, "queue", query] as const,
}

export function useCardFundsReviewQueueQuery(query: CardFundsReviewQueueQuery) {
  return useQuery({
    queryKey: cardFundsReviewKeys.queue(query),
    queryFn: () => fetchCardFundsReviewQueue(query),
  })
}

export function useClaimCardFundsMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: claimCardFundsReviewWorkItem,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: cardFundsReviewKeys.all })
    },
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

export function useHoldCardFundsMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: holdCardFundsReview,
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
      await queryClient.invalidateQueries({ queryKey: cardFundsReviewKeys.all })
    },
  })
}

export function useRegisterInvoiceMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: registerHistoricalInvoice,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: cardFundsReviewKeys.all })
    },
  })
}

export function useSaveCardFundsEvidenceMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: saveCardFundsEvidence,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: cardFundsReviewKeys.all })
    },
  })
}

export function useResolveUnknownCardFundsMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: resolveUnknownCardFundsResult,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: cardFundsReviewKeys.all,
        })
      }
    },
  })
}

export function useDemoDriftHashMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: demoDriftCardFundsHash,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: cardFundsReviewKeys.all })
    },
  })
}
