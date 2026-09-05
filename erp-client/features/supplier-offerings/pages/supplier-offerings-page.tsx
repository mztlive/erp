"use client"

import * as React from "react"
import Link from "next/link"
import { PlusIcon } from "lucide-react"

import { PageScaffold } from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
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
                    ? (items[0]?.sku_no ?? null)
                    : null,
                supplierNameLabel: state.urlState.supplierId
                    ? (items[0]?.supplier_name ?? null)
                    : null,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [items, state.urlState],
    )
    const title = state.taskMode
        ? "供应停止核对"
        : state.skuLocked
          ? "SKU 供给"
          : "供应商供给"
    const description = state.taskMode
        ? "核对安全暂停来源与影响并登记处置证据；完成任务不恢复供给或商品销售。"
        : state.skuLocked
          ? "维护当前公司 SKU 的供应商、订货编码、商业条款与可供情况。"
          : "维护商品的供应商、供货价格、可供数量与配送范围。"
    const showWorkspaceEmptyState =
        !query.isError && items.length === 0 && !query.isPending

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="采购"
                title={title}
                description={description}
            >
                <div className="flex items-center gap-2">
                    {state.taskMode ? (
                        <Button
                            id="supplier-offerings-page-back-workspace"
                            type="button"
                            variant="outline"
                            render={
                                <Link
                                    href={`/workspace?${new URLSearchParams({
                                        currentWorkItemId:
                                            state.urlState.workItemId ?? "",
                                    }).toString()}`}
                                />
                            }
                        >
                            返回待办队列
                        </Button>
                    ) : state.urlState.returnTo ? (
                        <Button
                            id="supplier-offerings-page-back-product"
                            type="button"
                            variant="outline"
                            render={<Link href={state.urlState.returnTo} />}
                        >
                            返回商品
                        </Button>
                    ) : null}
                    {!state.taskMode ? (
                        <Button
                            id="supplier-offerings-page-create"
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
            </ListWorkspaceHeader>

            {state.taskMode && state.urlState.workItemId ? (
                <SupplyExceptionTaskPanel
                    workItemId={state.urlState.workItemId}
                    task={taskQuery.data}
                    offering={taskOffering}
                    isPending={taskQuery.isPending}
                    error={taskQuery.error}
                    onRetry={() => void taskQuery.refetch()}
                />
            ) : null}

            {!state.taskMode || taskQuery.data ? (
                <ListWorkSurface
                    ariaLabel="供应商供给列表"
                    views={
                        <ListWorkspaceViews
                            ariaLabel="供应商供给状态视图"
                            hint={
                                state.appliedFilterLabels.length > 0
                                    ? `已生效筛选：${state.appliedFilterLabels.join("、")}`
                                    : "商业条款按版本追加 · 状态与数量独立更新"
                            }
                            items={[
                                {
                                    id: "supplier-offerings-view-all",
                                    label: "全部供给",
                                    count: query.data?.total ?? 0,
                                    active:
                                        !state.urlState.status &&
                                        !state.urlState.availabilityStatus,
                                    onClick: () =>
                                        state.patchUrl({
                                            status: undefined,
                                            availabilityStatus: undefined,
                                            page: 1,
                                        }),
                                },
                                {
                                    id: "supplier-offerings-view-active",
                                    label: "已启用",
                                    count: activeCount,
                                    active: state.urlState.status === "ACTIVE",
                                    onClick: () =>
                                        state.patchUrl({
                                            status: "ACTIVE",
                                            availabilityStatus: undefined,
                                            page: 1,
                                        }),
                                },
                                {
                                    id: "supplier-offerings-view-available",
                                    label: "当前可供",
                                    count: availableCount,
                                    active:
                                        state.urlState.availabilityStatus ===
                                        "AVAILABLE",
                                    onClick: () =>
                                        state.patchUrl({
                                            availabilityStatus: "AVAILABLE",
                                            status: undefined,
                                            page: 1,
                                        }),
                                },
                            ]}
                        />
                    }
                    toolbar={
                        <SupplierOfferingsToolbar
                            searchInputRef={state.searchInputRef}
                            searchDraft={state.searchDraft}
                            onSearchDraftChange={state.setSearchDraft}
                            filterPanelOpen={state.filterPanelOpen}
                            onFilterPanelOpenChange={state.setFilterPanelOpen}
                            appliedChips={appliedChips}
                            removeFilter={state.removeFilter}
                            onApplyFilters={state.applyFilters}
                            onClearFilters={state.clearFilters}
                            onResetMoreFilters={state.resetMoreFilters}
                            statusDraft={state.statusDraft}
                            onStatusDraftChange={state.setStatusDraft}
                            sourceTypeDraft={state.sourceTypeDraft}
                            onSourceTypeDraftChange={state.setSourceTypeDraft}
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
                            onProductNoDraftChange={state.setProductNoDraft}
                            supplierIdDraft={state.supplierIdDraft}
                            onSupplierIdDraftChange={state.setSupplierIdDraft}
                            hasPendingChanges={state.hasPendingChanges}
                            resultCount={query.data?.total}
                            loading={query.isFetching}
                            failed={query.isError}
                        />
                    }
                    tableClassName="flex flex-col"
                    table={
                        <div className="flex min-h-0 flex-1 flex-col">
                            <div
                                className={cn(
                                    "min-h-0 flex-1 overflow-auto",
                                    showWorkspaceEmptyState &&
                                        listWorkspaceEmptyStateClassName,
                                    showWorkspaceEmptyState &&
                                        "[&>[data-slot=business-empty-state]]:p-0",
                                )}
                            >
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
                                    onUpdateAvailability={
                                        setAvailabilityOffering
                                    }
                                    onReviseOffering={setReviseOffering}
                                />
                            </div>
                            <div className="shrink-0 border-t border-border py-3">
                                <SupplierOfferingsPagination
                                    page={state.urlState.page}
                                    totalPages={totalPages}
                                    disabled={query.isPending}
                                    onPageChange={(page) =>
                                        state.patchUrl({ page })
                                    }
                                />
                            </div>
                        </div>
                    }
                />
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
