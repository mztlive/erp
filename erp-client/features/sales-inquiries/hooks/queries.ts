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
    feedbackCount: () => [...inquiryKeys.all, "feedback-count"] as const,
    procurementCount: () => [...inquiryKeys.all, "procurement-count"] as const,
    list: (query: InquiryListQuery, procurement: boolean) =>
        [...inquiryKeys.all, "list", procurement, query] as const,
    detail: (id: string, procurement: boolean, taskId?: string) =>
        [...inquiryKeys.all, "detail", procurement, id, taskId] as const,
    skuDetails: (skuIds: readonly string[]) =>
        [...inquiryKeys.all, "sku-details", skuIds] as const,
}

/** 统计当前可见范围内已发布、尚待销售处理的询价单，不受列表分页和筛选影响。 */
export function useInquiryFeedbackCountQuery(enabled = true) {
    return useQuery({
        queryKey: inquiryKeys.feedbackCount(),
        queryFn: async () => {
            const result = await fetchInquiries({
                status: "awaiting_sales",
                page: 1,
                page_size: 20,
            })
            return result.total
        },
        enabled,
        staleTime: 30_000,
        refetchInterval: 30_000,
        refetchOnWindowFocus: true,
    })
}

/** 采购接口只返回本人开放任务对应的询价，包含新分派、重询和补充后重提。 */
export function useProcurementInquiryCountQuery(enabled = true) {
    return useQuery({
        queryKey: inquiryKeys.procurementCount(),
        queryFn: async () => {
            const result = await fetchInquiries(
                { page: 1, page_size: 20 },
                true,
            )
            return result.total
        },
        enabled,
        staleTime: 30_000,
        refetchInterval: 30_000,
        refetchOnWindowFocus: true,
    })
}

/** 批量复用可售商品的主图与单位名称，询价无需读取单位管理目录。 */
export function useInquirySkuDetailsQuery(skuIds: readonly string[]) {
    const ids = [...new Set(skuIds.filter(Boolean))].sort()
    const batches: string[][] = []
    for (let offset = 0; offset < ids.length; offset += 50) {
        batches.push(ids.slice(offset, offset + 50))
    }
    const queries = useQueries({
        queries: batches.map((batch) => ({
            queryKey: inquiryKeys.skuDetails(batch),
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
    const images = new Map(
        queries.flatMap((query) =>
            (query.data?.rows ?? []).map(
                (row) =>
                    [row.stableId, row.sellableItem?.mainImageAssetId] as const,
            ),
        ),
    )
    const units = new Map<string, string>()
    for (const query of queries) {
        for (const row of query.data?.rows ?? []) {
            const item = row.sellableItem
            if (item?.baseUnitCode && item.baseUnit && item.baseUnit !== "—") {
                units.set(item.baseUnitCode, item.baseUnit)
            }
        }
    }
    return {
        images,
        units,
        isPending: queries.some((query) => query.isPending),
        isFetching: queries.some((query) => query.isFetching),
        error: queries.find((query) => query.isError)?.error,
        refetch: () => Promise.all(queries.map((query) => query.refetch())),
    }
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
    enabled = true,
) {
    return useQuery({
        queryKey: inquiryKeys.detail(inquiryId, procurement, taskId),
        queryFn: () => fetchInquiry(inquiryId, procurement, taskId),
        enabled: Boolean(inquiryId) && enabled,
    })
}
