"use client"

import * as React from "react"
import { CircleHelpIcon, DownloadIcon } from "lucide-react"
import { useIsMutating } from "@tanstack/react-query"
import type { SortingState } from "@tanstack/react-table"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import styles from "./sellable-items-list-page.module.css"
import { SellableListToolbar } from "@/features/master-data/components/list/sellable-list-toolbar"
import { SellablePreviewSheet } from "@/features/master-data/components/list/sellable-preview-sheet"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useSellableListColumns } from "@/features/master-data/hooks/use-sellable-list-columns"
import { useSellableListState } from "@/features/master-data/hooks/use-sellable-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"

const supplyViews = [
    { value: "all", label: "全部商品" },
    { value: "single-supplier", label: "单一供应商" },
    { value: "nationwide", label: "全国可供" },
] as const

export function SellableItemsListPage() {
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useSellableListState(searchInputRef)
    const exportPending =
        useIsMutating({
            predicate: (mutation) => {
                const variables = mutation.state.variables
                return (
                    typeof variables === "object" &&
                    variables !== null &&
                    "resource" in variables &&
                    variables.resource === "sellable-items" &&
                    !("idempotencyKey" in variables)
                )
            },
        }) > 0
    const { filters } = state
    const columns = useSellableListColumns()
    // 排序是视图状态而非筛选：全量结果已在客户端，本地排序不重新请求，也不参与「清除筛选」
    const [sorting, setSorting] = React.useState<SortingState>([])
    const hasActiveFilters =
        filters.q.trim() !== "" ||
        filters.hasStructuredSellableFilters ||
        filters.supplyPreset != null
    const listLoadFailed = state.listQuery.isError

    return (
        <PageScaffold density="compact" className={styles.page}>
            <header className={styles.header}>
                <div>
                    <p className={styles.eyebrow}>基础资料</p>
                    <h1 className={styles.title}>公司商品池</h1>
                    <p className={styles.description}>
                        查看可售商品、销售价格与供货范围。
                    </p>
                </div>
                <div className={styles.headerActions}>
                    <Popover>
                        <PopoverTrigger
                            render={
                                <Button
                                    id="master-data-sellable-items-help"
                                    variant="ghost"
                                    type="button"
                                    className={styles.quietButton}
                                />
                            }
                        >
                            <CircleHelpIcon aria-hidden="true" />
                            查询说明
                        </PopoverTrigger>
                        <PopoverContent align="end" className="rounded-xl p-5">
                            <p className="font-medium">商品池查询说明</p>
                            <p className="text-sm leading-6 text-muted-foreground">
                                {masterDataCopy.sellableItemsHint}
                            </p>
                            <p className="text-sm leading-6 text-muted-foreground">
                                此页面用于查询。点击商品可查看资料，导出范围与当前筛选结果一致。
                            </p>
                        </PopoverContent>
                    </Popover>
                    <Button
                        id="master-data-sellable-items-list-export"
                        type="button"
                        variant="outline"
                        className={styles.exportButton}
                        disabled={exportPending || state.rows.length === 0}
                        onClick={state.onExport}
                    >
                        <DownloadIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {exportPending ? "导出中…" : "导出当前结果"}
                    </Button>
                </div>
            </header>

            {state.exportMeta ? (
                <BackgroundJobProgress
                    mode="all-or-nothing"
                    status="succeeded"
                    total={state.exportMeta.rowCount}
                    completed={state.exportMeta.rowCount}
                    succeeded={state.exportMeta.rowCount}
                    label={masterDataCopy.exportDone}
                    description={
                        <>
                            按当前筛选导出 {state.exportMeta.rowCount}{" "}
                            条。任务号{" "}
                            <span className="num">
                                {state.exportMeta.jobId}
                            </span>
                            。不含无权限查看的敏感信息。
                        </>
                    }
                />
            ) : null}

            <h2
                id="master-data-sellable-items-results"
                ref={resultsHeadingRef}
                tabIndex={-1}
                className="sr-only outline-none"
            >
                {`公司商品池 · ${state.rows.length} 条结果`}
            </h2>

            <section
                className={styles.workSurface}
                data-business-component="table-frame"
                aria-label="可售商品列表"
            >
                <div className={styles.viewBar}>
                    <div
                        className={styles.views}
                        role="group"
                        aria-label="供应快捷筛选"
                    >
                        {supplyViews.map(({ value, label }) => {
                            const active =
                                (filters.supplyPreset ?? "all") === value
                            return (
                                <button
                                    key={value}
                                    id={`master-data-sellable-preset-${value}`}
                                    type="button"
                                    aria-pressed={active}
                                    className={cn(
                                        styles.view,
                                        active && styles.activeView,
                                    )}
                                    onClick={() =>
                                        filters.applySupplyPreset(value)
                                    }
                                >
                                    {label}
                                    <span className={styles.viewCount}>
                                        {state.listQuery.data
                                            ? state.supplyPresetCounts[value]
                                            : "—"}
                                    </span>
                                </button>
                            )
                        })}
                    </div>
                    <span className={styles.viewHint}>选择商品查看详情</span>
                </div>
                <div className={styles.toolbar}>
                    <div className={styles.filters}>
                        <SellableListToolbar
                            idPrefix="master-data-sellable-items-list-toolbar"
                            searchInputRef={searchInputRef}
                            searchDraft={filters.searchDraft}
                            setSearchDraft={filters.setSearchDraft}
                            hasActiveFilters={hasActiveFilters}
                            clearAllFilters={filters.clearAllFilters}
                            appliedChips={state.appliedChips}
                            removeFilter={filters.removeFilter}
                            supplyPreset={filters.supplyPreset ?? "all"}
                            supplyPresetCounts={state.supplyPresetCounts}
                            applySupplyPreset={filters.applySupplyPreset}
                            showSupplyPreset={false}
                            sellableFilterPanelOpen={
                                filters.sellableFilterPanelOpen
                            }
                            setSellableFilterPanelOpen={
                                filters.setSellableFilterPanelOpen
                            }
                            hasStructuredSellableFilters={
                                filters.hasStructuredSellableFilters
                            }
                            applySellableFilters={filters.applySellableFilters}
                            resetMoreFilters={filters.resetMoreFilters}
                            supplyRegionDraft={filters.supplyRegionDraft}
                            setSupplyRegionDraft={filters.setSupplyRegionDraft}
                            productKindDraft={filters.productKindDraft}
                            setProductKindDraft={filters.setProductKindDraft}
                            productCategoryIdDraft={
                                filters.productCategoryIdDraft
                            }
                            setProductCategoryIdDraft={
                                filters.setProductCategoryIdDraft
                            }
                            productBrandIdDraft={filters.productBrandIdDraft}
                            setProductBrandIdDraft={
                                filters.setProductBrandIdDraft
                            }
                            productSupplierIdDraft={
                                filters.productSupplierIdDraft
                            }
                            setProductSupplierIdDraft={
                                filters.setProductSupplierIdDraft
                            }
                            productSalesPriceMinDraft={
                                filters.productSalesPriceMinDraft
                            }
                            setProductSalesPriceMinDraft={
                                filters.setProductSalesPriceMinDraft
                            }
                            productSalesPriceMaxDraft={
                                filters.productSalesPriceMaxDraft
                            }
                            setProductSalesPriceMaxDraft={
                                filters.setProductSalesPriceMaxDraft
                            }
                            productSalesPriceError={
                                filters.productSalesPriceError
                            }
                            setProductSalesPriceError={
                                filters.setProductSalesPriceError
                            }
                            productFilterOptionsQuery={
                                state.productFilterOptionsQuery
                            }
                        />
                    </div>

                    <div
                        className={styles.columnSettings}
                        data-slot="table-frame-view-options"
                    />
                </div>
                <div
                    className={styles.table}
                    data-slot="business-table-frame-table"
                >
                    <DataTable
                        id="master-data-sellable-items-list-table"
                        data={state.rows}
                        columns={columns}
                        getRowId={(row) => row.stableId}
                        rowLabel={(row) =>
                            row.sellableItem
                                ? `${row.name} · ${row.sellableItem.specificationLabel}`
                                : row.name
                        }
                        rowCount={state.rows.length}
                        pagination={filters.pagination}
                        onPaginationChange={filters.changePagination}
                        sorting={sorting}
                        onSortingChange={setSorting}
                        manualSorting={false}
                        manualPagination={false}
                        loading={state.listQuery.isFetching}
                        highlightedRowId={state.previewId ?? undefined}
                        layout="flush"
                        caption="可售商品、销售价格与供应保障"
                        defaultColumnVisibility={{ productNo: false }}
                        defaultColumnPinning={{
                            left: ["name"],
                            right: [],
                        }}
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    error={state.listQuery.error}
                                    action={
                                        <Button
                                            id="master-data-sellable-items-list-retry"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() =>
                                                void state.listQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                        emptyState={
                            !listLoadFailed && state.rows.length === 0 ? (
                                <BusinessEmptyState
                                    kind={
                                        hasActiveFilters ? "filter" : "no-data"
                                    }
                                    className="rounded-none border-0 bg-transparent px-6 py-16 shadow-none ring-0"
                                    title={
                                        hasActiveFilters
                                            ? "当前筛选无结果"
                                            : "还没有可销售的 SKU"
                                    }
                                    description={
                                        hasActiveFilters
                                            ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                            : "商品需要已上架、资料有效且存在有效供给，才会出现在这里。"
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <Button
                                                id="master-data-sellable-items-list-empty-clear-filters"
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={
                                                    filters.clearAllFilters
                                                }
                                            >
                                                清除筛选
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                        onRowPreview={(row) => {
                            lastFocusedRowId.current = row.stableId
                            state.setPreviewId(row.stableId)
                        }}
                    />
                </div>
            </section>

            <SellablePreviewSheet
                idPrefix="master-data-sellable-items-preview"
                previewRow={state.previewRow}
                lastFocusedRowId={lastFocusedRowId}
                onClose={() => state.setPreviewId(null)}
            />
        </PageScaffold>
    )
}
