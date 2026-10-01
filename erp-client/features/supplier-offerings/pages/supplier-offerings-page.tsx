"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useSearchParams } from "next/navigation"
import { OfferingPreviewSheet } from "../components/offering-preview-sheet"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { BatchSupplyDialog } from "../components/batch/batch-supply-dialog"
import type { BatchMode } from "../lib/batch-supply"
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
import { ChangeOfferingStatusDialog } from "@/features/supplier-offerings/components/dialogs/change-offering-status-dialog"
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
import type { OfferingStatusIntent } from "@/features/supplier-offerings/lib/offering-status"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"

const PAGE_SIZE = 50

/** 供应商供给列表与维护入口。 */
export const SupplierOfferingsPage = () => {
    const state = useSupplierOfferingsPageState()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnTo = `${pathname}?${searchParams.toString()}`
    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const lastFocusedRowId = React.useRef<string | null>(null)
    const closePreview = () => setPreviewId(null)
    const restorePreviewFocus = () => {
        const id = lastFocusedRowId.current
        if (id) {
            document
                .querySelector<HTMLElement>(`[data-row-id="${CSS.escape(id)}"]`)
                ?.focus()
        }
    }
    const account = useAccountProfileQuery()
    const canBatchCreate = hasPermission(
        account.data?.permissions,
        "supplier_offering:create",
    )
    const canBatchRevise =
        hasPermission(account.data?.permissions, "supplier_offering:update") &&
        hasPermission(
            account.data?.permissions,
            "supplier_offering_cost:detail",
        )
    const canBatchAvailability = hasPermission(
        account.data?.permissions,
        "supplier_offering_availability:update",
    )
    const [selection, setSelection] = React.useState<{
        scope: string
        ids: string[]
    }>({ scope: "", ids: [] })
    const [batch, setBatch] = React.useState<{
        mode: BatchMode
        offerings: SupplierOfferingView[]
    } | null>(null)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [reviseOffering, setReviseOffering] =
        React.useState<SupplierOfferingView | null>(null)
    const [availabilityOffering, setAvailabilityOffering] =
        React.useState<SupplierOfferingView | null>(null)
    const [statusChange, setStatusChange] = React.useState<{
        offering: SupplierOfferingView
        intent: OfferingStatusIntent
    } | null>(null)
    const query = useSupplierOfferingsQuery({
        q: state.urlState.q,
        skuId: state.urlState.skuId,
        skuNo: state.urlState.skuNo,
        productNo: state.urlState.productNo,
        supplierId: state.urlState.supplierId,
        status: state.urlState.status,
        sourceType: state.urlState.sourceType,
        availabilityStatus: state.urlState.availabilityStatus,
        ownerUserIds: state.urlState.ownerUserIds,
        procurementOwnerUserIds: state.urlState.procurementOwnerUserIds,
        orgUnitIds: state.urlState.orgUnitIds,
        includeDescendants: state.urlState.includeDescendants,
        scopeVersion:
            state.urlState.page > 1 ? state.urlState.scopeVersion : undefined,
        page: state.urlState.page,
        pageSize: PAGE_SIZE,
    })
    const taskQuery = useSupplierSupplyExceptionWorkItemQuery(
        state.urlState.workItemId,
    )
    const items = query.data?.items ?? []
    const selectionScope = items.map((item) => item.id).join("|")
    const selectedIds = selection.scope === selectionScope ? selection.ids : []
    const selectedItems = items.filter((item) => selectedIds.includes(item.id))
    const taskOffering = taskQuery.data
        ? items.find((item) => item.id === taskQuery.data.businessObjectId)
        : undefined
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
    const noScope = query.data?.empty_reason === "no_scope"
    const showWorkspaceEmptyState =
        !query.isError && items.length === 0 && !query.isPending

    const patchUrl = state.patchUrl
    const listPage = state.urlState.page
    const listScopeVersion = state.urlState.scopeVersion
    React.useEffect(() => {
        const next = query.data?.scope_version
        if (next && listPage === 1 && listScopeVersion !== next) {
            patchUrl({ scopeVersion: next })
        }
    }, [query.data?.scope_version, patchUrl, listPage, listScopeVersion])

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
                    {!state.taskMode && canBatchCreate && (
                        <Button
                            id="supplier-offerings-page-batch-create"
                            type="button"
                            variant="outline"
                            onClick={() =>
                                setBatch({ mode: "create", offerings: [] })
                            }
                        >
                            批量添加供给
                        </Button>
                    )}
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
                                    active: !state.urlState.status,
                                    onClick: () =>
                                        state.patchUrl({
                                            status: undefined,
                                            scopeVersion: undefined,
                                            page: 1,
                                        }),
                                },
                                ...(
                                    [
                                        ["ACTIVE", "active", "已启用"],
                                        ["PAUSED", "paused", "已暂停"],
                                        ["STOPPED", "stopped", "已停止"],
                                    ] as const
                                ).map(([status, id, label]) => ({
                                    id: `supplier-offerings-view-${id}`,
                                    label,
                                    active: state.urlState.status === status,
                                    onClick: () =>
                                        state.patchUrl({
                                            status,
                                            scopeVersion: undefined,
                                            page: 1,
                                        }),
                                })),
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
                            onCancelMoreFilters={state.cancelMoreFilters}
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
                            ownerUserIdsDraft={state.ownerUserIdsDraft}
                            onOwnerUserIdsDraftChange={
                                state.setOwnerUserIdsDraft
                            }

                            procurementOwnerUserIdsDraft={
                                state.procurementOwnerUserIdsDraft
                            }
                            onProcurementOwnerUserIdsDraftChange={
                                state.setProcurementOwnerUserIdsDraft
                            }

                            orgUnitIdsDraft={state.orgUnitIdsDraft}
                            onOrgUnitIdsDraftChange={state.setOrgUnitIdsDraft}
                            includeDescendantsDraft={
                                state.includeDescendantsDraft
                            }
                            onIncludeDescendantsDraftChange={
                                state.setIncludeDescendantsDraft
                            }
                            hasPendingChanges={state.hasPendingChanges}
                            resultCount={query.data?.total}
                            loading={query.isFetching}
                            failed={query.isError}
                        />
                    }
                    selectionBar={
                        !state.taskMode &&
                        (canBatchRevise || canBatchAvailability) ? (
                            <div className="flex flex-wrap items-center gap-2 text-sm">
                                <span>本页已选 {selectedItems.length} 行</span>
                                <Button
                                    id="supplier-offerings-batch-select-page"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() =>
                                        setSelection({
                                            scope: selectionScope,
                                            ids: items.map((item) => item.id),
                                        })
                                    }
                                >
                                    选择本页
                                </Button>
                                <Button
                                    id="supplier-offerings-batch-clear"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() =>
                                        setSelection({
                                            scope: selectionScope,
                                            ids: [],
                                        })
                                    }
                                >
                                    清空
                                </Button>
                                {canBatchRevise && (
                                    <Button
                                        id="supplier-offerings-batch-revise"
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        disabled={!selectedItems.length}
                                        onClick={() =>
                                            setBatch({
                                                mode: "revise",
                                                offerings: selectedItems,
                                            })
                                        }
                                    >
                                        批量调价与条款
                                    </Button>
                                )}
                                {canBatchAvailability && (
                                    <Button
                                        id="supplier-offerings-batch-availability"
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        disabled={!selectedItems.length}
                                        onClick={() =>
                                            setBatch({
                                                mode: "availability",
                                                offerings: selectedItems,
                                            })
                                        }
                                    >
                                        批量更新可供情况
                                    </Button>
                                )}
                            </div>
                        ) : undefined
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
                                    selectedIds={selectedIds}
                                    onSelectionChange={
                                        !state.taskMode &&
                                        (canBatchRevise || canBatchAvailability)
                                            ? (ids) =>
                                                  setSelection({
                                                      scope: selectionScope,
                                                      ids,
                                                  })
                                            : undefined
                                    }
                                    onRowPreview={(offering) => {
                                        lastFocusedRowId.current = offering.id
                                        setPreviewId(offering.id)
                                    }}
                                    highlightedRowId={previewId ?? undefined}
                                    canRevise={canBatchRevise}
                                    canAvailability={canBatchAvailability}
                                    returnTo={returnTo}
                                    total={query.data?.total ?? 0}
                                    items={items}
                                    isPending={query.isPending}
                                    isError={query.isError}
                                    error={query.error}
                                    hasFilters={state.hasFilters}
                                    noScope={noScope}

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
                                    onChangeStatus={(offering, intent) =>
                                        setStatusChange({ offering, intent })
                                    }
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

            <OfferingPreviewSheet
                offering={items.find((item) => item.id === previewId) ?? null}
                returnTo={returnTo}
                onClose={closePreview}
                onClosed={restorePreviewFocus}
            />
            {batch && (
                <BatchSupplyDialog
                    mode={batch.mode}
                    offerings={batch.offerings}
                    supplierId={state.urlState.supplierId}
                    onClose={() => setBatch(null)}
                />
            )}
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
            {!state.taskMode && statusChange ? (
                <ChangeOfferingStatusDialog
                    key={`${statusChange.offering.id}-${statusChange.intent.actionId}`}
                    offering={statusChange.offering}
                    intent={statusChange.intent}
                    onOpenChange={(open) => {
                        if (!open) setStatusChange(null)
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
