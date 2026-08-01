"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  clearSourceCorrectionPending,
  fetchCostEntriesForRow,
  fetchCostEntryDetail,
  fetchExportJob,
  fetchPeriodBasisConfig,
  fetchProfitLossView,
  markSourceCorrectionPending,
  startProfitLossExport,
  type PeriodBasisConfigQuery,
} from "@/features/actual-profit-loss/api"
import type {
  ProfitLossQuery,
  ProfitLossView,
} from "@/features/actual-profit-loss/types"

export const profitLossKeys = {
  all: ["actual-profit-loss"] as const,
  periodBasis: (q: PeriodBasisConfigQuery) =>
    [...profitLossKeys.all, "period-basis", q] as const,
  view: (query: ProfitLossQuery) =>
    [...profitLossKeys.all, "view", query] as const,
  costEntry: (id: string) =>
    [...profitLossKeys.all, "cost-entry", id] as const,
  costEntries: (ids: readonly string[]) =>
    [...profitLossKeys.all, "cost-entries", [...ids].sort().join(",")] as const,
  exportJob: (jobId: string) =>
    [...profitLossKeys.all, "export", jobId] as const,
}

export function usePeriodBasisConfigQuery(q: PeriodBasisConfigQuery = {}) {
  return useQuery({
    queryKey: profitLossKeys.periodBasis(q),
    queryFn: () => fetchPeriodBasisConfig(q),
    staleTime: 30_000,
  })
}

export function useProfitLossViewQuery(
  query: ProfitLossQuery | null,
  enabled: boolean
) {
  return useQuery({
    queryKey: profitLossKeys.view(
      query ?? {
        from: "",
        to: "",
        periodBasis: "",
        scopeId: "",
        coverage: "covered",
        dimension: "sales_order",
        sort: "actualProfitLossNet:asc",
        pageSize: 20,
      }
    ),
    queryFn: () => fetchProfitLossView(query!),
    enabled: enabled && query != null && Boolean(query.periodBasis),
  })
}

export function useCostEntryDetailQuery(costEntryId: string | null) {
  return useQuery({
    queryKey: profitLossKeys.costEntry(costEntryId ?? ""),
    queryFn: () => fetchCostEntryDetail(costEntryId!),
    enabled: Boolean(costEntryId),
  })
}

export function useCostEntriesForRowQuery(costEntryIds: readonly string[]) {
  return useQuery({
    queryKey: profitLossKeys.costEntries(costEntryIds),
    queryFn: () => fetchCostEntriesForRow(costEntryIds),
    enabled: costEntryIds.length > 0,
  })
}

export function useExportJobQuery(jobId: string | null) {
  return useQuery({
    queryKey: profitLossKeys.exportJob(jobId ?? ""),
    queryFn: () => fetchExportJob(jobId!),
    enabled: Boolean(jobId),
    refetchInterval: (q) => {
      const status = q.state.data?.status
      if (status === "succeeded" || status === "failed") return false
      return 400
    },
  })
}

export function useStartProfitLossExportMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: startProfitLossExport,
    onSuccess: async (job) => {
      await queryClient.invalidateQueries({
        queryKey: profitLossKeys.exportJob(job.jobId),
      })
    },
  })
}

export function useMarkCorrectionPendingMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: markSourceCorrectionPending,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: profitLossKeys.all })
    },
  })
}

export function useClearCorrectionPendingMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: clearSourceCorrectionPending,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: profitLossKeys.all })
    },
  })
}

export type { ProfitLossQuery, ProfitLossView }
