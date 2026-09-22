"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { DownloadIcon, PlusIcon, UploadIcon } from "lucide-react"
import { useIsMutating } from "@tanstack/react-query"

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
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { ProductListToolbar } from "@/features/master-data/components/list/product-list-toolbar"
import { productsListStyles } from "./products-list-styles"
import { ProductSupplyDialog } from "@/features/master-data/components/product/product-supply-dialog"
import { ProductDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useProductListColumns } from "@/features/master-data/hooks/use-product-list-columns"
import { useProductListState } from "@/features/master-data/hooks/use-product-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { RegisterSupplyForSkuDialog } from "@/features/supplier-offerings/offering-dialogs"
import { ProductImportDialog } from "@/features/master-data/components/product/product-import-dialog"
import { launchNavDelivery } from "@/lib/nav-delivery"
import type { ProductImportJob } from "@/features/master-data/api/product-import"

/** 导入动作按钮与投递落点：动作 id 同时是动画起点。 */
const IMPORT_ACTION_ID = "master-data-products-list-import"
const BACKGROUND_TASKS_HREF = "/governance/background-jobs"

export function ProductsListPage() {
    const router = useRouter()
    const [importOpen, setImportOpen] = React.useState(false)
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useProductListState(searchInputRef)
    const exportPending =
        useIsMutating({
            predicate: (mutation) => {
                const variables = mutation.state.variables
                return (
                    typeof variables === "object" &&
                    variables !== null &&
                    "resource" in variables &&
                    variables.resource === "products" &&
                    !("idempotencyKey" in variables)
                )
            },
        }) > 0
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
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: () => setImportOpen(true),
                },
                {
                    id: "master-data-products-list-export",
                    actionKey: "export",
                    label: exportPending
                        ? "导出中…"
                        : masterDataCopy.actionExport,
                    icon: DownloadIcon,
                    variant: "outline",
                    disabled: exportPending || state.rows.length === 0,
                    onClick: state.onExport,
                },
                {
                    id: "master-data-products-list-create",
                    actionKey: "create",
                    label: masterDataCopy.actionCreate,
                    icon: PlusIcon,
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: openCreate,
                },
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
                        hint="选择商品查看详情"
                        onChangeLifecycle={filters.changeLifecycle}
                    />
                }
                toolbar={
                    <ProductListToolbar
                        idPrefix="master-data-products-list-toolbar"
                        searchInputRef={searchInputRef}
                        filters={filters}
                        appliedChips={state.appliedChips}
                        ownerOptions={state.listQuery.data?.ownerOptions ?? []}
                        procurementOwnerOptions={
                            state.listQuery.data?.procurementOwnerOptions ?? []
                        }
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
                                            ? "当前没有可查看的商品范围"
                                            : hasActiveFilters
                                              ? "当前筛选无结果"
                                              : "还没有商品列表资料"
                                    }
                                    description={
                                        noScope
                                            ? "已授权动作但没有可见商品。请联系管理员配置数据范围，或清除筛选后重试。"
                                            : hasActiveFilters
                                              ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                              : "点击「新建」创建第一份资料；历史记录会随资料保留。"
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <Button
                                                id="master-data-products-list-empty-clear-filters"
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
                                        ) : state.canCreate ? (
                                            <Button
                                                id="master-data-products-list-empty-create"
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
            {state.listingConfirmationDialog}
            {importOpen ? (
                <ProductImportDialog
                    onClose={() => setImportOpen(false)}
                    onSubmitted={onImportSubmitted}
                />
            ) : null}
        </ListPageFrame>
    )
}
