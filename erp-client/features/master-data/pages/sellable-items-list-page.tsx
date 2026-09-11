"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { CircleHelpIcon } from "lucide-react"
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
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { sellableItemsListStyles as productStyles } from "./sellable-items-list-styles"
import { SellableExportConfirmDialog } from "@/features/master-data/components/list/sellable-export-confirm-dialog"
import { SellableGallerySelectionBar } from "@/features/master-data/components/list/sellable-gallery-selection-bar"
import { SellableItemsFilterBar } from "@/features/master-data/components/list/sellable-items-filter-bar"
import { SellableItemsGallery } from "@/features/master-data/components/list/sellable-gallery"
import { SellableListStatusActions } from "@/features/master-data/components/list/sellable-layout-toggle"
import { SellablePreviewDialog } from "@/features/master-data/components/list/sellable-preview-dialog"
import { SellablePreviewSheet } from "@/features/master-data/components/list/sellable-preview-sheet"
import { BookCreateDialog } from "@/features/sales-selection/components/book-create-dialog"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useSellableExcelExport } from "@/features/master-data/hooks/use-sellable-excel-export"
import { useSellableGallerySelection } from "@/features/master-data/hooks/use-sellable-gallery-selection"
import { useSellableListColumns } from "@/features/master-data/hooks/use-sellable-list-columns"
import { useSellableListState } from "@/features/master-data/hooks/use-sellable-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { resourceLabel } from "@/features/master-data/lib/data"
import type { SellableListLayout } from "@/features/master-data/lib/sellable-list-layout"

const supplyViews = [
    { value: "all", label: "全部商品" },
    { value: "single-supplier", label: "单一供应商" },
    { value: "nationwide", label: "全国可供" },
] as const

