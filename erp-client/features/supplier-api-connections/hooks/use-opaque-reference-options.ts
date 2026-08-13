"use client"

import { useQuery } from "@tanstack/react-query"

import { fetchOpaqueReferenceOptions } from "@/features/supplier-api-connections/api/connections"

export function useOpaqueReferenceOptionsQuery(
    kind: "credential" | "endpoint",
) {
    return useQuery({
        queryKey: [
            "supplier-api-connections",
            "opaque-reference-options",
            kind,
        ],
        queryFn: () => fetchOpaqueReferenceOptions(kind),
        staleTime: 5 * 60 * 1000,
    })
}
