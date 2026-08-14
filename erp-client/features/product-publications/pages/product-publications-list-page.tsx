"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import type { ColumnPinningState, PaginationState } from "@tanstack/react-table"
import { BanIcon, PlusIcon, RefreshCwIcon } from "lucide-react"

import {
    BusinessTableFrame,
    DataFreshness,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { PublicationListTable } from "@/features/product-publications/components/publication-list-table"
import { PublicationListToolbar } from "@/features/product-publications/components/publication-list-toolbar"
import { PublicationMetricStrip } from "@/features/product-publications/components/publication-metric-strip"
import { PublicationPreviewSheet } from "@/features/product-publications/components/publication-preview-sheet"
import { usePublicationListQuery } from "@/features/product-publications/hooks/queries"
import { usePublicationListColumns } from "@/features/product-publications/hooks/use-publication-list-columns"
import { usePublicationListFilters } from "@/features/product-publications/hooks/use-publication-list-filters"
import type { ProductPublicationListQuery } from "@/features/product-publications/types"

export function ProductPublicationsListPage() {
    const router = useRouter()
    const filters = usePublicationListFilters()

    // 预览 Sheet 由本地 state 管理（导航上下文，不写 URL、不随清除筛选变化）
    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const [columnPinning] = React.useState<ColumnPinningState>({
        left: ["sku"],
        right: ["actions"],
    })

    const query: ProductPublicationListQuery = {
        q: filters.qParam || undefined,
        skuId: filters.skuId,
        supplierOfferingRevisionId: filters.supplierOfferingRevisionId,
        mallId: filters.mallId,
        publicationStatus:
            filters.publicationStatus === "all"
                ? undefined
                : filters.publicationStatus,
        deliveryStatus:
            filters.deliveryStatus === "all" ? undefined : filters.deliveryStatus,
        metric: filters.metric === "all" ? undefined : filters.metric,
        page: filters.page,
        pageSize: filters.pageSize,
    }

    const listQuery = usePublicationListQuery(query)
    const data = listQuery.data
    const items = data?.items ?? []
    const pagination: PaginationState = {
        pageIndex: filters.page - 1,
        pageSize: filters.pageSize,
    }

    const columns = usePublicationListColumns(setPreviewId)
    const metrics = data?.metrics

    const toggleMetric = (metricKey: string) => {
        filters.replaceParams({
            metric: filters.metric === metricKey ? undefined : metricKey,
            deliveryStatus: undefined,
            publicationStatus: undefined,
        })
    }

    return (
        <PageScaffold>
            <PageHeader
                title="商品发布"
                breadcrumbs={[
                    {
                        id: "com",
                        label: "商城与发布",
                        href: "/commerce/publications",
                    },
                    { id: "pub", label: "商品发布", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt="列表"
                        dateTime={data?.queriedAt}
                        state={listQuery.isFetching ? "syncing" : "fresh"}
                        label="发布列表"
                    />
                }
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => void listQuery.refetch()}
                        >
                            <RefreshCwIcon />
                            刷新
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            disabled
                            title={data?.creationBlocker.message}
                        >
                            <BanIcon />
                            新建发布
                        </Button>
                    </div>
                }
            />

            {data?.creationBlocker ? (
                <Alert variant="warning">
                    <PlusIcon />
                    <AlertTitle>新建已阻断</AlertTitle>
                    <AlertDescription>
                        {data.creationBlocker.message}
                    </AlertDescription>
                </Alert>
            ) : null}

            <PublicationMetricStrip
                metrics={metrics}
                metric={filters.metric}
                onToggle={toggleMetric}
            />

            <BusinessTableFrame
                title="发布列表"
                description="管理各 SKU 在目标商城的发布版本与发送确认状态。"
                toolbar={
                    <PublicationListToolbar
                        searchInput={filters.searchInput}
                        searchInputRef={filters.searchInputRef}
                        onSearchInputChange={filters.setSearchInput}
                        onSearchCommit={filters.commitSearch}
                        mallId={filters.mallId}
                        publicationStatus={filters.publicationStatus}
                        deliveryStatus={filters.deliveryStatus}
                        skuId={filters.skuId}
                        supplierOfferingRevisionId={
                            filters.supplierOfferingRevisionId
                        }
                        resolvedSkuCode={data?.resolvedFilters.skuCode}
                        resolvedSupplierName={data?.resolvedFilters.supplierName}
                        filterSummary={data?.filterSummary}
                        hasActiveFilters={filters.hasActiveFilters}
                        onPatch={filters.replaceParams}
                        onClearFilters={filters.clearFilters}
                    />
                }
                table={
                    <PublicationListTable
                        isPending={listQuery.isPending}
                        isError={listQuery.isError}
                        error={listQuery.error}
                        onRetry={() => void listQuery.refetch()}
                        items={items}
                        emptyReason={data?.emptyReason}
                        creationBlockerMessage={data?.creationBlocker.message}
                        onClearFilters={filters.clearFilters}
                        columns={columns}
                        columnPinning={columnPinning}
                        pagination={pagination}
                        onPaginationChange={filters.handlePaginationChange}
                        rowCount={data?.total ?? 0}
                        isFetching={listQuery.isFetching}
                        onRowPreview={(row) => setPreviewId(row.publicationId)}
                        onRowOpen={(row) => {
                            router.push(
                                `/commerce/publications/${encodeURIComponent(row.publicationId)}`,
                            )
                        }}
                    />
                }
            />

            <PublicationPreviewSheet
                previewId={previewId}
                onClose={() => setPreviewId(null)}
            />
        </PageScaffold>
    )
}
