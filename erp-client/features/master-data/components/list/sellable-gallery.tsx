"use client"

import * as React from "react"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { listWorkspaceEmptyStateClassName } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { SellableGalleryCard } from "@/features/master-data/components/list/sellable-gallery-card"
import { SELLABLE_GALLERY_BATCH_SIZE } from "@/features/master-data/lib/sellable-list-layout"
import type { MasterDataListItem } from "@/features/master-data/types"

const galleryGridClassName =
    "grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4 min-[90rem]:grid-cols-5"

function GallerySkeletons() {
    return (
        <div className={galleryGridClassName}>
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
    loading,
    failed,
    error,
    hasActiveFilters,
    selectedIds,
    highlightedId,
    lastFocusedRowId,
    onRetry,
    onClearFilters,
    onToggle,
    onPreview,
}: {
    rows: readonly MasterDataListItem[]
    loading: boolean
    failed: boolean
    error: unknown
    hasActiveFilters: boolean
    selectedIds: ReadonlySet<string>
    highlightedId?: string
    lastFocusedRowId: { current: string | null }
    onRetry: () => void
    onClearFilters: () => void
    onToggle: (id: string, selected: boolean) => void
    onPreview: (row: MasterDataListItem) => void
}) {
    const [visibleCount, setVisibleCount] = React.useState(
        SELLABLE_GALLERY_BATCH_SIZE,
    )
    const sentinelRef = React.useRef<HTMLDivElement | null>(null)

    React.useEffect(() => {
        setVisibleCount(SELLABLE_GALLERY_BATCH_SIZE)
    }, [rows])

    const visibleRows = rows.slice(0, visibleCount)
    const hasMore = visibleCount < rows.length
    const revealMore = React.useCallback(() => {
        setVisibleCount((current) =>
            Math.min(rows.length, current + SELLABLE_GALLERY_BATCH_SIZE),
        )
    }, [rows.length])

    React.useEffect(() => {
        if (!hasMore) return
        const element = sentinelRef.current
        if (!element) return
        const observer = new IntersectionObserver(
            (entries) => {
                if (entries.some((entry) => entry.isIntersecting)) revealMore()
            },
            { rootMargin: "320px 0px" },
        )
        observer.observe(element)
        return () => observer.disconnect()
    }, [hasMore, revealMore, visibleRows.length])

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
                        ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                        : "商品需要已上架、资料有效且存在有效供给，才会出现在这里。"
                }
                action={
                    hasActiveFilters ? (
                        <Button
                            id="master-data-sellable-items-gallery-empty-clear-filters"
                            type="button"
                            variant="secondary"
                            size="sm"
                            className="rounded-lg shadow-none"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : undefined
                }
            />
        )
    }

    if (loading && rows.length === 0) {
        return <GallerySkeletons />
    }

    return (
        <div className="flex flex-col gap-4">
            <div
                id="master-data-sellable-items-gallery"
                className={galleryGridClassName}
            >
                {visibleRows.map((row) => (
                    <SellableGalleryCard
                        key={row.stableId}
                        row={row}
                        selected={selectedIds.has(row.stableId)}
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
                        onClick={revealMore}
                    >
                        加载更多
                    </Button>
                </div>
            ) : rows.length > 0 ? (
                <p className="pb-4 text-center text-xs text-muted-foreground">
                    已显示全部 {rows.length} 件
                </p>
            ) : null}
        </div>
    )
}
