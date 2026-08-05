"use client"

import { useMutation, useQuery } from "@tanstack/react-query"

import {
  fetchCardBusinessAnalytics,
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
        sort: "consumption:desc",
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

export function useStartCardBusinessExportMutation() {
  return useMutation({
    mutationFn: startCardBusinessExport,
  })
}

export type { CardBusinessAnalyticsQuery }
