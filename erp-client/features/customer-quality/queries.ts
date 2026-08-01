"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  fetchCustomerQuality,
  fetchCustomerQualityExportJob,
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
  exportJob: (jobId: string) =>
    [...customerQualityKeys.all, "export", jobId] as const,
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
        pageSize: 50,
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

export function useCustomerQualityExportJobQuery(jobId: string | null) {
  return useQuery({
    queryKey: customerQualityKeys.exportJob(jobId ?? ""),
    queryFn: () => fetchCustomerQualityExportJob(jobId!),
    enabled: Boolean(jobId),
    refetchInterval: (q) => {
      const status = q.state.data?.status
      if (status === "succeeded" || status === "failed") return false
      return 400
    },
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
