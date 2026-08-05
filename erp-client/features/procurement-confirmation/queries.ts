"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  claimProcurementWorkItem,
  completeProcurementDecision,
  deferProcurementConfirmation,
  fetchProcurementQueue,
  saveProcurementConfirmation,
  type QueueFilters,
} from "@/features/procurement-confirmation/api"

export const procurementConfirmKeys = {
  all: ["procurement-confirmation"] as const,
  queue: (filters: QueueFilters) =>
    [...procurementConfirmKeys.all, "queue", filters] as const,
}

export function useProcurementConfirmationQuery(filters: QueueFilters) {
  return useQuery({
    queryKey: procurementConfirmKeys.queue(filters),
    queryFn: () => fetchProcurementQueue(filters),
  })
}

export function useClaimProcurementMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: claimProcurementWorkItem,
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: procurementConfirmKeys.all,
      })
    },
  })
}

export function useSaveProcurementConfirmationMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: saveProcurementConfirmation,
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: procurementConfirmKeys.all,
      })
    },
  })
}

export function useCompleteProcurementMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: completeProcurementDecision,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: procurementConfirmKeys.all,
        })
      }
    },
  })
}

export function useDeferProcurementMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: deferProcurementConfirmation,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: procurementConfirmKeys.all,
        })
      }
    },
  })
}
