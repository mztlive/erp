"use client"

import * as React from "react"
import {
    CircleDollarSignIcon,
    DownloadIcon,
    EyeIcon,
    ShieldCheckIcon,
} from "lucide-react"
import type { SortingState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
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
        <ListPageFrame
            title="公司商品池"
            hint={masterDataCopy.sellableItemsHint}
            headerDensity="default"
            metadata={
                <div className="flex flex-wrap gap-x-4 gap-y-1">
                    <span className="inline-flex items-center gap-1">
                        <EyeIcon aria-hidden="true" />
                        只读查询
                    </span>
                    <span className="inline-flex items-center gap-1">
                        <CircleDollarSignIcon aria-hidden="true" />
                        销售可见口径
                    </span>
                    <span className="inline-flex items-center gap-1">
                        <ShieldCheckIcon aria-hidden="true" />
                        采购成本受保护
                    </span>
                </div>
            }
            exportMeta={state.exportMeta}
            actions={[
                {
                    actionKey: "export",
                    label: "导出当前结果",
                    icon: DownloadIcon,
                    variant: "outline",
                    mobileVisibility: "hide",
                    disabled: state.rows.length === 0,
                    onClick: state.onExport,
                },
            ]}
            resultsLabel={`公司商品池 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        可售商品
                        <span className="font-normal text-muted-foreground">
                            {state.rows.length} 条
                        </span>
                    </span>
                }
                description={state.sellableTableDescription}
                toolbar={
                    <SellableListToolbar
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
                }
                table={
                    <DataTable
                        // 全量结果交给表格：排序必须作用于整份结果，不能只排当前页
                        data={state.rows}
                        columns={columns}
                        getRowId={(row) => row.stableId}
                        // 行的可读名称用业务名，不要回退成「第 N 行」
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
                        layout="flush"
                        density="comfortable"
                        defaultColumnPinning={{
                            left: ["name"],
                            right: [],
                        }}
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    error={state.listQuery.error}
                                    onRetry={() =>
                                        void state.listQuery.refetch()
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
                }
            />
            <SellablePreviewSheet
                previewRow={state.previewRow}
                lastFocusedRowId={lastFocusedRowId}
                onClose={() => state.setPreviewId(null)}
            />
        </ListPageFrame>
    )
}
