"use client"

import * as React from "react"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { listWorkspaceEmptyStateClassName } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { MasterDataClearFiltersButton } from "@/features/master-data/components/list/list-empty-actions"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { Skeleton } from "@/components/ui/skeleton"
import { Spinner } from "@/components/ui/spinner"
import { SellableGalleryCard } from "@/features/master-data/components/list/sellable-gallery-card"
import type { MasterDataListItem } from "@/features/master-data/types"

const galleryGridClassName =
    "grid grid-cols-1 gap-3 min-[28rem]:grid-cols-2 sm:grid-cols-3 xl:grid-cols-4 min-[90rem]:grid-cols-5"

function GallerySkeletons() {
    return (
        <div
            className={`${galleryGridClassName} px-(--scaffold-px) pt-2`}
            aria-busy="true"
        >
            {Array.from({ length: 8 }, (_, index) => (
                <Skeleton
                    key={index}
                    className="aspect-[3/4] w-full rounded-xl"
                />
            ))}
        </div>
    )
}

export function SellableItemsGallery({
    rows,
    total,
    hasMore,
    loadingMore,
    onLoadMore,
    loading,
    failed,
    error,
    hasActiveFilters,
    canSelect,
    selectionDisabled,
    selectedIds,
    highlightedId,
    lastFocusedRowId,
    onRetry,
    onClearFilters,
    onToggle,
    onPreview,
}: {
    rows: readonly MasterDataListItem[]
    /** 符合条件的总件数。 */
    total: number
    /** 服务端还有未加载的页。 */
    hasMore: boolean
    loadingMore: boolean
    onLoadMore: () => void
    loading: boolean
    failed: boolean
    error: unknown
    hasActiveFilters: boolean
    canSelect: boolean
    selectionDisabled: boolean
    selectedIds: ReadonlySet<string>
    highlightedId?: string
    lastFocusedRowId: { current: string | null }
    onRetry: () => void
    onClearFilters: () => void
    onToggle: (id: string, selected: boolean) => void
    onPreview: (row: MasterDataListItem) => void
}) {
    const scrollRef = React.useRef<HTMLDivElement>(null)
    const sentinelRef = React.useRef<HTMLDivElement | null>(null)
    const revealMore = React.useCallback(() => {
        if (!loadingMore) onLoadMore()
    }, [loadingMore, onLoadMore])

    React.useEffect(() => {
        if (!hasMore) return
        const element = sentinelRef.current
        if (!element) return
        const observer = new IntersectionObserver(
            (entries) => {
                if (entries.some((entry) => entry.isIntersecting)) revealMore()
            },
            { root: scrollRef.current, rootMargin: "320px 0px" },
        )
        observer.observe(element)
        return () => observer.disconnect()
    }, [hasMore, revealMore, rows.length])

    if (failed) {
        return (
            <BusinessFailureState
                error={error}
                action={
                    <Button
                        id="master-data-sellable-items-gallery-retry"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                }
            />
        )
    }

    if (!loading && rows.length === 0) {
        return (
            <BusinessEmptyState
                kind={hasActiveFilters ? "filter" : "no-data"}
                className={listWorkspaceEmptyStateClassName}
                title={
                    hasActiveFilters ? "当前筛选无结果" : "还没有可销售的 SKU"
                }
                description={
                    hasActiveFilters
                        ? masterDataCopy.filterEmptyDescription
                        : "商品需要已上架、资料有效且存在有效供给，才会出现在这里。"
                }
                action={
                    hasActiveFilters ? (
                        <MasterDataClearFiltersButton
                            id="master-data-sellable-items-gallery-empty-clear-filters"
                            onClick={onClearFilters}
                        />
                    ) : undefined
                }
            />
        )
    }

    if (loading && rows.length === 0) {
        return <GallerySkeletons />
    }

    return (
        <div
            ref={scrollRef}
            className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto px-(--scaffold-px) pt-2"
            data-slot="table-container"
        >
            {loading ? (
                <p
                    role="status"
                    className="flex items-center gap-2 bg-surface-sunken px-2 py-1 text-xs text-muted-foreground"
                >
                    <Spinner />
                    正在刷新，当前内容会保留
                </p>
            ) : null}
            <div
                id="master-data-sellable-items-gallery"
                className={galleryGridClassName}
            >
                {rows.map((row) => (
                    <SellableGalleryCard
                        key={row.stableId}
                        row={row}
                        canSelect={canSelect}
                        selectionDisabled={selectionDisabled}
                        selected={canSelect && selectedIds.has(row.stableId)}
                        highlighted={highlightedId === row.stableId}
                        onToggle={(selected) =>
                            onToggle(row.stableId, selected)
                        }
                        onPreview={() => {
                            lastFocusedRowId.current = row.stableId
                            onPreview(row)
                        }}
                    />
                ))}
            </div>
            {hasMore ? (
                <div ref={sentinelRef} className="flex justify-center pb-4">
                    <Button
                        id="master-data-sellable-items-gallery-load-more"
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={loadingMore}
                        onClick={revealMore}
                    >
                        {loadingMore ? "加载中…" : "加载更多"}
                    </Button>
                </div>
            ) : rows.length > 0 ? (
                <p className="pb-4 text-center text-xs text-muted-foreground">
                    已显示全部 <span className="num">{total}</span> 件
                </p>
            ) : null}
        </div>
    )
}
