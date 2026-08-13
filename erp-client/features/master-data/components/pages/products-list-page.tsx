"use client"

import { useRouter } from "next/navigation"
import { DownloadIcon, PlusIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { ProductListToolbar } from "@/features/master-data/components/list/product-list-toolbar"
import { ProductSupplyDialog } from "@/features/master-data/components/product/product-supply-dialog"
import { ProductDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useProductListColumns } from "@/features/master-data/hooks/use-product-list-columns"
import { useProductListState } from "@/features/master-data/hooks/use-product-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { RegisterSupplyForSkuDialog } from "@/features/supplier-offerings/offering-dialogs"

export function ProductsListPage() {
    const router = useRouter()
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useProductListState(searchInputRef)
    const { filters } = state
    const columns = useProductListColumns({
        canUpdateProductListing: state.canUpdateProductListing,
        currentSupplySkuIds: state.currentSupplySkuIds,
        lastFocusedRowId,
        productSkusByProduct: state.productSkusByProduct,
        productSkusPending: state.productSkusQuery.isPending,
        productSkusError: state.productSkusQuery.isError,
        productListingPending: state.productListingMutation.isPending,
        productListingProductId:
            state.productListingMutation.variables?.productId,
        rows: state.rows,
        supplierOfferingsPending: state.supplierOfferingsQuery.isPending,
        supplierOfferingsError: state.supplierOfferingsQuery.isError,
        onUpdateProductListing: state.updateProductListing,
        onSupplyProduct: state.setSupplyProduct,
        onDisableTarget: state.setDisableTarget,
    })
    const hasActiveFilters =
        filters.q.trim() !== "" || filters.hasStructuredProductFilters
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data
    const openCreate = () => router.push("/master-data/products/new")
    const openDetail = (stableId: string) => {
        lastFocusedRowId.current = stableId
        router.push(`/master-data/products/${stableId}?section=overview`)
    }

    return (
        <ListPageFrame
            title={masterDataCopy.pageTitle("商品列表")}
            currentLabel="商品列表"
            alerts={
                state.listingError ? (
                    <p className="text-sm text-destructive" role="alert">
                        {state.listingError}
                    </p>
                ) : null
            }
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
                {
                    actionKey: "create",
                    label: masterDataCopy.actionCreate,
                    mobileVisibility: "hide",
                    icon: PlusIcon,
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: openCreate,
                },
            ]}
            metrics={
                <LifecycleMetricStrip
                    metrics={state.syncedMetrics}
                    metricKey={filters.metricKey}
                    ariaLabel="商品列表指标筛选"
                    onChangeLifecycle={filters.changeLifecycle}
                />
            }
            resultsLabel={`商品列表 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <BusinessTableFrame
                title="商品列表"
                description={masterDataCopy.productListDescription(
                    state.rows.length,
                )}
                toolbar={
                    <ProductListToolbar
                        searchInputRef={searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        rowCount={state.rows.length}
                        hasActiveFilters={hasActiveFilters}
                        clearAllFilters={filters.clearAllFilters}
                        productFilterPanelOpen={filters.productFilterPanelOpen}
                        setProductFilterPanelOpen={
                            filters.setProductFilterPanelOpen
                        }
                        hasStructuredProductFilters={
                            filters.hasStructuredProductFilters
                        }
                        applyProductFilters={filters.applyProductFilters}
                        productKindDraft={filters.productKindDraft}
                        setProductKindDraft={filters.setProductKindDraft}
                        lifecycleStatusDraft={filters.lifecycleStatusDraft}
                        setLifecycleStatusDraft={
                            filters.setLifecycleStatusDraft
                        }
                        revisionTimingDraft={filters.revisionTimingDraft}
                        setRevisionTimingDraft={filters.setRevisionTimingDraft}
                        productListingStatusDraft={
                            filters.productListingStatusDraft
                        }
                        setProductListingStatusDraft={
                            filters.setProductListingStatusDraft
                        }
                        productSupplyCoverageDraft={
                            filters.productSupplyCoverageDraft
                        }
                        setProductSupplyCoverageDraft={
                            filters.setProductSupplyCoverageDraft
                        }
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
                        defaultColumnPinning={{
                            left: ["stableNo"],
                            right: ["actions"],
                        }}
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
                                            : "还没有商品列表资料"
                                    }
                                    description={
                                        hasActiveFilters
                                            ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                            : "点击「新建」创建第一份资料；历史记录会随资料保留。"
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
                                        ) : state.canCreate ? (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={openCreate}
                                            >
                                                {masterDataCopy.actionCreate}
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                        onRowPreview={(row) => openDetail(row.stableId)}
                        onRowOpen={(row) => openDetail(row.stableId)}
                    />
                }
            />
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
                    state.setSupplyDialogSku({
                        skuId: sku.skuId,
                        skuCode: sku.skuNo,
                        skuName: sku.skuName || state.supplyProduct.name,
                        specification: sku.specification,
                        baseUnit: sku.baseUnit,
                        productKind: state.supplyProduct.productKind,
                    })
                }}
                onOpenChange={(open) => {
                    if (!open) state.setSupplyProduct(null)
                }}
            />
            {state.supplyDialogSku ? (
                <RegisterSupplyForSkuDialog
                    key={state.supplyDialogSku.skuId}
                    open
                    fixedSku={state.supplyDialogSku}
                    onOpenChange={(open) => {
                        if (!open) state.setSupplyDialogSku(null)
                    }}
                />
            ) : null}
            <ProductDisableDialog
                open={state.disableTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setDisableTarget(null)
                }}
                target={state.disableTarget}
            />
        </ListPageFrame>
    )
}
