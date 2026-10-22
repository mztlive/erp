"use client"

import * as React from "react"
import { useRouter } from "@/lib/navigation"
import { CircleHelpIcon, DownloadIcon, PackageSearchIcon } from "lucide-react"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { toast } from "@/components/ui/toast"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { sellableItemsListStyles as productStyles } from "./sellable-items-list-styles"
import { SellableExportConfirmDialog } from "@/features/master-data/components/list/sellable-export-confirm-dialog"
import { SellableGallerySelectionBar } from "@/features/master-data/components/list/sellable-gallery-selection-bar"
import { SellableItemsFilterBar } from "@/features/master-data/components/list/sellable-items-filter-bar"
import { SellableItemsGallery } from "@/features/master-data/components/list/sellable-gallery"
import { SellableLayoutToggle } from "@/features/master-data/components/list/sellable-layout-toggle"
import { SellablePreviewDialog } from "@/features/master-data/components/list/sellable-preview-dialog"
import { SellablePreviewSheet } from "@/features/master-data/components/list/sellable-preview-sheet"
import { useCreatePermission } from "@/features/master-data/hooks/use-create-permission"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useSellableExcelExport } from "@/features/master-data/hooks/use-sellable-excel-export"
import { useSellableGallerySelection } from "@/features/master-data/hooks/use-sellable-gallery-selection"
import { useSellableListColumns } from "@/features/master-data/hooks/use-sellable-list-columns"
import { useSellableListState } from "@/features/master-data/hooks/use-sellable-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { resourceLabel } from "@/features/master-data/lib/data"
import type { SellableListLayout } from "@/features/master-data/lib/sellable-list-layout"
import { BookCreateDialog } from "@/features/sales-selection/components/book-create-dialog"
import { launchNavDelivery } from "@/lib/nav-delivery"
import {
    describePoolSource,
    resolvePoolSourceKind,
} from "@/features/sales-selection/lib/pool-source"

