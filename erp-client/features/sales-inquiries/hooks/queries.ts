"use client"

import {
    keepPreviousData,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query"
import { fetchInquiries, fetchInquiry } from "../api"
import type { InquiryListQuery } from "../types"

export const inquiryKeys = {
    all: ["sales-inquiries"] as const,
    list: (query: InquiryListQuery, procurement: boolean) =>
        [...inquiryKeys.all, "list", procurement, query] as const,
    detail: (id: string, procurement: boolean, taskId?: string) =>
        [...inquiryKeys.all, "detail", procurement, id, taskId] as const,
}

export function useInquiries(query: InquiryListQuery, procurement = false) {
    const client = useQueryClient()
    return useQuery({
        queryKey: inquiryKeys.list(query, procurement),
        queryFn: async () => {
            if (query.page <= 1 || query.scope_version)
                return fetchInquiries(query, procurement)
            const firstPage = { ...query, page: 1, scope_version: undefined }
            const first = await client.fetchQuery({
                queryKey: inquiryKeys.list(firstPage, procurement),
                queryFn: () => fetchInquiries(firstPage, procurement),
            })
            return fetchInquiries(
                { ...query, scope_version: first.scope_version },
                procurement,
            )
        },
        placeholderData: keepPreviousData,
    })
}

export function useInquiry(
    inquiryId: string,
    procurement = false,
    taskId?: string,
) {
    return useQuery({
        queryKey: inquiryKeys.detail(inquiryId, procurement, taskId),
        queryFn: () => fetchInquiry(inquiryId, procurement, taskId),
        enabled: Boolean(inquiryId),
    })
}
