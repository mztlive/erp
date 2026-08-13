"use client"

import * as React from "react"
import Link from "next/link"
import { DownloadIcon, PlusIcon } from "lucide-react"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FormalActionResult,
    MetricFilterItem,
    MetricItem,
    MetricStrip,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { resourceLabel } from "@/features/master-data/lib/data"
import { MasterDataListDialogs } from "@/features/master-data/components/list/master-data-list-dialogs"
import { MasterDataListToolbar } from "@/features/master-data/components/list/master-data-list-toolbar"
import { MasterDataPreviewSheet } from "@/features/master-data/components/list/master-data-preview-sheet"
import { useMasterDataListWorkspace } from "@/features/master-data/hooks/use-master-data-list-workspace"
import type { MasterDataResource } from "@/features/master-data/types"
import { hasPermission } from "@/lib/permissions"

export function MasterDataListWorkspace({
    resource,
    searchInputRef,
    resultsHeadingRef,
    lastFocusedRowId,
}: {
    resource: MasterDataResource
    navRef: React.RefObject<HTMLElement | null>
    searchInputRef: React.RefObject<HTMLInputElement | null>
    resultsHeadingRef: React.RefObject<HTMLHeadingElement | null>
    lastFocusedRowId: React.MutableRefObject<string | null>
}) {
    const state = useMasterDataListWorkspace({
        resource,
        searchInputRef,
        resultsHeadingRef,
        lastFocusedRowId,
    })
    const {
        router,
        accountQuery,
        isProductResource,
        isSupplierResource,
        isBrandResource,
        isVoucherCategoryResource,
        isUnitOfMeasureResource,
        isSellableResource,
        isWarehouse,
        skipPreviewSheet,
        canCreate,
        createBlockedReason,
        filters,
        previewId,
        setPreviewId,
        createOpen,
        setCreateOpen,
        reviseTarget,
        setReviseTarget,
        disableTarget,
        setDisableTarget,
        supplyProduct,
        setSupplyProduct,
        supplyDialogSku,
        setSupplyDialogSku,
        exportMeta,
        listingError,
        listQuery,
        productFilterOptionsQuery,
        exportMutation,
        canUpdateProductListing,
        rows,
        previewDetailQuery,
        previewRow,
        pageRows,
        productPageSkuIds,
        productSkusByProduct,
        productSkusQuery,
        supplierOfferingsQuery,
        selectedCategoryLabel,
        selectedBrandLabel,
        selectedSupplierLabel,
        syncedMetrics,
        filterSnapshotLabel,
        sellableTableDescription,
        listTableDescription,
        handleExport,
        columns,
    } = state
    const {
        q,
        lifecycleStatus,
        revisionTiming,
        productKind,
        productCategoryId,
        productBrandId,
        productSupplierId,
        supplyRegion,
        productListingStatus,
        productSupplyCoverage,
        productSalesPriceMin,
        productSalesPriceMax,
        supplierCapabilityCodes,
        supplierQualificationTypes,
        supplierQualificationHealth,
        metricKey,
        hasStructuredProductFilters,
        hasStructuredSellableFilters,
        hasStructuredSupplierFilters,
        hasStructuredListFilters,
        searchDraft,
        setSearchDraft,
        productFilterPanelOpen,
        setProductFilterPanelOpen,
        sellableFilterPanelOpen,
        setSellableFilterPanelOpen,
        supplierFilterPanelOpen,
        setSupplierFilterPanelOpen,
        filterPanelOpen,
        setFilterPanelOpen,
        supplierCapabilityCodesDraft,
        setSupplierCapabilityCodesDraft,
        supplierQualificationTypesDraft,
        setSupplierQualificationTypesDraft,
        supplierQualificationHealthDraft,
        setSupplierQualificationHealthDraft,
        productKindDraft,
        setProductKindDraft,
        lifecycleStatusDraft,
        setLifecycleStatusDraft,
        revisionTimingDraft,
        setRevisionTimingDraft,
        productListingStatusDraft,
        setProductListingStatusDraft,
        productSupplyCoverageDraft,
        setProductSupplyCoverageDraft,
        productCategoryIdDraft,
        setProductCategoryIdDraft,
        productBrandIdDraft,
        setProductBrandIdDraft,
        productSupplierIdDraft,
        setProductSupplierIdDraft,
        productSalesPriceMinDraft,
        setProductSalesPriceMinDraft,
        productSalesPriceMaxDraft,
        setProductSalesPriceMaxDraft,
        supplyRegionDraft,
        setSupplyRegionDraft,
        productSalesPriceError,
        setProductSalesPriceError,
        pagination,
        setPagination,
        patchUrl,
        changeLifecycle,
        commitSearch,
        applySellableFilters,
        applySupplierFilters,
        applyListFilters,
        applyProductFilters,
        clearAllFilters,
    } = filters

    if (listQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <PageHeader
                    title={masterDataCopy.pageTitle(resourceLabel(resource))}
                />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    const listLoadFailed = listQuery.isError || !listQuery.data
    const hasActiveFilters = isSellableResource
        ? q.trim() !== "" || hasStructuredSellableFilters
        : q.trim() !== "" ||
          lifecycleStatus !== "all" ||
          (!isSupplierResource && revisionTiming !== "all") ||
          Boolean(
              supplierQualificationHealth ||
              supplierCapabilityCodes.length ||
              supplierQualificationTypes.length,
          ) ||
          Boolean(
              productKind ||
              productCategoryId ||
              productBrandId ||
              productSupplierId ||
              productListingStatus ||
              productSupplyCoverage ||
              productSalesPriceMin ||
              productSalesPriceMax,
          )
    // 公司商品池是资格投影，启停/待生效指标无业务含义；不展示伪筛选指标条。
    const metrics = isSellableResource
        ? []
        : isSupplierResource
          ? syncedMetrics.filter((metric) => metric.key !== "pending")
          : syncedMetrics
    const noDataWithCreate = !listLoadFailed && rows.length === 0

    return (
        <PageScaffold density="compact">
            <PageHeader
                title={masterDataCopy.pageTitle(resourceLabel(resource))}
                breadcrumbs={[
                    {
                        id: "md",
                        label: "基础资料",
                        href: "/master-data",
                    },
                    {
                        id: "resource",
                        label: resourceLabel(resource),
                        current: true,
                    },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt="刚刚"
                        dateTime={listQuery.data?.queriedAt ?? ""}
                        state="fresh"
                        label="基础资料列表"
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "export",
                                label: masterDataCopy.actionExport,
                                icon: DownloadIcon,
                                variant: "outline",
                                mobileVisibility: "hide" as const,
                                disabled: rows.length === 0,
                                onClick: handleExport,
                            },
                            ...(!isSellableResource
                                ? [
                                      {
                                          actionKey: "create",
                                          label: isWarehouse
                                              ? masterDataCopy.actionCreateClosed
                                              : masterDataCopy.actionCreate,
                                          mobileVisibility: "hide" as const,
                                          icon: PlusIcon,
                                          // 仓库写门禁未开放：按钮真正禁用，不再进入注定失败的表单。
                                          disabled: isWarehouse || !canCreate,
                                          title: isWarehouse
                                              ? masterDataCopy.warehouseWriteBody
                                              : !canCreate
                                                ? createBlockedReason
                                                : undefined,
                                          onClick: () => {
                                              if (
                                                  isProductResource ||
                                                  isSupplierResource
                                              ) {
                                                  router.push(
                                                      `/master-data/${resource}/new`,
                                                  )
                                              } else {
                                                  setCreateOpen(true)
                                              }
                                          },
                                      },
                                  ]
                                : []),
                        ]}
                    />
                }
            />

            {isWarehouse ? (
                <FormalActionResult
                    status="blocked"
                    title={masterDataCopy.warehouseWriteTitle}
                    description={masterDataCopy.warehouseWriteBody}
                    actions={
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={
                                    <Link href="/master-data/sellable-items" />
                                }
                            >
                                去公司商品池
                            </Button>
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={<Link href="/inventory?view=balance" />}
                            >
                                打开库存台账
                            </Button>
                        </div>
                    }
                />
            ) : null}

            {resource === "brands" ? (
                <p className="text-sm text-muted-foreground">
                    {masterDataCopy.brandListHint}
                </p>
            ) : null}

            {resource === "unit-of-measures" ? (
                <p className="text-sm text-muted-foreground">
                    {masterDataCopy.unitListHint}
                </p>
            ) : null}

            {resource === "sellable-items" ? (
                <p className="text-sm text-muted-foreground">
                    {masterDataCopy.sellableItemsHint}
                </p>
            ) : null}

            {isProductResource && listingError ? (
                <p className="text-sm text-destructive" role="alert">
                    {listingError}
                </p>
            ) : null}

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
                            按当前筛选导出 {exportMeta.rowCount} 条。任务号{" "}
                            <span className="num">{exportMeta.jobId}</span>
                            。不含无权限查看的敏感信息。
                        </>
                    }
                />
            ) : null}

            {!isVoucherCategoryResource && metrics.length > 0 ? (
                <MetricStrip
                    columns={4}
                    aria-label={`${resourceLabel(resource)}${
                        isSupplierResource ? "指标" : "指标筛选"
                    }`}
                >
                    {metrics.map((metric) => {
                        const isLifecycleMetric =
                            metric.key === "all" ||
                            metric.key === "enabled" ||
                            metric.key === "disabled"
                        if (isSupplierResource || !isLifecycleMetric) {
                            // 供应商页使用显式提交筛选面板；指标只读展示，避免第二套即时筛选。
                            // 待生效更新属于版本状态维度（有独立筛选控件），同样只读展示。
                            return (
                                <MetricItem
                                    key={metric.key}
                                    label={metric.label}
                                    value={metric.value}
                                    detail={metric.detail}
                                />
                            )
                        }
                        return (
                            <MetricFilterItem
                                key={metric.key}
                                label={metric.label}
                                value={metric.value}
                                detail={metric.detail}
                                // metricKey 与 lifecycleStatus 同源写入；指标高亮只做展示，筛选由 lifecycleStatus 承担
                                active={metricKey === metric.key}
                                onClick={() =>
                                    changeLifecycle(
                                        metric.key as
                                            | "enabled"
                                            | "disabled"
                                            | "all",
                                    )
                                }
                            />
                        )
                    })}
                </MetricStrip>
            ) : null}

            <h2
                ref={resultsHeadingRef}
                tabIndex={-1}
                className="sr-only outline-none"
            >
                {resourceLabel(resource)} · {rows.length} 条结果
            </h2>

            <BusinessTableFrame
                title={`${resourceLabel(resource)}列表`}
                description={
                    isSellableResource
                        ? (sellableTableDescription ??
                          masterDataCopy.sellableListDescription(rows.length))
                        : isProductResource
                          ? masterDataCopy.productListDescription(rows.length)
                          : isSupplierResource
                            ? masterDataCopy.supplierListDescription(
                                  rows.length,
                              )
                            : (listTableDescription ??
                              masterDataCopy.listDescription(rows.length))
                }
                toolbar={
                    <MasterDataListToolbar
                        isProductResource={isProductResource}
                        isSupplierResource={isSupplierResource}
                        isSellableResource={isSellableResource}
                        resource={resource}
                        searchInputRef={searchInputRef}
                        searchDraft={searchDraft}
                        setSearchDraft={setSearchDraft}
                        commitSearch={commitSearch}
                        rowCount={rows.length}
                        hasActiveFilters={hasActiveFilters}
                        clearAllFilters={clearAllFilters}
                        filterPanelOpen={filterPanelOpen}
                        setFilterPanelOpen={setFilterPanelOpen}
                        hasStructuredListFilters={hasStructuredListFilters}
                        applyListFilters={applyListFilters}
                        productFilterPanelOpen={productFilterPanelOpen}
                        setProductFilterPanelOpen={setProductFilterPanelOpen}
                        hasStructuredProductFilters={
                            hasStructuredProductFilters
                        }
                        applyProductFilters={applyProductFilters}
                        sellableFilterPanelOpen={sellableFilterPanelOpen}
                        setSellableFilterPanelOpen={setSellableFilterPanelOpen}
                        hasStructuredSellableFilters={
                            hasStructuredSellableFilters
                        }
                        applySellableFilters={applySellableFilters}
                        supplyRegionDraft={supplyRegionDraft}
                        setSupplyRegionDraft={setSupplyRegionDraft}
                        productKindDraft={productKindDraft}
                        setProductKindDraft={setProductKindDraft}
                        lifecycleStatusDraft={lifecycleStatusDraft}
                        setLifecycleStatusDraft={setLifecycleStatusDraft}
                        revisionTimingDraft={revisionTimingDraft}
                        setRevisionTimingDraft={setRevisionTimingDraft}
                        productListingStatusDraft={productListingStatusDraft}
                        setProductListingStatusDraft={
                            setProductListingStatusDraft
                        }
                        productSupplyCoverageDraft={productSupplyCoverageDraft}
                        setProductSupplyCoverageDraft={
                            setProductSupplyCoverageDraft
                        }
                        productCategoryIdDraft={productCategoryIdDraft}
                        setProductCategoryIdDraft={setProductCategoryIdDraft}
                        productBrandIdDraft={productBrandIdDraft}
                        setProductBrandIdDraft={setProductBrandIdDraft}
                        productSupplierIdDraft={productSupplierIdDraft}
                        setProductSupplierIdDraft={setProductSupplierIdDraft}
                        productSalesPriceMinDraft={productSalesPriceMinDraft}
                        setProductSalesPriceMinDraft={
                            setProductSalesPriceMinDraft
                        }
                        productSalesPriceMaxDraft={productSalesPriceMaxDraft}
                        setProductSalesPriceMaxDraft={
                            setProductSalesPriceMaxDraft
                        }
                        productSalesPriceError={productSalesPriceError}
                        setProductSalesPriceError={setProductSalesPriceError}
                        productFilterOptionsQuery={productFilterOptionsQuery}
                        supplierFilterPanelOpen={supplierFilterPanelOpen}
                        setSupplierFilterPanelOpen={setSupplierFilterPanelOpen}
                        hasStructuredSupplierFilters={
                            hasStructuredSupplierFilters
                        }
                        applySupplierFilters={applySupplierFilters}
                        supplierQualificationHealthDraft={
                            supplierQualificationHealthDraft
                        }
                        setSupplierQualificationHealthDraft={
                            setSupplierQualificationHealthDraft
                        }
                        supplierCapabilityCodesDraft={
                            supplierCapabilityCodesDraft
                        }
                        setSupplierCapabilityCodesDraft={
                            setSupplierCapabilityCodesDraft
                        }
                        supplierQualificationTypesDraft={
                            supplierQualificationTypesDraft
                        }
                        setSupplierQualificationTypesDraft={
                            setSupplierQualificationTypesDraft
                        }
                    />
                }
                table={
                    <DataTable
                        data={pageRows}
                        columns={columns}
                        getRowId={(row) => row.stableId}
                        rowCount={rows.length}
                        pagination={pagination}
                        onPaginationChange={(next) => {
                            setPagination(next)
                            const page = next.pageIndex + 1
                            patchUrl({ page: page > 1 ? String(page) : null })
                        }}
                        layout="flush"
                        density="compact"
                        defaultColumnPinning={{
                            left: [isSellableResource ? "name" : "stableNo"],
                            right: isSellableResource ? [] : ["actions"],
                        }}
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    error={listQuery.error}
                                    onRetry={() => void listQuery.refetch()}
                                />
                            ) : undefined
                        }
                        emptyState={
                            noDataWithCreate ? (
                                <BusinessEmptyState
                                    kind={
                                        hasActiveFilters ? "filter" : "no-data"
                                    }
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    title={
                                        hasActiveFilters
                                            ? "当前筛选无结果"
                                            : `还没有${resourceLabel(resource)}资料`
                                    }
                                    description={
                                        hasActiveFilters
                                            ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                            : isSellableResource
                                              ? "当前资格日期下没有可销售的公司 SKU。请确认商品已上架、资料有效且存在有效供给。"
                                              : "点击「新建」创建第一份资料；历史记录会随资料保留。"
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={clearAllFilters}
                                            >
                                                清除筛选
                                            </Button>
                                        ) : !isWarehouse &&
                                          !isSellableResource &&
                                          canCreate ? (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={() => {
                                                    if (
                                                        isProductResource ||
                                                        isSupplierResource
                                                    ) {
                                                        router.push(
                                                            `/master-data/${resource}/new`,
                                                        )
                                                    } else {
                                                        setCreateOpen(true)
                                                    }
                                                }}
                                            >
                                                {masterDataCopy.actionCreate}
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                        onRowPreview={(row) => {
                            lastFocusedRowId.current = row.stableId
                            if (isProductResource || isSupplierResource) {
                                router.push(
                                    `/master-data/${resource}/${row.stableId}?section=overview`,
                                )
                            } else if (
                                isBrandResource ||
                                isVoucherCategoryResource ||
                                isUnitOfMeasureResource
                            ) {
                                setReviseTarget(row)
                            } else {
                                setPreviewId(row.stableId)
                            }
                        }}
                        onRowOpen={(row) => {
                            lastFocusedRowId.current = row.stableId
                            if (isProductResource || isSupplierResource) {
                                router.push(
                                    `/master-data/${resource}/${row.stableId}?section=overview`,
                                )
                                return
                            }
                            if (
                                isBrandResource ||
                                isVoucherCategoryResource ||
                                isUnitOfMeasureResource
                            ) {
                                setReviseTarget(row)
                                return
                            }
                            setPreviewId(row.stableId)
                        }}
                    />
                }
            />
            <MasterDataPreviewSheet
                skipPreviewSheet={skipPreviewSheet}
                previewRow={previewRow}
                lastFocusedRowId={lastFocusedRowId}
                isSellableResource={isSellableResource}
                resource={resource}
                previewDetail={previewDetailQuery.data}
                previewDetailLoading={previewDetailQuery.isPending}
                onClose={() => setPreviewId(null)}
                onRevise={setReviseTarget}
                onDisable={setDisableTarget}
            />
            <MasterDataListDialogs
                resource={resource}
                isProductResource={isProductResource}
                isSupplierResource={isSupplierResource}
                isVoucherCategoryResource={isVoucherCategoryResource}
                isSellableResource={isSellableResource}
                createOpen={createOpen}
                setCreateOpen={setCreateOpen}
                reviseTarget={reviseTarget}
                setReviseTarget={setReviseTarget}
                disableTarget={disableTarget}
                setDisableTarget={setDisableTarget}
                supplyProduct={supplyProduct}
                setSupplyProduct={setSupplyProduct}
                supplyDialogSku={supplyDialogSku}
                setSupplyDialogSku={setSupplyDialogSku}
                productSkusByProduct={productSkusByProduct}
                productSkusPending={productSkusQuery.isPending}
                productSkusError={productSkusQuery.error}
                onRetrySkus={() => void productSkusQuery.refetch()}
                offerings={supplierOfferingsQuery.data ?? []}
                offeringLoading={supplierOfferingsQuery.isPending}
                offeringError={supplierOfferingsQuery.error}
                onRetryOfferings={() => void supplierOfferingsQuery.refetch()}
                productPageSkuIds={productPageSkuIds}
            />
        </PageScaffold>
    )
}
