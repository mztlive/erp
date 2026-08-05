"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  fetchCustomerQuality,
  fetchCustomerQualityPeriodPolicy,
  startCustomerQualityExport,
} from "@/features/customer-quality/api"
import type {
  CustomerQualityQuery,
  CustomerQualityScenario,
} from "@/features/customer-quality/types"

export const customerQualityKeys = {
  all: ["customer-quality"] as const,
  periodPolicy: (scenario?: CustomerQualityScenario) =>
    [...customerQualityKeys.all, "period-policy", scenario ?? "default"] as const,
  view: (query: CustomerQualityQuery) =>
    [...customerQualityKeys.all, "view", query] as const,
}

export function useCustomerQualityPeriodPolicyQuery(
  scenario?: CustomerQualityScenario
) {
  return useQuery({
    queryKey: customerQualityKeys.periodPolicy(scenario),
    queryFn: () => fetchCustomerQualityPeriodPolicy({ scenario }),
    staleTime: 60_000,
  })
}

export function useCustomerQualityQuery(
  query: CustomerQualityQuery | null
) {
  return useQuery({
    queryKey: customerQualityKeys.view(
      query ?? {
        from: "",
        to: "",
        periodBasis: "EXPLICIT",
        periodSelectionSource: "EXPLICIT",
        scopeId: "",
        fundsReview: "all",
        sort: "salesGrossAmount:desc",
        page: 1,
        pageSize: 20,
      }
    ),
    queryFn: () => fetchCustomerQuality(query!),
    enabled: Boolean(query?.from && query?.to),
  })
}

export function useStartCustomerQualityExportMutation() {
  return useMutation({
    mutationFn: startCustomerQualityExport,
  })
}

export function useRefreshCustomerQualityMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async () => {
      await queryClient.invalidateQueries({
        queryKey: customerQualityKeys.all,
      })
    },
  })
}
