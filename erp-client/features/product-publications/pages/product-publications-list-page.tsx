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
import { MALLS } from "@/features/product-publications/api/publications"
import {
    PublicationListToolbar,
    type PublicationAppliedChip,
} from "@/features/product-publications/components/publication-list-toolbar"
import { PublicationListTable } from "@/features/product-publications/components/publication-list-table"
import { PublicationMetricStrip } from "@/features/product-publications/components/publication-metric-strip"
import { PublicationPreviewSheet } from "@/features/product-publications/components/publication-preview-sheet"
import { usePublicationListQuery } from "@/features/product-publications/hooks/queries"
import { usePublicationListColumns } from "@/features/product-publications/hooks/use-publication-list-columns"
import { usePublicationListFilters } from "@/features/product-publications/hooks/use-publication-list-filters"
import {
    PUBLICATION_DELIVERY_STATUS_FILTER_LABELS,
    PUBLICATION_METRIC_FILTER_LABELS,
} from "@/features/product-publications/lib/publication-filter-labels"
import { PUBLICATION_STATUS_LABEL } from "@/features/product-publications/types"
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
            filters.deliveryStatus === "all"
                ? undefined
                : filters.deliveryStatus,
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

    const resolvedSkuCode = data?.resolvedFilters.skuCode
    const resolvedSupplierName = data?.resolvedFilters.supplierName

    // 已生效条件全部显性为 chip：关键词、结构化条件、指标快捷与来源锁定
    const appliedChips: PublicationAppliedChip[] = []
    if (filters.qParam.trim()) {
        appliedChips.push({
            key: "q",
            label: `搜索：${filters.qParam.trim()}`,
        })
    }
    if (filters.mallId) {
        const mallLabel = MALLS.find((mall) => mall.id === filters.mallId)?.name
        appliedChips.push({
            key: "mall",
            label: `目标商城：${mallLabel ?? filters.mallId}`,
        })
    }
    if (filters.publicationStatus !== "all") {
        appliedChips.push({
            key: "publicationStatus",
            label: `发布状态：${PUBLICATION_STATUS_LABEL[filters.publicationStatus]}`,
        })
    }
    if (filters.deliveryStatus !== "all") {
        appliedChips.push({
            key: "deliveryStatus",
            label: `发送状态：${PUBLICATION_DELIVERY_STATUS_FILTER_LABELS[filters.deliveryStatus]}`,
        })
    }
    if (filters.metric !== "all") {
        appliedChips.push({
            key: "metric",
            label: `指标：${PUBLICATION_METRIC_FILTER_LABELS[filters.metric] ?? filters.metric}`,
        })
    }
    if (filters.skuId) {
        appliedChips.push({
            key: "skuId",
            label: `已按 SKU：${resolvedSkuCode ?? filters.skuId}`,
        })
    }
    if (filters.supplierOfferingRevisionId) {
        appliedChips.push({
            key: "supplierOfferingRevisionId",
            label: resolvedSupplierName
                ? `已按固定供给：${resolvedSupplierName}`
                : "已按固定供给",
        })
    }

    // 表头说明：有筛选时展示人可读摘要，无筛选时展示默认操作说明
    const summaryParts: string[] = []
    if (filters.qParam.trim()) {
        summaryParts.push(`搜索“${filters.qParam.trim()}”`)
    }
    if (filters.mallId) {
        const mallLabel = MALLS.find((mall) => mall.id === filters.mallId)?.name
        summaryParts.push(`目标商城：${mallLabel ?? filters.mallId}`)
    }
    if (filters.publicationStatus !== "all") {
        summaryParts.push(
            `发布状态：${PUBLICATION_STATUS_LABEL[filters.publicationStatus]}`,
        )
    }
    if (filters.deliveryStatus !== "all") {
        summaryParts.push(
            `发送状态：${PUBLICATION_DELIVERY_STATUS_FILTER_LABELS[filters.deliveryStatus]}`,
        )
    }
    if (filters.metric !== "all") {
        summaryParts.push(
            `指标：${PUBLICATION_METRIC_FILTER_LABELS[filters.metric] ?? filters.metric}`,
        )
    }
    if (filters.skuId) {
        summaryParts.push(`已按 SKU：${resolvedSkuCode ?? filters.skuId}`)
    }
    if (filters.supplierOfferingRevisionId) {
        summaryParts.push(
            resolvedSupplierName
                ? `已按固定供给：${resolvedSupplierName}`
                : "已按固定供给",
        )
    }
    const tableDescription =
        summaryParts.length > 0
            ? summaryParts.join(" · ")
            : "管理各 SKU 在目标商城的发布版本与发送确认状态。"

    return (
        <PageScaffold>
            <PageHeader
                title="商品发布"
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
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        发布列表
                        <span
                            aria-live="polite"
                            className="font-normal text-muted-foreground"
                        >
                            {data?.total ?? 0} 条
                        </span>
                    </span>
                }
                description={tableDescription}
                toolbar={
                    <PublicationListToolbar
                        searchInputRef={filters.searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        appliedChips={appliedChips}
                        removeFilter={filters.removeFilter}
                        clearAllFilters={filters.clearAllFilters}
                        panelOpen={filters.panelOpen}
                        setPanelOpen={filters.setPanelOpen}
                        hasStructuredFilters={filters.hasStructuredFilters}
                        applyFilters={filters.applyFilters}
                        resetMoreFilters={filters.resetMoreFilters}
                        mallDraft={filters.mallDraft}
                        setMallDraft={filters.setMallDraft}
                        publicationStatusDraft={
                            filters.publicationStatusDraft
                        }
                        setPublicationStatusDraft={
                            filters.setPublicationStatusDraft
                        }
                        deliveryStatusDraft={filters.deliveryStatusDraft}
                        setDeliveryStatusDraft={
                            filters.setDeliveryStatusDraft
                        }
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
                        onClearFilters={filters.clearAllFilters}
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
