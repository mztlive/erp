"use client"

import { useQuery } from "@tanstack/react-query"

import { useScopedListQuery } from "@/lib/use-scoped-list-query"
import {
    fetchSupplierScopeDetail,
    fetchSupplierScopeList,
} from "@/features/supplier-payables/api/scoped-view"
import type { SupplierScopeQuery } from "@/features/supplier-payables/api/scoped"

export const supplierScopeKeys = {
    all: ["supplier-payables", "scoped"] as const,
    list: (query: SupplierScopeQuery) =>
        [...supplierScopeKeys.all, "list", query] as const,
    detail: (kind: string, id: string) =>
        [...supplierScopeKeys.all, "detail", kind, id] as const,
}

export function useSupplierScopeListQuery(
    query: SupplierScopeQuery,
    enabled: boolean,
) {
    return useScopedListQuery({
        query,
        queryKey: supplierScopeKeys.list,
        fetchPage: fetchSupplierScopeList,
        enabled,
        keepPreviousData: true,
    })
}

export function useSupplierScopeDetailQuery(
    kind: "payable" | "payment" | null,
    id: string | null,
    refreshOnMount = false,
) {
    return useQuery({
        queryKey: supplierScopeKeys.detail(kind ?? "", id ?? ""),
        staleTime: refreshOnMount ? 0 : undefined,
        queryFn: () => fetchSupplierScopeDetail(kind!, id!),
        enabled: Boolean(kind && id),
    })
}