export function SellableItemsListPage() {
    const router = useRouter()
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useSellableListState(searchInputRef)
    const selection = useSellableGallerySelection(
        state.rows,
        state.selectionResetKey,
    )
    const excelExport = useSellableExcelExport()
    const { filters } = state
    const layout = filters.layout
    const isGallery = layout === "gallery"
    const columns = useSellableListColumns()
    const hasActiveFilters =
        filters.q.trim() !== "" || filters.hasStructuredSellableFilters
    const listLoadFailed = state.list.isError
    const exportsSelection = isGallery || selection.selectedCount > 0
    const exportPending = excelExport.pending
    const exportMeta = excelExport.exportMeta
    const exportDisabled = exportsSelection
        ? exportPending || selection.selectedCount === 0
        : exportPending || state.list.total === 0

    const [exportConfirmOpen, setExportConfirmOpen] = React.useState(false)
    const [selectionLaunchOpen, setSelectionLaunchOpen] = React.useState(false)
    const launchButtonRef = React.useRef<HTMLSpanElement>(null)
    const { canCreate: canLaunchSelection } = useCreatePermission(
        "sales_selection_booklet:create",
    )
    const sourceKind = resolvePoolSourceKind(selection.selectedCount)
    const sourceSummary = describePoolSource({
        kind: sourceKind,
        itemCount:
            sourceKind === "SELECTION"
                ? selection.selectedCount
                : state.list.total,
        filterLabel: state.appliedChips.map((chip) => chip.label).join(" · "),
    })
    const launchDisabled =
        listLoadFailed ||
        (sourceKind === "SELECTION"
            ? selection.selectedCount === 0
            : state.list.total === 0)

    const changeLayout = React.useCallback(
        (next: SellableListLayout) => {
            state.setPreviewId(null)
            setExportConfirmOpen(false)
            filters.setLayout(next)
        },
        [filters, state],
    )

    const onSelectionExport = React.useCallback(() => {
        if (selection.selectedCount === 0) return
        setExportConfirmOpen(true)
    }, [selection.selectedCount])

    const onConfirmSelectionExport = React.useCallback(() => {
        void excelExport
            .handleExcelExport({
                query: {
                    resource: "sellable-items",
                    q: filters.q.trim() || undefined,
                    productKind: filters.productKind,
                    productCategoryId: filters.productCategoryId,
                    productBrandId: filters.productBrandId,
                    deliveryRegion: filters.deliveryRegion,
                    deliveryExtent: filters.deliveryExtent,
                    includeFreightExtra: filters.includeFreightExtra,
                    productPriceKind: filters.productPriceKind,
                    productSalesPriceMin: filters.productSalesPriceMin,
                    productSalesPriceMax: filters.productSalesPriceMax,
                    // 只按勾选的商品复核，不重新取回全部结果。
                    skuIds: [...selection.selectedIds],
                },
                selectedIds: selection.selectedIds,
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
                description="查看可售商品、四档含税销售参考价、供应商编号与供货范围。"
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
                            表格和卡片模式均可勾选商品，导出带主图、客户选品和配送地址填写页的
                            Excel，或把已勾选商品做成选品册。表格未勾选时导出当前筛选结果；发起选品时，已勾选则按勾选，未勾选则按当前筛选。
                        </p>
                    </PopoverContent>
                </Popover>
                <LoadingButton
                    id="master-data-sellable-items-list-export"
                    loading={exportPending}
                    type="button"
                    variant="outline"
                    size="sm"
                    className={styles.exportButton}
                    disabled={exportDisabled}
                    onClick={() => {
                        if (exportsSelection) onSelectionExport()
                        else
                            void excelExport.handleExcelExport({
                                query: state.exportQuery,
                                filterSnapshotLabel: state.filterSnapshotLabel,
                                fileLabel: resourceLabel("sellable-items"),
                            })
                    }}
                >
                    {!exportPending ? (
                        <DownloadIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                    ) : null}
                    {exportPending
                        ? "导出中…"
                        : exportsSelection
                          ? `${masterDataCopy.sellableExportSelected}${selection.selectedCount > 0 ? ` ${selection.selectedCount}` : ""}`
                          : "导出当前结果"}
                </LoadingButton>
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
                        <>
                            已导出 {exportMeta.rowCount}{" "}
                            件商品，文件含主图、客户选品及配送地址填写页。
                            发送客户前请自行处理内部信息。任务号{" "}
                            <span className="num">{exportMeta.jobId}</span>。
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
                {`公司商品池 · ${state.list.total} 条结果`}
            </h2>

            <ListWorkSurface
                ariaLabel="可售商品列表"
                toolbar={
                    <SellableItemsFilterBar
                        searchInputRef={searchInputRef}
                        filters={filters}
                        deliveryFilter={filters}
                        appliedChips={state.appliedChips}
                        filterOptions={state.productFilterOptionsQuery}
                        resultCount={
                            state.list.loaded ? state.list.total : undefined
                        }
                        loading={state.list.isFetching}
                        failed={state.list.isError}
                    />
                }
                tableActions={
                    <SellableLayoutToggle
                        layout={layout}
                        onLayoutChange={changeLayout}
                    />
                }
                selectionBar={
                    <SellableGallerySelectionBar
                        layout={layout}
                        actions={
                            canLaunchSelection ? (
                                <span
                                    ref={launchButtonRef}
                                    className="mr-1 inline-flex"
                                >
                                    <Button
                                        id="master-data-sellable-items-launch-selection"
                                        type="button"
                                        variant="default"
                                        size="sm"
                                        disabled={launchDisabled}
                                        title={sourceSummary}
                                        onClick={() =>
                                            setSelectionLaunchOpen(true)
                                        }
                                    >
                                        <PackageSearchIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        {sourceKind === "SELECTION"
                                            ? `发起选品 ${selection.selectedCount}`
                                            : "发起选品"}
                                    </Button>
                                </span>
                            ) : null
                        }
                        idPrefix={
                            isGallery
                                ? "master-data-sellable-items-gallery"
                                : "master-data-sellable-items-table"
                        }
                        resultCount={state.list.total}
                        selectedCount={selection.selectedCount}
                        allSelected={selection.allSelected}
                        someSelected={selection.someSelected}
                        onSelectAll={selection.selectAllResults}
                        onClear={selection.clear}
                    />
                }
                tableClassName={isGallery ? productStyles.gallery : undefined}
                table={
                    isGallery ? (
                        <SellableItemsGallery
                            rows={state.rows}
                            total={state.list.total}
                            hasMore={state.list.hasMore}
                            loadingMore={state.list.loadingMore}
                            onLoadMore={state.list.loadMore}
                            loading={state.list.isFetching}
                            failed={listLoadFailed}
                            error={state.list.error}
                            hasActiveFilters={hasActiveFilters}
                            selectedIds={selection.selectedIds}
                            highlightedId={state.previewId ?? undefined}
                            lastFocusedRowId={lastFocusedRowId}
                            onRetry={state.list.refetch}
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
                            enableRowSelection
                            rowSelection={selection.rowSelection}
                            onRowSelectionChange={
                                selection.onRowSelectionChange
                            }
                            rowLabel={(row) =>
                                row.sellableItem
                                    ? `${row.name} · ${row.sellableItem.specificationLabel}`
                                    : row.name
                            }
                            rowCount={state.list.total}
                            pagination={filters.pagination}
                            onPaginationChange={filters.changePagination}
                            sorting={state.sorting}
                            onSortingChange={state.changeSorting}
                            manualSorting
                            manualPagination
                            loading={state.list.isFetching}
                            highlightedRowId={state.previewId ?? undefined}
                            layout="flush"
                            caption="可售商品、销售参考价与供应保障"
                            defaultColumnVisibility={{ productNo: false }}
                            defaultColumnPinning={{
                                left: ["name"],
                                right: [],
                            }}
                            errorState={
                                listLoadFailed ? (
                                    <BusinessFailureState
                                        error={state.list.error}
                                        action={
                                            <Button
                                                id="master-data-sellable-items-list-retry"
                                                type="button"
                                                variant="outline"
                                                size="sm"
                                                onClick={state.list.refetch}
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
                onConfirm={onConfirmSelectionExport}
            />

            {canLaunchSelection ? (
                <BookCreateDialog
                    open={selectionLaunchOpen}
                    onOpenChange={setSelectionLaunchOpen}
                    sourceKind={sourceKind}
                    sourceSummary={sourceSummary}
                    initialFilter={{
                        q: filters.q.trim() || undefined,
                        product_kind: filters.productKind,
                        category_id: filters.productCategoryId,
                        brand_id: filters.productBrandId,
                        price_kind: filters.productPriceKind,
                        sales_price_min: filters.productSalesPriceMin,
                        sales_price_max: filters.productSalesPriceMax,
                        delivery_region_code: filters.deliveryRegion,
                        delivery_extent: filters.deliveryRegion
                            ? filters.deliveryExtent
                            : undefined,
                        include_freight_extra: filters.deliveryRegion
                            ? filters.includeFreightExtra
                            : undefined,
                    }}
                    initialSkuIds={
                        sourceKind === "SELECTION"
                            ? [...selection.selectedIds]
                            : []
                    }
                    onFlyToNav={() =>
                        launchNavDelivery(
                            "selection-booklet",
                            launchButtonRef.current,
                        )
                    }
                    onLaunched={({ bookId, customerName, prepared }) => {
                        toast.add({
                            title: prepared ? "正在准备选品册" : "选品册已创建",
                            description: prepared
                                ? `正在为「${customerName}」冻结商品并生成陈列。`
                                : `已为「${customerName}」创建选品册，准备未开始，可在选品册继续。`,
                            type: prepared ? "success" : "warning",
                            timeout: 6000,
                            actionProps: {
                                children: "查看这本",
                                onClick: () =>
                                    router.push(`/sales/selection/${bookId}`),
                            },
                        })
                    }}
                />
            ) : null}

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
