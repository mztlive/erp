"use client"

import { useQuery, useQueryClient } from "@tanstack/react-query"

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

/** 范围列表：第二页起自动绑定首个响应的范围版本，避免拼接不同授权页。 */
export function useReceivableScopeListQuery(query: ReceivableScopeQuery) {
    const client = useQueryClient()
    const firstPage = { ...query, page: 1, scopeVersion: undefined }
    const baseline = client.getQueryData<
        import("@/features/customer-receivables/api/scoped").ReceivableScopeListView
    >(receivableScopeKeys.list(firstPage))
    const scopeVersion =
        query.scopeVersion ??
        (query.page > 1 ? baseline?.scopeVersion : undefined)
    const scoped = { ...query, scopeVersion }
    return useQuery({
        queryKey: receivableScopeKeys.list(scoped),
        queryFn: async () => {
            if (query.page === 1 || scopeVersion)
                return fetchReceivableScopeList(scoped)
            const first = await client.fetchQuery({
                queryKey: receivableScopeKeys.list(firstPage),
                queryFn: () => fetchReceivableScopeList(firstPage),
            })
            return fetchReceivableScopeList({
                ...query,
                scopeVersion: first.scopeVersion,
            })
        },
        // 切换筛选时保留上一批结果渲染，避免整卡闪烁。
        placeholderData: (previous) => previous,
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
