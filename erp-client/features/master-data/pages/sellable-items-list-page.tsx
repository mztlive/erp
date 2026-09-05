"use client"

import * as React from "react"
import {
    CircleDollarSignIcon,
    DownloadIcon,
    EyeIcon,
    ShieldCheckIcon,
} from "lucide-react"
import { useIsMutating } from "@tanstack/react-query"
import type { SortingState } from "@tanstack/react-table"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    PageHeader,
    PageHeaderMeta,
    PageHeaderMetaItem,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { SellableListToolbar } from "@/features/master-data/components/list/sellable-list-toolbar"
import { SellablePreviewSheet } from "@/features/master-data/components/list/sellable-preview-sheet"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useSellableListColumns } from "@/features/master-data/hooks/use-sellable-list-columns"
import { useSellableListState } from "@/features/master-data/hooks/use-sellable-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"

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
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="公司商品池"
                description={masterDataCopy.sellableItemsHint}
                metadata={
                    <PageHeaderMeta>
                        <PageHeaderMetaItem>
                            <EyeIcon aria-hidden="true" />
                            只读查询
                        </PageHeaderMetaItem>
                        <PageHeaderMetaItem>
                            <CircleDollarSignIcon aria-hidden="true" />
                            销售可见口径
                        </PageHeaderMetaItem>
                        <PageHeaderMetaItem>
                            <ShieldCheckIcon aria-hidden="true" />
                            采购成本受保护
                        </PageHeaderMetaItem>
                    </PageHeaderMeta>
                }
                actions={
                    <div className="flex items-center gap-2">
                        <Button
                            id="master-data-sellable-items-list-export"
                            type="button"
                            variant="outline"
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
                }
            />

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
                ref={resultsHeadingRef}
                tabIndex={-1}
                className="sr-only outline-none"
            >
                {`公司商品池 · ${state.rows.length} 条结果`}
            </h2>

            <div className="overflow-hidden rounded-xl border border-border/80 bg-card shadow-2xs transition-all">
                {/* 1. 快捷状态分段筛选条：高反差精密轨道，彻底告别低透明度发虚 */}
                <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/80 bg-muted/35 px-4 py-2.5 sm:px-5">
                    <div className="inline-flex items-center gap-1 rounded-lg border border-border/80 bg-surface-sunken p-1 text-xs shadow-2xs">
                        <button
                            type="button"
                            id="master-data-sellable-preset-all"
                            onClick={() => filters.applySupplyPreset("all")}
                            className={cn(
                                "inline-flex items-center gap-2 rounded-md px-3 py-1.5 font-medium transition-all",
                                (filters.supplyPreset ?? "all") === "all"
                                    ? "bg-card text-foreground font-semibold shadow-xs border border-border/80 ring-1 ring-black/5 dark:ring-white/10"
                                    : "text-muted-foreground hover:text-foreground hover:bg-card/50",
                            )}
                        >
                            全部商品
                            <span
                                className={cn(
                                    "rounded-full px-1.5 py-0.2 text-[10px] tabular-nums font-semibold transition-colors",
                                    (filters.supplyPreset ?? "all") === "all"
                                        ? "bg-foreground text-background"
                                        : "bg-muted-foreground/15 text-foreground/75",
                                )}
                            >
                                {state.supplyPresetCounts.all}
                            </span>
                        </button>
                        <button
                            type="button"
                            id="master-data-sellable-preset-single-supplier"
                            onClick={() =>
                                filters.applySupplyPreset("single-supplier")
                            }
                            className={cn(
                                "inline-flex items-center gap-2 rounded-md px-3 py-1.5 font-medium transition-all",
                                filters.supplyPreset === "single-supplier"
                                    ? "bg-card text-foreground font-semibold shadow-xs border border-border/80 ring-1 ring-black/5 dark:ring-white/10"
                                    : "text-muted-foreground hover:text-foreground hover:bg-card/50",
                            )}
                        >
                            单一供应商
                            <span
                                className={cn(
                                    "rounded-full px-1.5 py-0.2 text-[10px] tabular-nums font-semibold transition-colors",
                                    filters.supplyPreset === "single-supplier"
                                        ? "bg-foreground text-background"
                                        : "bg-muted-foreground/15 text-foreground/75",
                                )}
                            >
                                {state.supplyPresetCounts["single-supplier"]}
                            </span>
                        </button>
                        <button
                            type="button"
                            id="master-data-sellable-preset-nationwide"
                            onClick={() =>
                                filters.applySupplyPreset("nationwide")
                            }
                            className={cn(
                                "inline-flex items-center gap-2 rounded-md px-3 py-1.5 font-medium transition-all",
                                filters.supplyPreset === "nationwide"
                                    ? "bg-card text-foreground font-semibold shadow-xs border border-border/80 ring-1 ring-black/5 dark:ring-white/10"
                                    : "text-muted-foreground hover:text-foreground hover:bg-card/50",
                            )}
                        >
                            全国可供
                            <span
                                className={cn(
                                    "rounded-full px-1.5 py-0.2 text-[10px] tabular-nums font-semibold transition-colors",
                                    filters.supplyPreset === "nationwide"
                                        ? "bg-foreground text-background"
                                        : "bg-muted-foreground/15 text-foreground/75",
                                )}
                            >
                                {state.supplyPresetCounts.nationwide}
                            </span>
                        </button>
                    </div>
                    <div className="text-xs text-muted-foreground font-medium">
                        {state.appliedChips.length > 0
                            ? `已生效筛选：${state.appliedChips.map((c) => c.label).join("、")}`
                            : "销售可见口径 · 采购成本受保护 · 点击整行抽屉预览"}
                    </div>
                </div>

                {/* 2. 内嵌工具栏（搜索框已收敛比例，筛选成组紧邻） */}
                <div className="px-4 py-3 sm:px-5">
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
                        productCategoryIdDraft={filters.productCategoryIdDraft}
                        setProductCategoryIdDraft={
                            filters.setProductCategoryIdDraft
                        }
                        productBrandIdDraft={filters.productBrandIdDraft}
                        setProductBrandIdDraft={filters.setProductBrandIdDraft}
                        productSupplierIdDraft={filters.productSupplierIdDraft}
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
                        productSalesPriceError={filters.productSalesPriceError}
                        setProductSalesPriceError={
                            filters.setProductSalesPriceError
                        }
                        productFilterOptionsQuery={
                            state.productFilterOptionsQuery
                        }
                    />
                </div>

                {/* 3. 核心数据表（无缝嵌入，去除了多余的二次标题卡片） */}
                <div className="border-t border-border/70 [&_[data-slot=data-table]]:gap-0 [&_[data-slot=data-table-surface]]:rounded-none [&_[data-slot=data-table-surface]]:border-0 [&_[data-slot=data-table-pagination]]:border-t [&_[data-slot=data-table-pagination]]:border-border/70 [&_[data-slot=data-table-pagination]]:bg-muted/10 [&_[data-slot=data-table-pagination]]:px-4 [&_[data-slot=data-table-pagination]]:py-2.5 sm:[&_[data-slot=data-table-pagination]]:px-5">
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
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
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
            </div>

            <SellablePreviewSheet
                idPrefix="master-data-sellable-items-preview"
                previewRow={state.previewRow}
                lastFocusedRowId={lastFocusedRowId}
                onClose={() => state.setPreviewId(null)}
            />
        </PageScaffold>
    )
}
