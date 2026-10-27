"use client"

import { useEffect } from "react"
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
} from "@/features/supplier-api-connections/api/connections"

import {
    fetchCatalog,
    catalogRunning,
    matchCatalog,
    type CatalogFilters,
} from "@/features/supplier-api-connections/api/catalog"

import { fetchConnectorTypes } from "@/features/supplier-api-connections/api/list"

export function useConnectorTypesQuery(enabled: boolean) {
    return useQuery({
        queryKey: ["supplier-api-connector-types"],
        queryFn: fetchConnectorTypes,
        enabled,
    })
}

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
        refetchInterval: (query) =>
            query.state.data?.healthRecords.some(
                (record) => record.result === "UNKNOWN",
            )
                ? 2000
                : false,
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
            if (
                result.status === "processing" ||
                result.status === "succeeded"
            ) {
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

export function useCatalogQuery(
    connectionId: string,
    page: number,
    filters: CatalogFilters = {},
) {
    const queryClient = useQueryClient()
    const query = useQuery({
        queryKey: [
            ...supplierConnectionKeys.all,
            "catalog",
            connectionId,
            page,
            filters,
        ],
        queryFn: () => fetchCatalog(connectionId, page, filters),
        enabled: Boolean(connectionId),
        refetchInterval: (query) =>
            catalogRunning(query.state.data?.jobs[0])
                ? 2000
                : query.state.data?.items.some(
                        (item) => item.media_retry_at != null,
                    )
                  ? 10000
                  : false,
    })
    const latest = query.data?.jobs[0]
    const jobId = latest?.job_id
    useEffect(() => {
        if (jobId) {
            void queryClient.invalidateQueries({
                queryKey: supplierConnectionKeys.center(connectionId),
            })
        }
    }, [connectionId, jobId, latest?.status, latest?.stalled, queryClient])
    return query
}
export function useMatchCatalogMutation() {
    const invalidate = useInvalidateAll()
    return useMutation({ mutationFn: matchCatalog, onSuccess: invalidate })
}
