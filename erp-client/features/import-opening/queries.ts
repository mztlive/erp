"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  acknowledgeUploadReceived,
  fetchImportBatchDetail,
  fetchImportBatchList,
  fetchImportIssues,
  invalidateTrialByRuleChange,
  openRepairBatch,
} from "@/features/import-opening/api"
import type {
  ImportBatchListQuery,
  ImportIssueQuery,
  ViewerRoleDemo,
} from "@/features/import-opening/types"

export const importOpeningKeys = {
  all: ["import-opening"] as const,
  list: (query: ImportBatchListQuery & { role?: ViewerRoleDemo }) =>
    [...importOpeningKeys.all, "list", query] as const,
  detail: (batchId: string, role?: ViewerRoleDemo) =>
    [...importOpeningKeys.all, "detail", batchId, role ?? "SYSTEM_ADMIN"] as const,
  issues: (query: ImportIssueQuery) =>
    [...importOpeningKeys.all, "issues", query] as const,
}

export function useImportBatchListQuery(
  query: ImportBatchListQuery & { role?: ViewerRoleDemo }
) {
  return useQuery({
    queryKey: importOpeningKeys.list(query),
    queryFn: () => fetchImportBatchList(query),
  })
}

export function useImportBatchDetailQuery(
  batchId: string | undefined,
  role?: ViewerRoleDemo
) {
  return useQuery({
    queryKey: importOpeningKeys.detail(batchId ?? "", role),
    queryFn: () =>
      fetchImportBatchDetail({ batchId: batchId!, role }),
    enabled: Boolean(batchId),
  })
}

export function useImportIssuesQuery(query: ImportIssueQuery, enabled = true) {
  return useQuery({
    queryKey: importOpeningKeys.issues(query),
    queryFn: () => fetchImportIssues(query),
    enabled: enabled && Boolean(query.batchId),
  })
}

export function useInvalidateTrialMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: invalidateTrialByRuleChange,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: importOpeningKeys.all })
    },
  })
}

export function useOpenRepairBatchMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: openRepairBatch,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: importOpeningKeys.all })
    },
  })
}

export function useAcknowledgeUploadMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: acknowledgeUploadReceived,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: importOpeningKeys.all })
    },
  })
}
