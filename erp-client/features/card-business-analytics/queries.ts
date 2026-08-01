"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  fetchCardBusinessAnalytics,
  fetchCardBusinessExportJob,
  fetchDateBasisConfig,
  startCardBusinessExport,
  type DateBasisConfigQuery,
} from "@/features/card-business-analytics/api"
import type { CardBusinessAnalyticsQuery } from "@/features/card-business-analytics/types"

export const cardBusinessKeys = {
  all: ["card-business-analytics"] as const,
  dateBasis: (q: DateBasisConfigQuery) =>
    [...cardBusinessKeys.all, "date-basis", q] as const,
  view: (query: CardBusinessAnalyticsQuery) =>
    [...cardBusinessKeys.all, "view", query] as const,
  exportJob: (jobId: string) =>
    [...cardBusinessKeys.all, "export", jobId] as const,
}

export function useDateBasisConfigQuery(q: DateBasisConfigQuery = {}) {
  return useQuery({
    queryKey: cardBusinessKeys.dateBasis(q),
    queryFn: () => fetchDateBasisConfig(q),
    staleTime: 30_000,
  })
}

export function useCardBusinessAnalyticsQuery(
  query: CardBusinessAnalyticsQuery | null,
  enabled: boolean
) {
  return useQuery({
    queryKey: cardBusinessKeys.view(
      query ?? {
        from: "",
        to: "",
        dateBasis: "consumption",
        dimension: "customer",
        sort: "consumptionGross:desc",
        page: 1,
        pageSize: 50,
      }
    ),
    queryFn: () => fetchCardBusinessAnalytics(query!),
    enabled:
      enabled &&
      query != null &&
      Boolean(query.from) &&
      Boolean(query.to) &&
      Boolean(query.dateBasis),
  })
}

export function useCardBusinessExportJobQuery(jobId: string | null) {
  return useQuery({
    queryKey: cardBusinessKeys.exportJob(jobId ?? ""),
    queryFn: () => fetchCardBusinessExportJob(jobId!),
    enabled: Boolean(jobId),
    refetchInterval: (q) => {
      const status = q.state.data?.status
      if (status === "succeeded" || status === "failed") return false
      return 400
    },
  })
}

export function useStartCardBusinessExportMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: startCardBusinessExport,
    onSuccess: async (job) => {
      await queryClient.invalidateQueries({
        queryKey: cardBusinessKeys.exportJob(job.jobId),
      })
    },
  })
}

export type { CardBusinessAnalyticsQuery }
