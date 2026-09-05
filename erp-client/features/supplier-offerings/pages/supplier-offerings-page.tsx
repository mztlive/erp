"use client"

import * as React from "react"
import Link from "next/link"
import { PlusIcon } from "lucide-react"

import { PageHeader, PageScaffold } from "@/components/business"
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
                        ? "核对安全暂停来源与影响并登记处置证据；完成任务不恢复供给或商品销售。"
                        : state.skuLocked
                          ? "维护当前公司 SKU 的供应商、订货编码、商业条款与可供情况。"
                          : "每条记录直接连接一个公司 SKU 与一个供应商；不存在独立的供应商商品主档。"
                }
                actions={
                    <div className="flex items-center gap-2">
                        {state.taskMode ? (
                            <Button
                                id="supplier-offerings-page-back-workspace"
                                type="button"
                                variant="outline"
                                render={
                                    <Link
                                        href={`/workspace?${new URLSearchParams(
                                            {
                                                currentWorkItemId:
                                                    state.urlState.workItemId ??
                                                    "",
                                            },
                                        ).toString()}`}
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
                }
            />

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
                <div className="overflow-hidden rounded-xl border border-border/80 bg-card shadow-2xs transition-all">
                    {/* 1. 快捷状态分段筛选条：高反差精密轨道，彻底告别低透明度发虚 */}
                    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/80 bg-muted/35 px-4 py-2.5 sm:px-5">
                        <div className="inline-flex items-center gap-1 rounded-lg border border-border/80 bg-surface-sunken p-1 text-xs shadow-2xs">
                            <button
                                type="button"
                                onClick={() =>
                                    state.patchUrl({
                                        status: undefined,
                                        availabilityStatus: undefined,
                                        page: 1,
                                    })
                                }
                                className={cn(
                                    "inline-flex items-center gap-2 rounded-md px-3 py-1.5 font-medium transition-all",
                                    !state.urlState.status &&
                                        !state.urlState.availabilityStatus
                                        ? "bg-card text-foreground font-semibold shadow-xs border border-border/80 ring-1 ring-black/5 dark:ring-white/10"
                                        : "text-muted-foreground hover:text-foreground hover:bg-card/50",
                                )}
                            >
                                全部供给
                                <span
                                    className={cn(
                                        "rounded-full px-1.5 py-0.2 text-[10px] tabular-nums font-semibold transition-colors",
                                        !state.urlState.status &&
                                            !state.urlState.availabilityStatus
                                            ? "bg-foreground text-background"
                                            : "bg-muted-foreground/15 text-foreground/75",
                                    )}
                                >
                                    {query.data?.total ?? 0}
                                </span>
                            </button>
                            <button
                                type="button"
                                onClick={() =>
                                    state.patchUrl({
                                        status: "ACTIVE",
                                        availabilityStatus: undefined,
                                        page: 1,
                                    })
                                }
                                className={cn(
                                    "inline-flex items-center gap-2 rounded-md px-3 py-1.5 font-medium transition-all",
                                    state.urlState.status === "ACTIVE"
                                        ? "bg-card text-foreground font-semibold shadow-xs border border-border/80 ring-1 ring-black/5 dark:ring-white/10"
                                        : "text-muted-foreground hover:text-foreground hover:bg-card/50",
                                )}
                            >
                                已启用
                                <span
                                    className={cn(
                                        "rounded-full px-1.5 py-0.2 text-[10px] tabular-nums font-semibold transition-colors",
                                        state.urlState.status === "ACTIVE"
                                            ? "bg-foreground text-background"
                                            : "bg-muted-foreground/15 text-foreground/75",
                                    )}
                                >
                                    {activeCount}
                                </span>
                            </button>
                            <button
                                type="button"
                                onClick={() =>
                                    state.patchUrl({
                                        availabilityStatus: "AVAILABLE",
                                        status: undefined,
                                        page: 1,
                                    })
                                }
                                className={cn(
                                    "inline-flex items-center gap-2 rounded-md px-3 py-1.5 font-medium transition-all",
                                    state.urlState.availabilityStatus ===
                                        "AVAILABLE"
                                        ? "bg-card text-foreground font-semibold shadow-xs border border-border/80 ring-1 ring-black/5 dark:ring-white/10"
                                        : "text-muted-foreground hover:text-foreground hover:bg-card/50",
                                )}
                            >
                                当前可供
                                <span
                                    className={cn(
                                        "rounded-full px-1.5 py-0.2 text-[10px] tabular-nums font-semibold transition-colors",
                                        state.urlState.availabilityStatus ===
                                            "AVAILABLE"
                                            ? "bg-foreground text-background"
                                            : "bg-muted-foreground/15 text-foreground/75",
                                    )}
                                >
                                    {availableCount}
                                </span>
                            </button>
                        </div>
                        <div className="text-xs text-muted-foreground font-medium">
                            {state.appliedFilterLabels.length > 0
                                ? `已生效筛选：${state.appliedFilterLabels.join("、")}`
                                : "商业条款按版本追加 · 状态与数量独立更新"}
                        </div>
                    </div>

                    {/* 2. 内嵌工具栏（搜索框已收敛比例，筛选成组紧邻） */}
                    <div className="px-4 py-3 sm:px-5">
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
                            onProductNoDraftChange={state.setProductNoDraft}
                            supplierIdDraft={state.supplierIdDraft}
                            onSupplierIdDraftChange={
                                state.setSupplierIdDraft
                            }
                        />
                    </div>

                    {/* 3. 核心数据表（无缝嵌入，去除了多余的二次标题卡片） */}
                    <div className="border-t border-border/70">
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
                    </div>

                    {/* 4. 底部分页栏（内嵌整洁对齐） */}
                    <div className="border-t border-border/70 bg-muted/10 px-4 py-2.5 sm:px-5">
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
