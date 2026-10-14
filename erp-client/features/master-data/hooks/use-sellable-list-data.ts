"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

import {
    useSellableInfiniteQuery,
    useSellablePageQuery,
} from "@/features/master-data/hooks/queries"
import {
    SELLABLE_GALLERY_BATCH_SIZE,
    type SellableListLayout,
} from "@/features/master-data/lib/sellable-list-layout"
import type {
    MasterDataListItem,
    MasterDataListQuery,
} from "@/features/master-data/types"

/**
 * 公司商品池数据：表格按服务端页取当前页，卡片滚动到底按页追加。
 * 两种布局只启用当前布局的查询。
 */
export function useSellableListData(input: {
    query: MasterDataListQuery
    layout: SellableListLayout
    pagination: PaginationState
}) {
    const gallery = input.layout === "gallery"
    const pageQuery = useSellablePageQuery(
        {
            ...input.query,
            page: input.pagination.pageIndex + 1,
            pageSize: input.pagination.pageSize,
        },
        !gallery,
    )
    const galleryQuery = useSellableInfiniteQuery(
        input.query,
        SELLABLE_GALLERY_BATCH_SIZE,
        gallery,
    )
    // DataTable 需要可变数组。
    const rows = React.useMemo<MasterDataListItem[]>(
        () =>
            gallery
                ? (galleryQuery.data?.pages.flatMap((page) => page.rows) ?? [])
                : [...(pageQuery.data?.rows ?? [])],
        [gallery, galleryQuery.data, pageQuery.data],
    )
    const total = gallery
        ? (galleryQuery.data?.pages[0]?.total ?? 0)
        : (pageQuery.data?.total ?? 0)
    const active = gallery ? galleryQuery : pageQuery
    return {
        rows,
        total,
        loaded: active.data !== undefined,
        isFetching: gallery
            ? galleryQuery.isFetching && !galleryQuery.isFetchingNextPage
            : pageQuery.isFetching,
        isError: active.isError,
        error: active.error,
        refetch: () => void active.refetch(),
        hasMore: gallery && galleryQuery.hasNextPage,
        loadingMore: gallery && galleryQuery.isFetchingNextPage,
        loadMore: () => void galleryQuery.fetchNextPage(),
    }
}
