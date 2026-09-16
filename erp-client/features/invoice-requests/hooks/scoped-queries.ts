"use client"

import { useQuery, useQueryClient } from "@tanstack/react-query"

import {
    fetchInvoiceRequestScopeDetail,
    fetchInvoiceRequestScopeList,
} from "@/features/invoice-requests/scoped-view"
import type {
    InvoiceRequestScopeListView,
    InvoiceRequestScopeQuery,
} from "@/features/invoice-requests/scoped"

export const invoiceRequestScopeKeys = {
    all: ["invoice-requests", "scoped"] as const,
    list: (query: InvoiceRequestScopeQuery) =>
        [...invoiceRequestScopeKeys.all, "list", query] as const,
    detail: (id: string) =>
        [...invoiceRequestScopeKeys.all, "detail", id] as const,
}

/** 范围列表：第二页起自动绑定首个响应的范围版本，避免拼接不同授权页。 */
export function useInvoiceRequestScopeListQuery(
    query: InvoiceRequestScopeQuery,
) {
    const client = useQueryClient()
    const firstPage = { ...query, page: 1, scopeVersion: undefined }
    const baseline = client.getQueryData<InvoiceRequestScopeListView>(
        invoiceRequestScopeKeys.list(firstPage),
    )
    const scopeVersion =
        query.scopeVersion ??
        (query.page > 1 ? baseline?.scopeVersion : undefined)
    const scoped = { ...query, scopeVersion }
    return useQuery({
        queryKey: invoiceRequestScopeKeys.list(scoped),
        queryFn: async () => {
            if (query.page === 1 || scopeVersion)
                return fetchInvoiceRequestScopeList(scoped)
            const first = await client.fetchQuery({
                queryKey: invoiceRequestScopeKeys.list(firstPage),
                queryFn: () => fetchInvoiceRequestScopeList(firstPage),
            })
            return fetchInvoiceRequestScopeList({
                ...query,
                scopeVersion: first.scopeVersion,
            })
        },
        placeholderData: (previous) => previous,
    })
}

export function useInvoiceRequestScopeDetailQuery(id: string | null) {
    return useQuery({
        queryKey: invoiceRequestScopeKeys.detail(id ?? ""),
        queryFn: () => fetchInvoiceRequestScopeDetail(id!),
        enabled: Boolean(id),
    })
}
