"use client"

import * as React from "react"
import type { FixedSku } from "@/features/supplier-offerings/types"
import { useRouter } from "@/lib/navigation"
import { UploadIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import {
    ListWorkSurface,
    listWorkspaceEmptyStateClassName,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import {
    MasterDataClearFiltersButton,
    MasterDataEmptyCreateButton,
} from "@/features/master-data/components/list/list-empty-actions"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { ProductListSelectionBar } from "@/features/master-data/components/list/product-list-selection-bar"
import { ProductListToolbar } from "@/features/master-data/components/list/product-list-toolbar"
import { productsListStyles } from "./products-list-styles"
import { ProductBatchListingDialog } from "@/features/master-data/components/product/product-batch-listing-dialog"
import { ProductSupplyDialog } from "@/features/master-data/components/product/product-supply-dialog"
import { ProductDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useProductListColumns } from "@/features/master-data/hooks/use-product-list-columns"
import { useProductListSkusQuery } from "@/features/master-data/hooks/queries"
import { useProductListState } from "@/features/master-data/hooks/use-product-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    masterDataCreateAction,
    masterDataExportAction,
} from "@/features/master-data/lib/list-page-actions"
import { RegisterSupplyForSkuDialog } from "@/features/supplier-offerings/offering-dialogs"
import { ProductImportDialog } from "@/features/master-data/components/product/product-import-dialog"
import { launchNavDelivery } from "@/lib/nav-delivery"
import type {
    MasterDataListItem,
    ProductListSkuSummary,
} from "@/features/master-data/types"
import type { ProductImportJob } from "@/features/master-data/api/product-import"

/** 导入动作按钮与投递落点：动作 id 同时是动画起点。 */
const IMPORT_ACTION_ID = "master-data-products-list-import"
const BACKGROUND_TASKS_HREF = "/governance/background-jobs"

function toSupplySkus(
    product: MasterDataListItem,
    skus: readonly ProductListSkuSummary[],
): FixedSku[] {
    return skus.map((sku) => ({
        skuId: sku.skuId,
        skuCode: sku.skuNo,
        skuName: sku.skuName || product.name,
        productName: product.name,
        specification: sku.specification,
        baseUnit: sku.baseUnit,
        productKind: product.productKind,
    }))
}

export function ProductsListPage() {
    const router = useRouter()
    const [supplyDialogFixedSku, setSupplyDialogFixedSku] =
        React.useState<FixedSku>()
    const [supplyDialogProduct, setSupplyDialogProduct] =
        React.useState<MasterDataListItem | null>(null)
    const [importOpen, setImportOpen] = React.useState(false)
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useProductListState(searchInputRef)
    const supplyOptionsQuery = useProductListSkusQuery(
        state.canAddSupply && supplyDialogProduct
            ? [supplyDialogProduct.stableId]
            : [],
        state.canReadUnits,
    )
    const exportPending = state.exportPending
    const { filters } = state
    const openSupply = (
        product: MasterDataListItem,
        skus: readonly ProductListSkuSummary[],
    ) => {
        const options = toSupplySkus(product, skus)
        setSupplyDialogFixedSku(options.length === 1 ? options[0] : undefined)
        setSupplyDialogProduct(product)
        state.setSupplyDialogSkus(options)
    }
    const columns = useProductListColumns({
        canUpdateProductListing: state.canUpdateProductListing,
        canReadSkus: state.canReadSkus,
        canReadOfferings: state.canReadOfferings,
        currentSupplySkuIds: state.currentSupplySkuIds,
        lastFocusedRowId,
        productSkusByProduct: state.productSkusByProduct,
        productSkusPending: state.productSkusQuery.isPending,
        productSkusError: state.productSkusQuery.isError,
        productListingPending: state.productListingMutation.isPending,
        productListingProductId:
            state.productListingMutation.variables?.productId,
        batchListingPending: state.batchProductListingMutation.isPending,
        rows: state.rows,
        supplierOfferingsPending: state.supplierOfferingsQuery.isPending,
        supplierOfferingsError: state.supplierOfferingsQuery.isError,
        onUpdateProductListing: state.updateProductListing,
        onSupplyProduct: state.setSupplyProduct,
        onSourceDiff: state.canReadProductDetail
            ? (product) => {
                  lastFocusedRowId.current = product.stableId
                  router.push(
                      `/master-data/products/${encodeURIComponent(product.stableId)}?sourceDiff=true`,
                  )
              }
            : undefined,
        onAddSupply: state.canAddSupply
            ? (product) => {
                  state.setSupplyProduct(null)
                  openSupply(
                      product,
                      state.productSkusByProduct.get(product.stableId) ?? [],
                  )
              }
            : undefined,
        onDisableTarget: state.canUpdateProductListing
            ? state.setDisableTarget
            : undefined,
    })
    const hasActiveFilters =
        filters.q.trim() !== "" || filters.hasStructuredProductFilters
    const noScope = state.listQuery.data?.emptyReason === "no_scope"
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data
    const openCreate = () => router.push("/master-data/products/new")
    const openDetail = (stableId: string) => {
        lastFocusedRowId.current = stableId
        router.push(`/master-data/products/${stableId}?section=overview`)
    }
    // 导入只做任务投递：本页不留进度，用曲线把任务飞到侧栏「后台任务」，结果由 toast 交代。
    const onImportSubmitted = (job: ProductImportJob) => {
        launchNavDelivery(
            "background-task",
            document.getElementById(IMPORT_ACTION_ID),
        )
        toast.add({
            title: "导入任务已提交",
            description: `「${job.file_name ?? "所选文件"}」共 ${job.total_count} 行，正在后台逐行导入，进度与结果在「后台任务」查看。`,
            type: "success",
            timeout: 6000,
            actionProps: {
                children: "查看任务",
                onClick: () => router.push(BACKGROUND_TASKS_HREF),
            },
        })
    }

    return (
        <ListPageFrame
            resource="products"
            title="商品列表"
            description="查看商品资料、上架状态与供应覆盖。"
            alerts={
                state.listingError ? (
                    <p className="text-sm text-destructive" role="alert">
                        {state.listingError}
                    </p>
                ) : null
            }
            exportMeta={state.exportMeta}
            actions={[
                {
                    id: IMPORT_ACTION_ID,
                    actionKey: "import",
                    label: "导入",
                    icon: UploadIcon,
                    variant: "outline",
                    hidden: !state.canCreate,
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: () => setImportOpen(true),
                },
                masterDataExportAction({
                    id: "master-data-products-list-export",
                    pending: exportPending,
                    rowCount: state.rows.length,
                    onClick: state.onExport,
                }),
                masterDataCreateAction({
                    id: "master-data-products-list-create",
                    canCreate: state.canCreate,
                    blockedReason: state.createBlockedReason,
                    onClick: openCreate,
                }),
            ]}
            resultsLabel={`商品列表 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <ListWorkSurface
                ariaLabel="商品列表"
                views={
                    <LifecycleMetricStrip
                        idPrefix="master-data-products-list-metrics"
                        metrics={state.syncedMetrics}
                        metricKey={filters.metricKey}
                        ariaLabel="商品列表指标筛选"
                        allLabel="全部商品"
                        hint={
                            state.canReadProductDetail
                                ? "选择商品查看详情"
                                : undefined
                        }
                        onChangeLifecycle={filters.changeLifecycle}
                    />
                }
                toolbar={
                    <ProductListToolbar
                        idPrefix="master-data-products-list-toolbar"
                        searchInputRef={searchInputRef}
                        filters={filters}
                        appliedChips={state.appliedChips}

                        productFilterOptionsQuery={
                            state.productFilterOptionsQuery
                        }
                        resultCount={
                            state.listQuery.data ? state.rows.length : undefined
                        }
                        loading={state.listQuery.isFetching}
                        failed={state.listQuery.isError}
                    />
                }
                selectionBarOpen={
                    state.canUpdateProductListing &&
                    state.selection.selectedCount > 0
                }
                selectionBar={
                    state.canUpdateProductListing ? (
                        <ProductListSelectionBar
                            resultCount={state.rows.length}
                            pageRowCount={state.pageRows.length}
                            selectedCount={state.selection.selectedCount}
                            actionableCount={
                                state.batchListingPartition.actionable.length
                            }
                            skippedCount={
                                state.batchListingPartition.skipped.length
                            }
                            allPageSelected={state.selection.allPageSelected}
                            busy={state.batchProductListingMutation.isPending}
                            onSelectPage={state.selection.selectAllPage}
                            onClear={state.selection.clear}
                            onBatchList={state.openBatchListingDialog}
                        />
                    ) : undefined
                }
                tableClassName={productsListStyles.table}
                table={
                    <DataTable
                        id="master-data-products-list-table"
                        data={state.pageRows}
                        columns={columns}
                        defaultColumnVisibility={{
                            stableNo: false,
                            revisionNo: false,
                            revisionTiming: false,
                            skuNames: false,
                            blocker: false,
                        }}
                        defaultColumnOrder={[
                            "__selection",
                            "name",
                            "maintainer",
                            "skuCount",
                            "skuPriceRange",
                            "supply",
                            "lifecycle",
                            "listing",
                            "actions",
                            "stableNo",
                            "revisionNo",
                            "revisionTiming",
                            "skuNames",
                            "blocker",
                        ]}
                        getRowId={(row) => row.stableId}
                        rowLabel={(row) => `${row.name}（${row.stableNo}）`}
                        enableRowSelection={state.canUpdateProductListing}
                        rowSelection={state.selection.rowSelection}
                        onRowSelectionChange={
                            state.selection.onRowSelectionChange
                        }
                        rowCount={state.rows.length}
                        pagination={filters.pagination}
                        onPaginationChange={filters.changePagination}
                        loading={state.listQuery.isFetching}
                        layout="flush"
                        defaultColumnPinning={{
                            left: ["name"],
                            right: ["actions"],
                        }}
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    error={state.listQuery.error}
                                    action={
                                        <Button
                                            id="master-data-products-list-retry"
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
                                        noScope
                                            ? "no-data"
                                            : hasActiveFilters
                                              ? "filter"
                                              : "no-data"
                                    }
                                    className={listWorkspaceEmptyStateClassName}
                                    title={
                                        noScope
                                            ? "暂无可查看的商品"
                                            : hasActiveFilters
                                              ? "当前筛选无结果"
                                              : "还没有商品列表资料"
                                    }
                                    description={
                                        noScope
                                            ? "您当前尚未分配商品的数据查看权限，请联系管理员开通后再试。"
                                            : hasActiveFilters
                                              ? masterDataCopy.filterEmptyDescription
                                              : state.canCreate
                                                ? "点击「新建」创建第一份资料；历史记录会随资料保留。"
                                                : undefined
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <MasterDataClearFiltersButton
                                                id="master-data-products-list-empty-clear-filters"
                                                onClick={
                                                    filters.clearAllFilters
                                                }
                                            />
                                        ) : state.canCreate ? (
                                            <MasterDataEmptyCreateButton
                                                id="master-data-products-list-empty-create"
                                                onClick={openCreate}
                                            />
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                        onRowPreview={
                            state.canReadProductDetail
                                ? (row) => openDetail(row.stableId)
                                : undefined
                        }
                        onRowOpen={
                            state.canReadProductDetail
                                ? (row) => openDetail(row.stableId)
                                : undefined
                        }
                    />
                }
            />

            {state.canReadOfferings && !state.supplyDialogSkus && (
                <ProductSupplyDialog
                    product={state.supplyProduct}
                    skus={
                        state.supplyProduct
                            ? (state.productSkusByProduct.get(
                                  state.supplyProduct.stableId,
                              ) ?? [])
                            : []
                    }
                    skuLoading={state.productSkusQuery.isPending}
                    skuError={state.productSkusQuery.error}
                    offerings={state.supplierOfferingsQuery.data ?? []}
                    offeringLoading={
                        state.productPageSkuIds.length > 0 &&
                        state.supplierOfferingsQuery.isPending
                    }
                    offeringError={state.supplierOfferingsQuery.error}
                    onRetrySkus={() => void state.productSkusQuery.refetch()}
                    onRetryOfferings={() =>
                        void state.supplierOfferingsQuery.refetch()
                    }
                    onAddSupply={(sku) => {
                        if (!state.supplyProduct) return
                        openSupply(state.supplyProduct, [sku])
                    }}
                    onOpenChange={(open) => {
                        if (!open) state.setSupplyProduct(null)
                    }}
                />
            )}
            {state.canAddSupply && state.supplyDialogSkus ? (
                <RegisterSupplyForSkuDialog
                    open
                    fixedSku={supplyDialogFixedSku}
                    skuOptions={state.supplyDialogSkus}
                    onRefreshSkus={async () => {
                        if (!supplyDialogProduct) return
                        const result = await supplyOptionsQuery.refetch({
                            throwOnError: true,
                        })
                        state.setSupplyDialogSkus(
                            toSupplySkus(
                                supplyDialogProduct,
                                result.data ?? [],
                            ),
                        )
                    }}
                    onOpenChange={(open) => {
                        if (!open) state.setSupplyDialogSkus(null)
                    }}
                />
            ) : null}
            <ProductDisableDialog
                open={
                    state.canUpdateProductListing && state.disableTarget != null
                }
                onOpenChange={(open) => {
                    if (!open) state.setDisableTarget(null)
                }}
                target={state.disableTarget}
            />
            {state.canUpdateProductListing
                ? state.listingConfirmationDialog
                : null}
            {state.canUpdateProductListing ? (
                <ProductBatchListingDialog
                    open={state.batchListingOpen}
                    rows={state.selection.selectedRows}
                    filterSummary={state.filterSnapshotLabel}
                    totalResultCount={state.rows.length}
                    pending={state.batchProductListingMutation.isPending}
                    progress={state.batchListingProgress}
                    result={state.batchListingResult}
                    onOpenChange={(open) => {
                        if (!open) state.closeBatchListingDialog()
                    }}
                    onRemove={state.selection.remove}
                    onConfirm={(actionable, skipped) =>
                        void state.submitBatchListing(actionable, skipped)
                    }
                    onRetryFailed={(failedRows) =>
                        void state.retryFailedBatchListing(failedRows)
                    }
                />
            ) : null}
            {state.canCreate && importOpen ? (
                <ProductImportDialog
                    onClose={() => setImportOpen(false)}
                    onSubmitted={onImportSubmitted}
                />
            ) : null}
        </ListPageFrame>
    )
}
