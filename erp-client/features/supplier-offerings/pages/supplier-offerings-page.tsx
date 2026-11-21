"use client"

import * as React from "react"
import type { PaginationState, RowSelectionState } from "@tanstack/react-table"
import {
    FilePenLineIcon,
    PackageCheckIcon,
    PlusIcon,
    TruckIcon,
} from "lucide-react"
import Link from "next/link"

import {
    BusinessFailureState,
    ListSelectionBar,
    ListWorkSurface,
    ListWorkspaceHeader,
    ListWorkspaceViews,
    PageScaffold,
    listWorkspaceStyles as styles,
    useFrozenSelectionSnapshot,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { isDataScopeChanged } from "@/features/data-scope/cache"
import { restoreListRowFocus } from "@/lib/list-row-focus"
import { usePathname, useSearchParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"

import { BatchSupplyDialog } from "../components/batch/batch-supply-dialog"
import { AssignDeliveryTemplateDialog } from "../components/delivery-templates/assign-delivery-template-dialog"
import { ChangeOfferingStatusDialog } from "../components/dialogs/change-offering-status-dialog"
import { RegisterSupplyForSkuDialog } from "../components/dialogs/register-supply-for-sku-dialog"
import { ReviseOfferingDialog } from "../components/dialogs/revise-offering-dialog"
import { UpdateAvailabilityDialog } from "../components/dialogs/update-availability-dialog"
import { OfferingPreviewSheet } from "../components/offering-preview-sheet"
import { SupplierOfferingsPagination } from "../components/supplier-offerings-pagination"
import { SupplierOfferingsTable } from "../components/supplier-offerings-table"
import { SupplierOfferingsToolbar } from "../components/supplier-offerings-toolbar"
import { SupplyExceptionTaskPanel } from "../components/supply-exception-task-panel"
import {
    useSupplierOfferingsQuery,
    useSupplierSupplyExceptionWorkItemQuery,
} from "../hooks/queries"
import {
    buildSupplierOfferingAppliedChips,
    useSupplierOfferingsPageState,
} from "../hooks/use-supplier-offerings-page-state"
import type { BatchMode } from "../lib/batch-supply"
import type { OfferingStatusIntent } from "../lib/offering-status"
import type { SupplierOfferingView } from "../types"

const PAGE_SIZE = 50
const EMPTY_ROWS: readonly SupplierOfferingView[] = []
const STATUS_VIEWS = [
    [undefined, "all", "全部供给"],
    ["ACTIVE", "active", "已启用"],
    ["PAUSED", "paused", "已暂停"],
    ["STOPPED", "stopped", "已停止"],
] as const

type RowAction =
    | { kind: "revise" | "availability"; offering: SupplierOfferingView }
    | {
          kind: "status"
          offering: SupplierOfferingView
          intent: OfferingStatusIntent
      }

/** 供应商供给列表与维护入口。 */
export function SupplierOfferingsPage() {
    const state = useSupplierOfferingsPageState()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnTo = `${pathname}?${searchParams.toString()}`
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const canRead = hasPermission(permissions, "supplier_offering:list")
    const canCreate =
        canRead &&
        !state.taskMode &&
        hasPermission(permissions, "supplier_offering:create") &&
        state.canFilterSupplier &&
        (Boolean(state.urlState.skuId) || state.canFilterSku)
    const canViewCosts = hasPermission(
        permissions,
        "supplier_offering_cost:detail",
    )
    const canAssignTemplate =
        canRead &&
        !state.taskMode &&
        hasPermission(permissions, "supplier_offering:update")
    const canBatchRevise = canAssignTemplate && canViewCosts
    const canBatchAvailability =
        canRead &&
        !state.taskMode &&
        hasPermission(permissions, "supplier_offering_availability:update")
    const canSelect = canAssignTemplate || canBatchAvailability

    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const lastFocusedRowId = React.useRef<string | null>(null)
    const [assigning, setAssigning] = React.useState<
        readonly SupplierOfferingView[] | null
    >(null)
    const [batch, setBatch] = React.useState<{
        mode: BatchMode
        offerings: readonly SupplierOfferingView[]
        selectionScope: string
    } | null>(null)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [rowAction, setRowAction] = React.useState<RowAction | null>(null)
    const openCreate = () => setCreateOpen(true)
    const openPreview = React.useCallback((offering: SupplierOfferingView) => {
        lastFocusedRowId.current = offering.id
        setPreviewId(offering.id)
    }, [])
    const openRevise = React.useCallback((offering: SupplierOfferingView) => {
        lastFocusedRowId.current = offering.id
        setRowAction({ kind: "revise", offering })
    }, [])
    const openAvailability = React.useCallback(
        (offering: SupplierOfferingView) => {
            lastFocusedRowId.current = offering.id
            setRowAction({ kind: "availability", offering })
        },
        [],
    )
    const openStatusChange = React.useCallback(
        (offering: SupplierOfferingView, intent: OfferingStatusIntent) => {
            lastFocusedRowId.current = offering.id
            setRowAction({ kind: "status", offering, intent })
        },
        [],
    )
    const restoreRowFocus = () => restoreListRowFocus(lastFocusedRowId.current)
    const onRowActionOpenChange = (open: boolean) => {
        if (open) return
        setRowAction(null)
        requestAnimationFrame(restoreRowFocus)
    }

    const { urlState, patchUrl } = state
    const listPage = urlState.page
    const listScopeVersion = urlState.scopeVersion
    const queryInput = {
        q: urlState.q,
        skuId: urlState.skuId,
        skuNo: urlState.skuNo,
        productNo: urlState.productNo,
        supplierId: urlState.supplierId,
        status: urlState.status,
        sourceType: urlState.sourceType,
        availabilityStatus: urlState.availabilityStatus,
        ownerUserIds: urlState.ownerUserIds,
        procurementOwnerUserIds: urlState.procurementOwnerUserIds,
        scopeVersion: listPage > 1 ? listScopeVersion : undefined,
        page: listPage,
        pageSize: PAGE_SIZE,
    }
    const query = useSupplierOfferingsQuery(queryInput, { enabled: canRead })
    const taskQuery = useSupplierSupplyExceptionWorkItemQuery(
        urlState.workItemId,
    )
    const loadFailed = query.isError || (!query.isPending && !query.data)
    const loading = (query.isPending || query.isFetching) && !loadFailed
    const [hasLoadedList, setHasLoadedList] = React.useState(false)
    if (canRead && !query.isPending && !hasLoadedList) setHasLoadedList(true)
    const scopeChanged = isDataScopeChanged(query.error)
    const items =
        canRead && !loadFailed ? (query.data?.items ?? EMPTY_ROWS) : EMPTY_ROWS
    const total = query.data?.total ?? 0
    const showTable = !state.taskMode || Boolean(taskQuery.data)
    const taskOffering = taskQuery.data
        ? items.find((item) => item.id === taskQuery.data.businessObjectId)
        : undefined

    // 批量维护仅针对当前页；换页、换筛选或授权范围变化都撤销勾选。
    const selectionScope = JSON.stringify([
        canSelect,
        queryInput,
        query.data?.scope_version,
        scopeChanged,
    ])
    const [selection, setSelection] = React.useState<{
        scope: string
        rows: RowSelectionState
    }>(() => ({ scope: selectionScope, rows: {} }))
    if (selection.scope !== selectionScope)
        setSelection({ scope: selectionScope, rows: {} })
    const rowSelection =
        canSelect && selection.scope === selectionScope ? selection.rows : {}
    const selectedItems = items.filter((item) => rowSelection[item.id])
    const onRowSelectionChange = (rows: RowSelectionState) =>
        setSelection({ scope: selectionScope, rows })
    const clearSelection = () => onRowSelectionChange({})
    const selectPage = () =>
        onRowSelectionChange(
            Object.fromEntries(items.map((item) => [item.id, true])),
        )
    const batchCount = selectedItems.filter(
        (item) => item.source_type !== "API",
    ).length
    const batchDisplay = useFrozenSelectionSnapshot(selectedItems.length, {
        batchCount,
        skippedCount: selectedItems.length - batchCount,
        assignCount: selectedItems.every(
            (item) => item.supplier_id === selectedItems[0].supplier_id,
        )
            ? selectedItems.length
            : 0,
    })
    const batchActions = [
        {
            key: "availability",
            id: "supplier-offerings-batch-availability",
            visible: canBatchAvailability,
            label: "批量更新可供情况",
            Icon: PackageCheckIcon,
            count: batchDisplay.batchCount,
            disabledReason: "接口来源的供给不能批量更新可供。",
            run: () => openBatch("availability"),
        },
        {
            key: "template",
            id: "supplier-offerings-batch-delivery-template",
            visible: canAssignTemplate,
            label: "设置配送模板",
            Icon: TruckIcon,
            count: batchDisplay.assignCount,
            disabledReason: "请只选择同一供应商的供给。",
            run: () => setAssigning(selectedItems),
        },
        {
            key: "revise",
            id: "supplier-offerings-batch-revise",
            visible: canBatchRevise,
            label: "批量调价与条款",
            Icon: FilePenLineIcon,
            count: batchDisplay.batchCount,
            disabledReason: "接口来源的供给不能批量修订条款。",
            run: () => openBatch("revise"),
        },
    ].filter((action) => action.visible)
    const primaryBatchAction = batchActions
        .filter((action) => action.count > 0)
        .at(-1)?.key
    const openBatch = (mode: BatchMode) =>
        setBatch({ mode, offerings: selectedItems, selectionScope })
    const closeBatch = (remainingIds?: readonly string[]) => {
        if (batch && remainingIds) {
            const remaining = new Set(remainingIds)
            setSelection((current) => {
                if (current.scope !== batch.selectionScope) return current
                const rows = { ...current.rows }
                for (const id of Object.keys(rows)) {
                    if (!remaining.has(id)) delete rows[id]
                }
                return { ...current, rows }
            })
        }
        setBatch(null)
    }

    const skuNo = items.find((item) => item.sku_id === urlState.skuId)?.sku_no
    const appliedChips = buildSupplierOfferingAppliedChips(urlState, skuNo)
    const title = state.taskMode
        ? "供应停止核对"
        : state.skuLocked
          ? "SKU 供给"
          : "供应商供给"
    const changePage = (page: number) => {
        if (page === listPage) return
        patchUrl({
            page,
            scopeVersion: query.data?.scope_version ?? listScopeVersion,
        })
    }
    const onPaginationChange = (pagination: PaginationState) =>
        changePage(pagination.pageIndex + 1)

    React.useEffect(() => {
        const next = query.data?.scope_version
        if (
            canRead &&
            !state.hasUnauthorizedFilters &&
            next &&
            listPage === 1 &&
            listScopeVersion !== next
        ) {
            patchUrl({ scopeVersion: next })
        }
    }, [
        canRead,
        query.data?.scope_version,
        patchUrl,
        listPage,
        listScopeVersion,
        state.hasUnauthorizedFilters,
    ])

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                title={title}
                views={
                    showTable && canRead ? (
                        <ListWorkspaceViews
                            ariaLabel="供应商供给状态视图"
                            variant="inline"
                            items={STATUS_VIEWS.map(([status, id, label]) => ({
                                id: `supplier-offerings-view-${id}`,
                                label,
                                active: urlState.status === status,
                                onClick: () =>
                                    patchUrl({
                                        status,
                                        scopeVersion: undefined,
                                        page: 1,
                                    }),
                            }))}
                        />
                    ) : undefined
                }
            >
                {state.taskMode ? (
                    <Button
                        id="supplier-offerings-page-back-workspace"
                        variant="ghost"
                        size="sm"
                        className={styles.quietButton}
                        render={
                            <Link
                                href={buildWorkspaceDetailHref(
                                    "tasks",
                                    urlState.workItemId ?? "",
                                )}
                            />
                        }
                    >
                        返回任务详情
                    </Button>
                ) : urlState.returnTo ? (
                    <Button
                        id="supplier-offerings-page-back-product"
                        variant="ghost"
                        size="sm"
                        className={styles.quietButton}
                        render={<Link href={urlState.returnTo} />}
                    >
                        返回商品
                    </Button>
                ) : null}
                {canCreate ? (
                    <Button
                        id="supplier-offerings-page-create"
                        type="button"
                        size="sm"
                        onClick={openCreate}
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden="true" />
                        添加供给
                    </Button>
                ) : null}
            </ListWorkspaceHeader>

            {state.taskMode && urlState.workItemId ? (
                <SupplyExceptionTaskPanel
                    workItemId={urlState.workItemId}
                    task={taskQuery.data}
                    offering={taskOffering}
                    isPending={taskQuery.isPending}
                    error={taskQuery.error}
                    onRetry={() => void taskQuery.refetch()}
                />
            ) : null}

            {showTable && !canRead ? (
                account.isError && !account.data ? (
                    <BusinessFailureState
                        idPrefix="supplier-offerings-account"
                        error={account.error}
                        onRetry={() => void account.refetch()}
                    />
                ) : account.data ? (
                    <BusinessFailureState
                        kind="permission"
                        title="无法查看供给列表"
                    />
                ) : (
                    <div
                        role="status"
                        className="py-3 text-body-compact text-muted-foreground"
                    >
                        正在读取账号权限…
                    </div>
                )
            ) : null}

            {showTable && canRead && !state.taskMode && !hasLoadedList ? (
                <div aria-busy="true" aria-label="正在加载供给列表">
                    <Skeleton className="mb-3 h-10 rounded-lg" />
                    <Skeleton className="h-96" />
                </div>
            ) : null}

            {showTable && canRead && (state.taskMode || hasLoadedList) ? (
                <ListWorkSurface
                    ariaLabel="供应商供给列表"
                    toolbar={
                        <SupplierOfferingsToolbar
                            state={state}
                            appliedChips={appliedChips}
                            resultStatus={
                                loadFailed
                                    ? "查询未完成"
                                    : loading
                                      ? "查询中…"
                                      : null
                            }
                        />
                    }
                    selectionBarOpen={canSelect && selectedItems.length > 0}
                    selectionBar={
                        canSelect ? (
                            <ListSelectionBar
                                idPrefix="supplier-offerings-selection"
                                resultCount={total}
                                pageRowCount={items.length}
                                selectedCount={selectedItems.length}
                                allPageSelected={
                                    items.length > 0 &&
                                    selectedItems.length === items.length
                                }
                                onSelectPage={selectPage}
                                onClear={clearSelection}
                                summaryExtra={
                                    (canBatchRevise || canBatchAvailability) &&
                                    batchDisplay.skippedCount > 0 ? (
                                        <span className="num">
                                            批量维护将跳过{" "}
                                            {batchDisplay.skippedCount}{" "}
                                            条接口供给
                                        </span>
                                    ) : null
                                }
                                actions={batchActions.map((action) => {
                                    const primary =
                                        action.key === primaryBatchAction
                                    return (
                                        <Button
                                            key={action.key}
                                            id={action.id}
                                            type="button"
                                            size="sm"
                                            variant={
                                                primary
                                                    ? "default"
                                                    : "secondary"
                                            }
                                            disabled={action.count === 0}
                                            title={
                                                action.count === 0
                                                    ? action.disabledReason
                                                    : undefined
                                            }
                                            onClick={action.run}
                                        >
                                            {primary ? (
                                                <action.Icon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                            ) : null}
                                            {action.label}
                                            {primary ? (
                                                <span className="num">
                                                    {action.count}
                                                </span>
                                            ) : null}
                                        </Button>
                                    )
                                })}
                            />
                        ) : undefined
                    }
                    tableClassName="flex flex-col max-md:[&_[data-column-id=sku-name]]:min-w-40 max-md:[&_[data-column-id=sku-name]]:max-w-40 max-sm:[&_[data-column-id=actions][data-column-pinned=right]]:right-auto! max-sm:[&_[data-column-id=actions][data-column-pinned=right]]:shadow-none!"
                    table={
                        <>
                            <SupplierOfferingsTable
                                rowSelection={rowSelection}
                                onRowSelectionChange={
                                    canSelect && !loadFailed
                                        ? onRowSelectionChange
                                        : undefined
                                }
                                onRowPreview={openPreview}
                                highlightedRowId={previewId ?? undefined}
                                canViewCosts={canViewCosts}
                                canRevise={canBatchRevise}
                                canAvailability={canBatchAvailability}
                                returnTo={returnTo}
                                total={loadFailed ? 0 : total}
                                items={items}
                                loading={loading}
                                isError={loadFailed}
                                error={query.error}
                                hasFilters={state.hasClearableFilters}
                                noScope={
                                    query.data?.empty_reason === "no_scope"
                                }
                                pagination={{
                                    pageIndex: listPage - 1,
                                    pageSize: PAGE_SIZE,
                                }}
                                onPaginationChange={onPaginationChange}
                                taskMode={state.taskMode}
                                taskBusinessObjectId={
                                    taskQuery.data?.businessObjectId
                                }
                                scopeChanged={scopeChanged && listPage > 1}
                                onRetry={() => {
                                    if (scopeChanged && listPage > 1)
                                        patchUrl({
                                            page: 1,
                                            scopeVersion: undefined,
                                        })
                                    else void query.refetch()
                                }}
                                onClearFilters={state.clearFilters}
                                onCreateOffering={
                                    canCreate ? openCreate : undefined
                                }
                                onUpdateAvailability={openAvailability}
                                onReviseOffering={openRevise}
                                onChangeStatus={openStatusChange}
                            />
                            {!loadFailed && query.data ? (
                                <SupplierOfferingsPagination
                                    page={listPage}
                                    pageSize={PAGE_SIZE}
                                    total={total}
                                    loading={loading}
                                    onPageChange={changePage}
                                />
                            ) : null}
                        </>
                    }
                />
            ) : null}

            <OfferingPreviewSheet
                offering={
                    showTable
                        ? (items.find((item) => item.id === previewId) ?? null)
                        : null
                }
                returnTo={returnTo}
                onClose={() => setPreviewId(null)}
                onClosed={restoreRowFocus}
            />
            {canAssignTemplate && assigning ? (
                <AssignDeliveryTemplateDialog
                    offerings={assigning}
                    onOpenChange={(open) => {
                        if (!open) setAssigning(null)
                    }}
                />
            ) : null}
            {batch &&
            (batch.mode === "revise"
                ? canBatchRevise
                : canBatchAvailability) ? (
                <BatchSupplyDialog
                    mode={batch.mode}
                    offerings={batch.offerings}
                    onClose={closeBatch}
                />
            ) : null}
            {canCreate ? (
                <RegisterSupplyForSkuDialog
                    key={
                        createOpen
                            ? `create-${urlState.skuId ?? "select"}`
                            : "closed"
                    }
                    open={createOpen}
                    onOpenChange={setCreateOpen}
                    fixedSku={
                        urlState.skuId
                            ? {
                                  skuId: urlState.skuId,
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
            {canBatchRevise && rowAction?.kind === "revise" ? (
                <ReviseOfferingDialog
                    key={rowAction.offering.id}
                    offering={rowAction.offering}
                    onOpenChange={onRowActionOpenChange}
                />
            ) : null}
            {canBatchAvailability && rowAction?.kind === "availability" ? (
                <UpdateAvailabilityDialog
                    key={rowAction.offering.id}
                    offering={rowAction.offering}
                    onOpenChange={onRowActionOpenChange}
                />
            ) : null}
            {canBatchRevise && rowAction?.kind === "status" ? (
                <ChangeOfferingStatusDialog
                    key={`${rowAction.offering.id}-${rowAction.intent.actionId}`}
                    offering={rowAction.offering}
                    intent={rowAction.intent}
                    onOpenChange={onRowActionOpenChange}
                />
            ) : null}
        </PageScaffold>
    )
}