export function SellableItemsListPage() {
    const router = useRouter()
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useSellableListState(searchInputRef)
    const selection = useSellableGallerySelection(state.rows)
    const excelExport = useSellableExcelExport()
    const csvExportPending =
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
    const layout = filters.layout
    const isGallery = layout === "gallery"
    const columns = useSellableListColumns()
    // 排序是视图状态而非筛选：全量结果已在客户端，本地排序不重新请求，也不参与「清除筛选」
    const [sorting, setSorting] = React.useState<SortingState>([])
    const hasActiveFilters =
        filters.q.trim() !== "" ||
        filters.hasStructuredSellableFilters ||
        filters.supplyPreset != null
    const listLoadFailed = state.listQuery.isError
    const exportPending = isGallery ? excelExport.pending : csvExportPending
    const exportMeta = isGallery ? excelExport.exportMeta : state.exportMeta
    const exportDisabled = isGallery
        ? exportPending || selection.selectedCount === 0
        : exportPending || state.rows.length === 0

    const [exportConfirmOpen, setExportConfirmOpen] = React.useState(false)
    const [selectionLaunchOpen, setSelectionLaunchOpen] = React.useState(false)

    const changeLayout = React.useCallback(
        (next: SellableListLayout) => {
            state.setPreviewId(null)
            setExportConfirmOpen(false)
            filters.setLayout(next)
        },
        [filters, state],
    )

    const onGalleryExport = React.useCallback(() => {
        if (selection.selectedCount === 0) return
        setExportConfirmOpen(true)
    }, [selection.selectedCount])

    const onConfirmGalleryExport = React.useCallback(() => {
        void excelExport
            .handleExcelExport({
                query: {
                    resource: "sellable-items",
                    q: filters.q.trim() || undefined,
                    productKind: filters.productKind,
                    productCategoryId: filters.productCategoryId,
                    productBrandId: filters.productBrandId,
                    productSupplierId: filters.productSupplierId,
                    supplyRegion: filters.supplyRegion,
                    productSalesPriceMin: filters.productSalesPriceMin,
                    productSalesPriceMax: filters.productSalesPriceMax,
                    sellableSupplyPreset: filters.supplyPreset,
                },
                selectedIds: selection.selectedIds,
                fallbackRows: state.rows,
                filterSnapshotLabel: state.filterSnapshotLabel,
                fileLabel: resourceLabel("sellable-items"),
            })
            .then((ok) => {
                if (ok) setExportConfirmOpen(false)
            })
    }, [excelExport, filters, selection.selectedIds, state])

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="基础资料"
                title="公司商品池"
                description="查看可售商品、销售价格与供货范围。"
            >
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
                            表格模式点击行查看资料，导出范围与当前筛选一致。选品模式可勾选商品，导出带主图的表格文件。
                        </p>
                    </PopoverContent>
                </Popover>
            </ListWorkspaceHeader>

            {exportMeta ? (
                <BackgroundJobProgress
                    mode="all-or-nothing"
                    status="succeeded"
                    total={exportMeta.rowCount}
                    completed={exportMeta.rowCount}
                    succeeded={exportMeta.rowCount}
                    label={masterDataCopy.exportDone}
                    description={
                        isGallery ? (
                            <>
                                已按勾选导出 {exportMeta.rowCount}{" "}
                                条，单元格含商品主图。任务号{" "}
                                <span className="num">{exportMeta.jobId}</span>
                                。不含无权限查看的敏感信息。
                            </>
                        ) : (
                            <>
                                按当前筛选导出 {exportMeta.rowCount} 条。任务号{" "}
                                <span className="num">{exportMeta.jobId}</span>
                                。不含无权限查看的敏感信息。
                            </>
                        )
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

            <ListWorkSurface
                ariaLabel="可售商品列表"
                views={
                    <ListWorkspaceViews
                        ariaLabel="供应快捷筛选"
                        hint={
                            isGallery
                                ? "勾选后可导出带主图的表格"
                                : "选择商品查看详情"
                        }
                        items={supplyViews.map(({ value, label }) => ({
                            id: `master-data-sellable-preset-${value}`,
                            label,
                            count: state.listQuery.data
                                ? state.supplyPresetCounts[value]
                                : "—",
                            active: (filters.supplyPreset ?? "all") === value,
                            onClick: () => filters.applySupplyPreset(value),
                        }))}
                    />
                }
                toolbar={
                    <SellableItemsFilterBar
                        searchInputRef={searchInputRef}
                        filters={filters}
                        appliedChips={state.appliedChips}
                        filterOptions={state.productFilterOptionsQuery}
                        resultCount={
                            state.listQuery.data ? state.rows.length : undefined
                        }
                        loading={state.listQuery.isFetching}
                        failed={state.listQuery.isError}
                        idleHint={
                            isGallery
                                ? "导出范围为当前勾选商品"
                                : "导出与当前查询结果一致"
                        }
                        statusActions={
                            <div className="flex items-center">
                                <Button
                                    id="master-data-sellable-items-launch-selection"
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className="h-auto rounded-none px-1 text-xs font-normal text-foreground shadow-none hover:bg-transparent"
                                    onClick={() => setSelectionLaunchOpen(true)}
                                >
                                    发起选品
                                </Button>
                                <SellableListStatusActions
                                    layout={layout}
                                    onLayoutChange={changeLayout}
                                    exportPending={exportPending}
                                    exportDisabled={exportDisabled}
                                    exportLabel={
                                        exportPending
                                            ? "导出中…"
                                            : isGallery
                                              ? `${masterDataCopy.sellableExportSelected}${selection.selectedCount > 0 ? ` ${selection.selectedCount}` : ""}`
                                              : "导出当前结果"
                                    }
                                    onExport={
                                        isGallery
                                            ? onGalleryExport
                                            : state.onExport
                                    }
                                />
                            </div>
                        }
                    />
                }
                selectionBar={
                    isGallery ? (
                        <SellableGallerySelectionBar
                            resultCount={state.rows.length}
                            selectedCount={selection.selectedCount}
                            allSelected={selection.allSelected}
                            someSelected={selection.someSelected}
                            onSelectAll={selection.selectAllResults}
                            onClear={selection.clear}
                        />
                    ) : null
                }
                tableClassName={
                    isGallery ? productStyles.gallery : productStyles.table
                }
                table={
                    isGallery ? (
                        <SellableItemsGallery
                            rows={state.rows}
                            loading={state.listQuery.isFetching}
                            failed={listLoadFailed}
                            error={state.listQuery.error}
                            hasActiveFilters={hasActiveFilters}
                            selectedIds={selection.selectedIds}
                            highlightedId={state.previewId ?? undefined}
                            lastFocusedRowId={lastFocusedRowId}
                            onRetry={() => void state.listQuery.refetch()}
                            onClearFilters={filters.clearAllFilters}
                            onToggle={selection.toggle}
                            onPreview={(row) =>
                                state.setPreviewId(row.stableId)
                            }
                        />
                    ) : (
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
                                            hasActiveFilters
                                                ? "filter"
                                                : "no-data"
                                        }
                                        className={
                                            listWorkspaceEmptyStateClassName
                                        }
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
                    )
                }
            />

            <SellableExportConfirmDialog
                open={exportConfirmOpen}
                rows={selection.selectedRows}
                pending={excelExport.pending}
                onOpenChange={setExportConfirmOpen}
                onRemove={(id) => selection.toggle(id, false)}
                onConfirm={onConfirmGalleryExport}
            />

            <BookCreateDialog
                open={selectionLaunchOpen}
                onOpenChange={setSelectionLaunchOpen}
                initialFilterQ={filters.q.trim()}
                initialFilter={{
                    max_supplier_count:
                        filters.supplyPreset === "single-supplier"
                            ? 1
                            : undefined,
                    nationwide_only: filters.supplyPreset === "nationwide",
                    q: filters.q.trim() || undefined,
                    product_kind: filters.productKind,
                    category_id: filters.productCategoryId,
                    brand_id: filters.productBrandId,
                    supplier_id: filters.productSupplierId,
                    supply_region: filters.supplyRegion,
                    sales_price_min: filters.productSalesPriceMin,
                    sales_price_max: filters.productSalesPriceMax,
                }}
                initialSkuIds={isGallery ? [...selection.selectedIds] : []}
                onCreated={(bookId) =>
                    router.push(`/sales/selection/${bookId}`)
                }
            />

            {isGallery ? (
                <SellablePreviewDialog
                    idPrefix="master-data-sellable-items-preview"
                    previewRow={state.previewRow}
                    lastFocusedRowId={lastFocusedRowId}
                    onClose={() => state.setPreviewId(null)}
                />
            ) : (
                <SellablePreviewSheet
                    idPrefix="master-data-sellable-items-preview"
                    previewRow={state.previewRow}
                    lastFocusedRowId={lastFocusedRowId}
                    onClose={() => state.setPreviewId(null)}
                />
            )}
        </PageScaffold>
    )
}
