"use client"

import { useQuery, useQueryClient } from "@tanstack/react-query"

import {
    fetchSupplierScopeDetail,
    fetchSupplierScopeList,
} from "@/features/supplier-payables/api/scoped-view"
import type {
    SupplierScopeListView,
    SupplierScopeQuery,
} from "@/features/supplier-payables/api/scoped"

export const supplierScopeKeys = {
    all: ["supplier-payables", "scoped"] as const,
    list: (query: SupplierScopeQuery) =>
        [...supplierScopeKeys.all, "list", query] as const,
    detail: (kind: string, id: string) =>
        [...supplierScopeKeys.all, "detail", kind, id] as const,
}

/** 范围列表：第二页起自动绑定首个响应的范围版本，避免拼接不同授权页。 */
export function useSupplierScopeListQuery(query: SupplierScopeQuery) {
    const client = useQueryClient()
    const firstPage = { ...query, page: 1, scopeVersion: undefined }
    const baseline = client.getQueryData<SupplierScopeListView>(
        supplierScopeKeys.list(firstPage),
    )
    const scopeVersion =
        query.scopeVersion ??
        (query.page > 1 ? baseline?.scopeVersion : undefined)
    const scoped = { ...query, scopeVersion }
    return useQuery({
        queryKey: supplierScopeKeys.list(scoped),
        queryFn: async () => {
            if (query.page === 1 || scopeVersion)
                return fetchSupplierScopeList(scoped)
            const first = await client.fetchQuery({
                queryKey: supplierScopeKeys.list(firstPage),
                queryFn: () => fetchSupplierScopeList(firstPage),
            })
            return fetchSupplierScopeList({
                ...query,
                scopeVersion: first.scopeVersion,
            })
        },
        placeholderData: (previous) => previous,
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
