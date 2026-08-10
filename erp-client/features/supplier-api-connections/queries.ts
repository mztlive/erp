"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  bindCredentialReference,
  bindEndpointReference,
  createConnection,
  disableConnection,
  enableConnection,
  fetchConnectionCenter,
  fetchConnectionList,
  runHealthCheck,
  startCatalogSync,
  updateCapabilities,
  type ListQueryInput,
} from "@/features/supplier-api-connections/api"

const supplierConnectionKeys = {
  all: ["supplier-api-connections"] as const,
  list: (input: ListQueryInput) =>
    [...supplierConnectionKeys.all, "list", input] as const,
  center: (connectionId: string) =>
    [...supplierConnectionKeys.all, "center", connectionId] as const,
}

export function useConnectionListQuery(input: ListQueryInput) {
  return useQuery({
    queryKey: supplierConnectionKeys.list(input),
    queryFn: () => fetchConnectionList(input),
  })
}

export function useConnectionCenterQuery(connectionId: string | undefined) {
  return useQuery({
    queryKey: supplierConnectionKeys.center(connectionId ?? ""),
    queryFn: () => fetchConnectionCenter({ connectionId: connectionId! }),
    enabled: Boolean(connectionId),
  })
}

function useInvalidateAll() {
  const queryClient = useQueryClient()
  return async () => {
    await queryClient.invalidateQueries({
      queryKey: supplierConnectionKeys.all,
    })
  }
}

export function useCreateConnectionMutation() {
  const invalidate = useInvalidateAll()
  return useMutation({
    mutationFn: createConnection,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}

export function useBindCredentialMutation() {
  const invalidate = useInvalidateAll()
  return useMutation({
    mutationFn: bindCredentialReference,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}

export function useBindEndpointMutation() {
  const invalidate = useInvalidateAll()
  return useMutation({
    mutationFn: bindEndpointReference,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}

export function useUpdateCapabilitiesMutation() {
  const invalidate = useInvalidateAll()
  return useMutation({
    mutationFn: updateCapabilities,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}

export function useRunHealthCheckMutation() {
  const invalidate = useInvalidateAll()
  return useMutation({
    mutationFn: runHealthCheck,
    onSuccess: async (result) => {
      if (
        result.status === "processing" ||
        result.status === "succeeded" ||
        result.status === "unknown"
      ) {
        await invalidate()
      }
    },
  })
}

export function useStartCatalogSyncMutation() {
  const invalidate = useInvalidateAll()
  return useMutation({
    mutationFn: startCatalogSync,
    onSuccess: async (result) => {
      if (result.status === "processing" || result.status === "succeeded") {
        await invalidate()
      }
    },
  })
}

export function useDisableConnectionMutation() {
  const invalidate = useInvalidateAll()
  return useMutation({
    mutationFn: disableConnection,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}

export function useEnableConnectionMutation() {
  const invalidate = useInvalidateAll()
  return useMutation({
    mutationFn: enableConnection,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}
