"use client"

import { useQuery } from "@tanstack/react-query"

import { useScopedListQuery } from "@/lib/use-scoped-list-query"
import {
    fetchInvoiceRequestScopeDetail,
    fetchInvoiceRequestScopeList,
} from "@/features/invoice-requests/scoped-view"
import type { InvoiceRequestScopeQuery } from "@/features/invoice-requests/scoped"

export const invoiceRequestScopeKeys = {
    all: ["invoice-requests", "scoped"] as const,
    list: (query: InvoiceRequestScopeQuery) =>
        [...invoiceRequestScopeKeys.all, "list", query] as const,
    detail: (id: string) =>
        [...invoiceRequestScopeKeys.all, "detail", id] as const,
}

export function useInvoiceRequestScopeListQuery(
    query: InvoiceRequestScopeQuery,
) {
    return useScopedListQuery({
        query,
        queryKey: invoiceRequestScopeKeys.list,
        fetchPage: fetchInvoiceRequestScopeList,
        keepPreviousData: true,
    })
}

export function useInvoiceRequestScopeDetailQuery(id: string | null) {
    return useQuery({
        queryKey: invoiceRequestScopeKeys.detail(id ?? ""),
        queryFn: () => fetchInvoiceRequestScopeDetail(id!),
        enabled: Boolean(id),
    })
}
