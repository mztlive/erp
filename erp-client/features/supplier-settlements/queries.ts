"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  appendDifferenceEvidence,
  createSettlementDraft,
  decideSettlementReview,
  fetchSettlementDetail,
  fetchSettlementList,
  queryFormalByIdempotency,
  refreshSettlementTrial,
  resolveDifference,
  submitSettlementReview,
  type ListQueryInput,
} from "@/features/supplier-settlements/api"
import type { DemoRole } from "@/features/supplier-settlements/types"

export const settlementKeys = {
  all: ["supplier-settlements"] as const,
  list: (input: ListQueryInput) =>
    [...settlementKeys.all, "list", input] as const,
  detail: (statementId: string, role: DemoRole) =>
    [...settlementKeys.all, "detail", statementId, role] as const,
}

export function useSettlementListQuery(input: ListQueryInput) {
  return useQuery({
    queryKey: settlementKeys.list(input),
    queryFn: () => fetchSettlementList(input),
  })
}

export function useSettlementDetailQuery(
  statementId: string | undefined,
  role: DemoRole
) {
  return useQuery({
    queryKey: settlementKeys.detail(statementId ?? "", role),
    queryFn: () =>
      fetchSettlementDetail({ statementId: statementId!, role }),
    enabled: Boolean(statementId),
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

export function useQueryFormalIdempotencyMutation() {
  const invalidate = useInvalidateAll()
  return useMutation({
    mutationFn: (key: string) => queryFormalByIdempotency(key),
    onSuccess: async (result) => {
      if (result?.status === "succeeded" || result?.status === "rejected") {
        await invalidate()
      }
    },
  })
}
