"use client"

import * as React from "react"
import Link from "next/link"
import { PlusIcon } from "lucide-react"

import {
    BusinessTableFrame,
    MetricItem,
    MetricStrip,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { RegisterSupplyForSkuDialog } from "@/features/supplier-offerings/components/dialogs/register-supply-for-sku-dialog"
import { ReviseOfferingDialog } from "@/features/supplier-offerings/components/dialogs/revise-offering-dialog"
import { UpdateAvailabilityDialog } from "@/features/supplier-offerings/components/dialogs/update-availability-dialog"
import { SupplierOfferingsPagination } from "@/features/supplier-offerings/components/supplier-offerings-pagination"
import { SupplierOfferingsTable } from "@/features/supplier-offerings/components/supplier-offerings-table"
import { SupplierOfferingsToolbar } from "@/features/supplier-offerings/components/supplier-offerings-toolbar"
import { SupplyExceptionTaskPanel } from "@/features/supplier-offerings/components/supply-exception-task-panel"
import {
    buildSupplierOfferingAppliedChips,
    useSupplierOfferingsPageState,
} from "@/features/supplier-offerings/hooks/use-supplier-offerings-page-state"
import {
    useSupplierOfferingsQuery,
    useSupplierSupplyExceptionWorkItemQuery,
} from "@/features/supplier-offerings/hooks/queries"
import { isCurrentlyAvailable } from "@/features/supplier-offerings/lib/presentation"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"

const PAGE_SIZE = 50

/** 供应商供给列表与维护入口。 */
export const SupplierOfferingsPage = () => {
    const state = useSupplierOfferingsPageState()
    const [createOpen, setCreateOpen] = React.useState(false)
    const [reviseOffering, setReviseOffering] =
        React.useState<SupplierOfferingView | null>(null)
    const [availabilityOffering, setAvailabilityOffering] =
        React.useState<SupplierOfferingView | null>(null)
    const query = useSupplierOfferingsQuery({
        q: state.urlState.q,
        skuId: state.urlState.skuId,
        skuNo: state.urlState.skuNo,
        productNo: state.urlState.productNo,
        supplierId: state.urlState.supplierId,
        status: state.urlState.status,
        sourceType: state.urlState.sourceType,
        availabilityStatus: state.urlState.availabilityStatus,
        page: state.urlState.page,
        pageSize: PAGE_SIZE,
    })
    const taskQuery = useSupplierSupplyExceptionWorkItemQuery(
        state.urlState.workItemId,
    )
    const items = query.data?.items ?? []
    const taskOffering = taskQuery.data
        ? items.find((item) => item.id === taskQuery.data.businessObjectId)
        : undefined
    const activeCount = items.filter((item) => item.status === "ACTIVE").length
    const availableCount = items.filter(isCurrentlyAvailable).length
    const totalPages = Math.max(
        1,
        Math.ceil((query.data?.total ?? 0) / PAGE_SIZE),
    )
    /** 全部已生效条件派生为可移除 chip；业务名称优先取自当前结果首行（§3.6）。 */
    const appliedChips = React.useMemo(
        () =>
            buildSupplierOfferingAppliedChips(state.urlState, {
                skuNoLabel: state.urlState.skuId
                    ? items[0]?.sku_no ?? null
                    : null,
                supplierNameLabel: state.urlState.supplierId
                    ? items[0]?.supplier_name ?? null
                    : null,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [items, state.urlState],
    )

    return (
        <PageScaffold density="compact">
            <PageHeader
                title={
                    state.taskMode
                        ? "供应停止核对"
                        : state.skuLocked
                          ? "SKU 供给"
                          : "供应商供给"
                }
                description={
                    state.taskMode
                        ? "只读核对安全暂停来源与影响；不选定替代供给，不恢复商品销售。"
                        : state.skuLocked
                          ? "维护当前公司 SKU 的供应商、订货编码、商业条款与可供情况。"
                          : "每条记录直接连接一个公司 SKU 与一个供应商；不存在独立的供应商商品主档。"
                }
                actions={
                    <div className="flex items-center gap-2">
                        {state.taskMode ? (
                            <Button
                                type="button"
                                variant="outline"
                                render={
                                    <Link
                                        href={`/workspace/tasks?${new URLSearchParams(
                                            {
                                                currentWorkItemId:
                                                    state.urlState.workItemId ??
                                                    "",
                                                ...(state.urlState
                                                    .queueContextId
                                                    ? {
                                                          queueContextId:
                                                              state.urlState
                                                                  .queueContextId,
                                                      }
                                                    : {}),
                                            },
                                        ).toString()}`}
                                    />
                                }
                            >
                                返回待办队列
                            </Button>
                        ) : state.urlState.returnTo ? (
                            <Button
                                type="button"
                                variant="outline"
                                render={
                                    <Link href={state.urlState.returnTo} />
                                }
                            >
                                返回商品
                            </Button>
                        ) : null}
                        {!state.taskMode ? (
                            <Button
                                type="button"
                                onClick={() => setCreateOpen(true)}
                            >
                                <PlusIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                添加供给
                            </Button>
                        ) : null}
                    </div>
                }
            />

            {state.taskMode && state.urlState.workItemId ? (
                <SupplyExceptionTaskPanel
                    workItemId={state.urlState.workItemId}
                    queueContextId={state.urlState.queueContextId}
                    task={taskQuery.data}
                    offering={taskOffering}
                    isPending={taskQuery.isPending}
                    error={taskQuery.error}
                    onRetry={() => void taskQuery.refetch()}
                />
            ) : null}

            {!state.taskMode || taskQuery.data ? (
                <>
                    {/* 只读指标：不属于筛选表单（§2.1、§7）。 */}
                    <MetricStrip
                        columns={3}
                        density="compact"
                        aria-label="供给列表指标"
                    >
                        <MetricItem
                            label="当前结果"
                            value={query.data?.total ?? 0}
                        />
                        <MetricItem label="本页启用关系" value={activeCount} />
                        <MetricItem
                            label="本页当前可供"
                            value={availableCount}
                        />
                    </MetricStrip>

                    <BusinessTableFrame
                        showHeader
                        title={
                            <span className="inline-flex items-baseline gap-2">
                                供给关系列表
                                <span
                                    className="font-normal text-muted-foreground"
                                    aria-live="polite"
                                >
                                    {query.data?.total ?? 0} 条
                                </span>
                            </span>
                        }
                        description={
                            state.appliedFilterLabels.length > 0
                                ? `筛选条件：${state.appliedFilterLabels.join("、")}`
                                : "商业条款按版本追加；可供状态与数量独立更新。"
                        }
                        toolbar={
                            <SupplierOfferingsToolbar
                                searchInputRef={state.searchInputRef}
                                searchDraft={state.searchDraft}
                                onSearchDraftChange={state.setSearchDraft}
                                filterPanelOpen={state.filterPanelOpen}
                                onFilterPanelOpenChange={
                                    state.setFilterPanelOpen
                                }
                                hasStructuredFilters={
                                    state.hasStructuredFilters
                                }
                                appliedChips={appliedChips}
                                removeFilter={state.removeFilter}
                                onApplyFilters={state.applyFilters}
                                onClearFilters={state.clearFilters}
                                onResetMoreFilters={state.resetMoreFilters}
                                statusDraft={state.statusDraft}
                                onStatusDraftChange={state.setStatusDraft}
                                sourceTypeDraft={state.sourceTypeDraft}
                                onSourceTypeDraftChange={
                                    state.setSourceTypeDraft
                                }
                                availabilityStatusDraft={
                                    state.availabilityStatusDraft
                                }
                                onAvailabilityStatusDraftChange={
                                    state.setAvailabilityStatusDraft
                                }
                                skuLocked={state.skuLocked}
                                skuIdDraft={state.skuIdDraft}
                                onSkuIdDraftChange={state.setSkuIdDraft}
                                skuNoDraft={state.skuNoDraft}
                                onSkuNoDraftChange={state.setSkuNoDraft}
                                productNoDraft={state.productNoDraft}
                                onProductNoDraftChange={
                                    state.setProductNoDraft
                                }
                                supplierIdDraft={state.supplierIdDraft}
                                onSupplierIdDraftChange={
                                    state.setSupplierIdDraft
                                }
                            />
                        }
                        table={
                            <SupplierOfferingsTable
                                items={items}
                                isPending={query.isPending}
                                isError={query.isError}
                                error={query.error}
                                hasFilters={state.hasFilters}
                                taskMode={state.taskMode}
                                taskBusinessObjectId={
                                    taskQuery.data?.businessObjectId
                                }
                                onRetry={() => void query.refetch()}
                                onClearFilters={state.clearFilters}
                                onCreateOffering={() => setCreateOpen(true)}
                                onUpdateAvailability={setAvailabilityOffering}
                                onReviseOffering={setReviseOffering}
                            />
                        }
                        footer={
                            <SupplierOfferingsPagination
                                page={state.urlState.page}
                                totalPages={totalPages}
                                disabled={query.isPending}
                                onPageChange={(page) =>
                                    state.patchUrl({ page })
                                }
                            />
                        }
                    />
                </>
            ) : null}

            {!state.taskMode ? (
                <RegisterSupplyForSkuDialog
                    key={
                        createOpen
                            ? `create-${state.urlState.skuId ?? "select"}`
                            : "closed"
                    }
                    open={createOpen}
                    onOpenChange={setCreateOpen}
                    fixedSku={
                        state.urlState.skuId
                            ? {
                                  skuId: state.urlState.skuId,
                                  skuCode: items[0]?.sku_no ?? "当前公司 SKU",
                                  skuName: items[0]?.sku_name ?? "当前公司 SKU",
                                  specification:
                                      items[0]?.specification ?? "默认规格",
                                  baseUnit: "",
                              }
                            : undefined
                    }
                />
            ) : null}
            {!state.taskMode && reviseOffering ? (
                <ReviseOfferingDialog
                    key={reviseOffering.id}
                    offering={reviseOffering}
                    onOpenChange={(open) => {
                        if (!open) setReviseOffering(null)
                    }}
                />
            ) : null}
            {!state.taskMode && availabilityOffering ? (
                <UpdateAvailabilityDialog
                    key={availabilityOffering.id}
                    offering={availabilityOffering}
                    onOpenChange={(open) => {
                        if (!open) setAvailabilityOffering(null)
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
