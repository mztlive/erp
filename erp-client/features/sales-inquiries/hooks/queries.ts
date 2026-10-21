"use client"

import {
    keepPreviousData,
    useQueries,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query"
import { fetchInquiries, fetchInquiry } from "../api"
import { listSellableItemsPage } from "@/features/master-data/api"
import type { InquiryListQuery } from "../types"

export const inquiryKeys = {
    all: ["sales-inquiries"] as const,
    list: (query: InquiryListQuery, procurement: boolean) =>
        [...inquiryKeys.all, "list", procurement, query] as const,
    detail: (id: string, procurement: boolean, taskId?: string) =>
        [...inquiryKeys.all, "detail", procurement, id, taskId] as const,
    skuImages: (skuIds: readonly string[]) =>
        [...inquiryKeys.all, "sku-images", skuIds] as const,
}

/** 按需求中的 SKU 批量读取主图，避免逐行请求或拉取整个商品池。 */
export function useInquirySkuImages(skuIds: readonly string[]) {
    const ids = [...new Set(skuIds.filter(Boolean))].sort()
    const batches: string[][] = []
    for (let offset = 0; offset < ids.length; offset += 50) {
        batches.push(ids.slice(offset, offset + 50))
    }
    const queries = useQueries({
        queries: batches.map((batch) => ({
            queryKey: inquiryKeys.skuImages(batch),
            queryFn: () =>
                listSellableItemsPage({
                    resource: "sellable-items",
                    skuIds: batch,
                    page: 1,
                    pageSize: 50,
                }),
            staleTime: 5 * 60 * 1000,
        })),
    })
    return new Map(
        queries.flatMap((query) =>
            (query.data?.rows ?? []).map(
                (row) =>
                    [row.stableId, row.sellableItem?.mainImageAssetId] as const,
            ),
        ),
    )
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
