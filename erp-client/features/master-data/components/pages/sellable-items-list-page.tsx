"use client"

import { DownloadIcon } from "lucide-react"

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
    const hasActiveFilters =
        filters.q.trim() !== "" || filters.hasStructuredSellableFilters
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data

    return (
        <ListPageFrame
            title={masterDataCopy.pageTitle("可售商品池")}
            currentLabel="可售商品池"
            hint={masterDataCopy.sellableItemsHint}
            exportMeta={state.exportMeta}
            queriedAt={state.listQuery.data?.queriedAt}
            actions={[
                {
                    actionKey: "export",
                    label: masterDataCopy.actionExport,
                    icon: DownloadIcon,
                    variant: "outline",
                    mobileVisibility: "hide",
                    disabled: state.rows.length === 0,
                    onClick: state.onExport,
                },
            ]}
            resultsLabel={`可售商品池 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <BusinessTableFrame
                title="可售商品池列表"
                description={
                    state.sellableTableDescription ??
                    masterDataCopy.sellableListDescription(state.rows.length)
                }
                toolbar={
                    <SellableListToolbar
                        searchInputRef={searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        rowCount={state.rows.length}
                        hasActiveFilters={hasActiveFilters}
                        clearAllFilters={filters.clearAllFilters}
                        sellableFilterPanelOpen={filters.sellableFilterPanelOpen}
                        setSellableFilterPanelOpen={
                            filters.setSellableFilterPanelOpen
                        }
                        hasStructuredSellableFilters={
                            filters.hasStructuredSellableFilters
                        }
                        applySellableFilters={filters.applySellableFilters}
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
                        data={state.pageRows}
                        columns={columns}
                        getRowId={(row) => row.stableId}
                        rowCount={state.rows.length}
                        pagination={filters.pagination}
                        onPaginationChange={filters.changePagination}
                        layout="flush"
                        density="compact"
                        defaultColumnPinning={{ left: ["name"], right: [] }}
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    error={state.listQuery.error}
                                    onRetry={() => void state.listQuery.refetch()}
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
                                            : "还没有可售商品池资料"
                                    }
                                    description={
                                        hasActiveFilters
                                            ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                            : "当前资格日期下没有可销售的公司 SKU。请确认商品已上架、资料有效且存在有效供给。"
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={filters.clearAllFilters}
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
                        onRowOpen={(row) => {
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
