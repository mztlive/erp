"use client"

import { useQuery } from "@tanstack/react-query"

import { fetchOpaqueReferenceOptions } from "@/features/supplier-api-connections/api/connections"

export function useOpaqueReferenceOptionsQuery(
    connectionId: string,
    kind: "credential" | "endpoint",
    connectionVersion: string,
    enabled: boolean,
) {
    return useQuery({
        queryKey: [
            "supplier-api-connections",
            "detail",
            connectionId,
            "reference-options",
            kind,
            connectionVersion,
        ],
        queryFn: () => fetchOpaqueReferenceOptions(connectionId, kind),
        enabled: enabled && Boolean(connectionId),
        staleTime: 0,
        gcTime: 0,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
    })
}
