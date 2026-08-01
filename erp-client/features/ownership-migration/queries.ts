"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  enableCutoverDemoReady,
  fetchConsumptionCutover,
  fetchMaintenanceFreeze,
  fetchOwnershipMigrationBatch,
  fetchOwnershipMigrationList,
  submitMigrationFormal,
} from "@/features/ownership-migration/api"
import type {
  MigrationFormalCommand,
  OwnershipMigrationListQuery,
  ViewerRoleDemo,
} from "@/features/ownership-migration/types"

export const ownershipMigrationKeys = {
  all: ["ownership-migration"] as const,
  freeze: () => [...ownershipMigrationKeys.all, "freeze"] as const,
  list: (query: OwnershipMigrationListQuery) =>
    [...ownershipMigrationKeys.all, "list", query] as const,
  batch: (batchId: string, role?: ViewerRoleDemo) =>
    [...ownershipMigrationKeys.all, "batch", batchId, role ?? "SYSTEM_ADMIN"] as const,
  cutover: (mallId: string, role?: ViewerRoleDemo) =>
    [...ownershipMigrationKeys.all, "cutover", mallId, role ?? "SYSTEM_ADMIN"] as const,
}

export function useMaintenanceFreezeQuery() {
  return useQuery({
    queryKey: ownershipMigrationKeys.freeze(),
    queryFn: fetchMaintenanceFreeze,
    staleTime: 15_000,
    refetchInterval: 60_000,
  })
}

export function useOwnershipMigrationListQuery(
  query: OwnershipMigrationListQuery
) {
  return useQuery({
    queryKey: ownershipMigrationKeys.list(query),
    queryFn: () => fetchOwnershipMigrationList(query),
  })
}

export function useOwnershipMigrationBatchQuery(
  batchId: string | undefined,
  role?: ViewerRoleDemo
) {
  return useQuery({
    queryKey: ownershipMigrationKeys.batch(batchId ?? "", role),
    queryFn: () =>
      fetchOwnershipMigrationBatch({ batchId: batchId!, role }),
    enabled: Boolean(batchId),
  })
}

export function useConsumptionCutoverQuery(
  mallId: string,
  role?: ViewerRoleDemo
) {
  return useQuery({
    queryKey: ownershipMigrationKeys.cutover(mallId, role),
    queryFn: () => fetchConsumptionCutover({ mallId, role }),
  })
}

export function useMigrationFormalMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (command: MigrationFormalCommand) =>
      submitMigrationFormal(command),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ownershipMigrationKeys.all,
      })
    },
  })
}

export function useCutoverDemoReadyMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: enableCutoverDemoReady,
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ownershipMigrationKeys.all,
      })
    },
  })
}
