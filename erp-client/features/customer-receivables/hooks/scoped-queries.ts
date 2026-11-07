"use client"

import { useQuery } from "@tanstack/react-query"

import { useScopedListQuery } from "@/lib/use-scoped-list-query"
import {
    fetchReceivableScopeDetail,
    fetchReceivableScopeList,
} from "@/features/customer-receivables/api/scoped-view"
import type { ReceivableScopeQuery } from "@/features/customer-receivables/api/scoped"

export const receivableScopeKeys = {
    all: ["customer-receivables", "scoped"] as const,
    list: (query: ReceivableScopeQuery) =>
        [...receivableScopeKeys.all, "list", query] as const,
    detail: (kind: string, id: string) =>
        [...receivableScopeKeys.all, "detail", kind, id] as const,
}

export function useReceivableScopeListQuery(query: ReceivableScopeQuery) {
    return useScopedListQuery({
        query,
        queryKey: receivableScopeKeys.list,
        fetchPage: fetchReceivableScopeList,
        keepPreviousData: true,
    })
}

export function useReceivableScopeDetailQuery(
    kind: "receivable" | "receipt" | "invoice" | null,
    id: string | null,
    refreshOnMount = false,
) {
    return useQuery({
        queryKey: receivableScopeKeys.detail(kind ?? "", id ?? ""),
        staleTime: refreshOnMount ? 0 : undefined,
        queryFn: () => fetchReceivableScopeDetail(kind!, id!),
        enabled: Boolean(kind && id),
    })
}
