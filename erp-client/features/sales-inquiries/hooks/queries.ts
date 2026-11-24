"use client"

import {
    keepPreviousData,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query"
import { fetchInquiries, fetchInquiry } from "../api"
import {
    indexProductUnitNames,
    useProductUnitDirectoryQuery,
    useSkuPresentationQueries,
} from "@/hooks/use-product-unit-names"
import type { InquiryListQuery } from "../types"

export const inquiryKeys = {
    all: ["sales-inquiries"] as const,
    feedbackCount: () => [...inquiryKeys.all, "feedback-count"] as const,
    procurementCount: () => [...inquiryKeys.all, "procurement-count"] as const,
    list: (query: InquiryListQuery, procurement: boolean) =>
        [...inquiryKeys.all, "list", procurement, query] as const,
    detail: (id: string, procurement: boolean, taskId?: string) =>
        [...inquiryKeys.all, "detail", procurement, id, taskId] as const,
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
        refetchOnWindowFocus: true,
    })
}

/** 待处理角标只统计本人开放任务，已发布的历史记录不计入。 */
export function useProcurementInquiryCountQuery(enabled = true) {
    return useQuery({
        queryKey: inquiryKeys.procurementCount(),
        queryFn: async () => {
            const result = await fetchInquiries(
                { page: 1, page_size: 20, task_status: "open" },
                true,
            )
            return result.total
        },
        enabled,
        staleTime: 30_000,
        refetchOnWindowFocus: true,
    })
}

/** 批量读取可售商品主图和单位名称，历史单位由有权读取的目录补齐。 */
export function useInquirySkuDetailsQuery(skuIds: readonly string[]) {
    const { rows, ...queryState } = useSkuPresentationQueries(skuIds)
    const directory = useProductUnitDirectoryQuery(skuIds.length > 0)
    const images = new Map(
        rows.map(
            (row) =>
                [row.sku_id, row.main_image_asset_id ?? undefined] as const,
        ),
    )
    const { names: units } = indexProductUnitNames(rows, directory)
    return {
        images,
        units,
        ...queryState,
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
