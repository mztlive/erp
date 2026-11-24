"use client"

import * as React from "react"
import Link from "next/link"
import { ExternalLinkIcon, PlusIcon, RefreshCwIcon } from "lucide-react"

import {
    FormalActionResult,
    GuardedBusinessAction,
} from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import {
    ListDataTable,
    ListWorkspacePage,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { CreateDraftDialog } from "@/features/supplier-settlements/components/create-draft-dialog"
import { CrossEntryBanner } from "@/features/supplier-settlements/components/cross-entry-banner"
import { SettlementMetricsStrip } from "@/features/supplier-settlements/components/settlement-list-metrics"
import { SettlementListPreviewSheet } from "@/features/supplier-settlements/components/settlement-list-preview"
import { SettlementListToolbar } from "@/features/supplier-settlements/components/settlement-list-toolbar"
import { hasPermission } from "@/lib/permissions"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useSettlementListQuery } from "@/features/supplier-settlements/hooks/queries"
import { useSettlementListColumns } from "@/features/supplier-settlements/hooks/use-settlement-list-columns"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { useSettlementListState } from "@/features/supplier-settlements/hooks/use-settlement-list-state"
import { outcomeToResult } from "@/features/supplier-settlements/lib/operations"
import {
    joinSettlementStatusParam,
    parseSettlementStatusParam,
} from "@/features/supplier-settlements/lib/settlement-list-filters"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import { VIEW_LABEL } from "@/features/supplier-settlements/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

const TABLE_ID = "supplier-settlements-list-table"

function SettlementList({
    urlState,
    patchUrl,
    onOpen,
    returnTo,
}: {
    urlState: SettlementsUrlState
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
    onOpen: (statementId: string) => void
    returnTo?: string
}) {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [result, setResult] = React.useState<ResultState>(null)
    const lastFocusedRowId = React.useRef<string | null>(null)

    const profileQuery = useAccountProfileQuery()
    const listQuery = useSettlementListQuery({
        view: urlState.view,
        supplierId: urlState.supplierId,
        periodFrom: urlState.periodFrom,
        periodTo: urlState.periodTo,
        // 非法枚举值在解析时降级，不继续传给接口
        status: joinSettlementStatusParam(
            parseSettlementStatusParam(urlState.status),
        ),
        differenceType: urlState.differenceType,
        q: urlState.q,
        ownerUserIds: urlState.ownerUserIds,
        operatorUserIds: urlState.operatorUserIds,
        handlerUserIds: urlState.handlerUserIds,
        orgUnitIds: urlState.orgUnitIds,
        includeDescendants: urlState.includeDescendants,
        scopeVersion: urlState.scopeVersion,
        currentUserId: profileQuery.data?.userid,
        page: urlState.page,
        pageSize: 50,
    })

    const data = listQuery.data
    const filters = useSettlementListState(
        urlState,
        patchUrl,
        searchInputRef,
        hasPermission(profileQuery.data?.permissions, "business_person:list"),
        hasPermission(profileQuery.data?.permissions, "org_unit:list"),
    )
    const { pagination } = filters

    useSearchHotkey(searchInputRef)

    const previewRow =
        data?.rows.find((r) => r.statementId === urlState.preview) ?? null

    const openPreview = React.useCallback(
        (statementId: string) => {
            lastFocusedRowId.current = statementId
            patchUrl({ preview: statementId })
        },
        [patchUrl],
    )
    const columns = useSettlementListColumns(openPreview, onOpen)

    React.useEffect(() => {
        if (data?.scopeVersion && data.scopeVersion !== urlState.scopeVersion) {
            patchUrl({ scopeVersion: data.scopeVersion })
        }
    }, [data?.scopeVersion, patchUrl, urlState.scopeVersion])

    const canCreate = hasPermission(
        profileQuery.data?.permissions,
        "supplier_settlement_statement:create",
    )
    const canCreateInScope = Boolean(
        data?.hasModulePermission && data.emptyReason !== "NO_SCOPE",
    )
    const listLoadFailed = listQuery.isError || !listQuery.data

    const total = data?.total ?? 0
    const empty = data?.emptyReason

    return (
        <ListWorkspacePage
            title="供应商结算"
            className="min-h-0"
            loading={listQuery.isPending}
            headerActions={
                <>
                    <Button
                        id="supplier-settlements-list-refresh"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={listWorkspaceStyles.quietButton}
                        onClick={() => void listQuery.refetch()}
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        刷新
                    </Button>
                    {canCreate && (
                        <GuardedBusinessAction
                            id="supplier-settlements-list-create"
                            type="button"
                            size="sm"
                            disabled={!canCreateInScope}
                            reason={
                                canCreateInScope
                                    ? undefined
                                    : "当前没有可创建结算单的数据范围"
                            }
                            onClick={() => setCreateOpen(true)}
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            新建结算草稿
                        </GuardedBusinessAction>
                    )}
                </>
            }
            views={{
                ariaLabel: "供应商结算工作视图",
                items: (
                    Object.keys(VIEW_LABEL) as Array<keyof typeof VIEW_LABEL>
                ).map((item) => ({
                    id: `supplier-settlements-list-view-${toAutomationIdSegment(item)}`,
                    label: VIEW_LABEL[item],
                    count:
                        item === urlState.view && !listLoadFailed
                            ? total.toLocaleString("zh-CN")
                            : undefined,
                    active: item === urlState.view,
                    onClick: () =>
                        patchUrl({
                            view: item,
                            status: undefined,
                            differenceType: undefined,
                            page: 1,
                        }),
                })),
            }}
            alerts={
                <>
                    {returnTo ? <CrossEntryBanner returnTo={returnTo} /> : null}
                    {result ? (
                        <FormalActionResult
                            status={
                                result.status === "failed"
                                    ? "blocked"
                                    : result.status
                            }
                            title={result.title}
                            description={result.description}
                            reference={result.reference}
                            facts={result.facts}
                            actions={
                                result.w12Href ? (
                                    <Button
                                        id="supplier-settlements-list-result-w12"
                                        nativeButton={false}
                                        size="sm"
                                        render={<Link href={result.w12Href} />}
                                    >
                                        打开供应商往来应付
                                        <ExternalLinkIcon
                                            data-icon="inline-end"
                                            aria-hidden="true"
                                        />
                                    </Button>
                                ) : null
                            }
                        />
                    ) : null}
                </>
            }
            notices={
                data?.hasModulePermission && data.emptyReason !== "NO_SCOPE" ? (
                    <SettlementMetricsStrip
                        pendingReconcile={data.totals.pendingReconcile}
                        hasDifference={data.metrics.hasDifference}
                        pendingReview={data.metrics.pendingReview}
                        confirmedAmount={data.metrics.confirmedAmount}
                        sourceAsOf={
                            listLoadFailed || listQuery.isFetching
                                ? undefined
                                : data.sourceAsOf
                        }
                    />
                ) : null
            }
            surface={{
                ariaLabel: "供应商结算列表",
                tableClassName:
                    "max-md:[&_[data-column-id=statementNo]]:min-w-36",
                toolbar: (
                    <SettlementListToolbar
                        urlState={urlState}
                        suppliers={data?.suppliers ?? []}

                        searchInputRef={searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        panelOpen={filters.panelOpen}
                        setPanelOpen={filters.setPanelOpen}
                        applyFilters={filters.applyFilters}
                        removeFilter={filters.removeFilter}
                        resetMoreFilters={filters.resetMoreFilters}
                        cancelMoreFilters={filters.cancelMoreFilters}
                        clearAllFilters={filters.clearAllFilters}
                        supplierIdDraft={filters.supplierIdDraft}
                        setSupplierIdDraft={filters.setSupplierIdDraft}
                        statusDraft={filters.statusDraft}
                        setStatusDraft={filters.setStatusDraft}
                        differenceTypeDraft={filters.differenceTypeDraft}
                        setDifferenceTypeDraft={filters.setDifferenceTypeDraft}
                        periodFromDraft={filters.periodFromDraft}
                        setPeriodFromDraft={filters.setPeriodFromDraft}
                        periodToDraft={filters.periodToDraft}
                        setPeriodToDraft={filters.setPeriodToDraft}
                        ownerUserIdsDraft={filters.ownerUserIdsDraft}
                        setOwnerUserIdsDraft={filters.setOwnerUserIdsDraft}
                        operatorUserIdsDraft={filters.operatorUserIdsDraft}
                        setOperatorUserIdsDraft={
                            filters.setOperatorUserIdsDraft
                        }
                        handlerUserIdsDraft={filters.handlerUserIdsDraft}
                        setHandlerUserIdsDraft={filters.setHandlerUserIdsDraft}
                        orgUnitIdsDraft={filters.orgUnitIdsDraft}
                        setOrgUnitIdsDraft={filters.setOrgUnitIdsDraft}
                        includeDescendantsDraft={
                            filters.includeDescendantsDraft
                        }
                        setIncludeDescendantsDraft={
                            filters.setIncludeDescendantsDraft
                        }
                        periodError={filters.periodError}
                        setPeriodError={filters.setPeriodError}
                        hasPendingChanges={filters.hasPendingChanges}
                        loading={listQuery.isFetching}
                        failed={listQuery.isError}
                    />
                ),
                table: (
                    <ListDataTable
                        id={TABLE_ID}
                        caption="供应商结算列表"
                        entityName="结算单"
                        data={data?.rows ?? []}
                        columns={columns}
                        defaultColumnVisibility={{
                            supplier: false,
                            actors: false,
                        }}
                        defaultColumnPinning={{
                            left: ["statementNo"],
                            right: ["actions"],
                        }}
                        getRowId={(row) => row.statementId}
                        rowLabel={(row) =>
                            `${row.statementNo}（${row.supplierName}）`
                        }
                        rowCount={total}
                        pagination={pagination}
                        onPaginationChange={(next) => {
                            // 只写 URL，分页由 URL 派生，消除本地/URL 双写漂移
                            patchUrl({ page: next.pageIndex + 1 })
                        }}
                        manualPagination
                        scrollable
                        className="[&_[data-slot=data-table-surface]]:flex [&_[data-slot=data-table-surface]]:flex-col [&_[data-slot=table-container]]:min-h-0 [&_[data-slot=table-container]]:flex-1"
                        loading={listQuery.isFetching}
                        isError={listLoadFailed}
                        error={listQuery.error}
                        onRetry={() => void listQuery.refetch()}
                        retryButtonId="supplier-settlements-list-error-retry"
                        noScope={empty === "NO_SCOPE"}
                        hasActiveFilters={empty === "FILTER_NO_RESULT"}
                        onClearFilters={filters.clearAllFilters}
                        clearFiltersButtonId="supplier-settlements-list-empty-clear"
                        canCreate={canCreate && canCreateInScope}
                        onCreate={() => setCreateOpen(true)}
                        createButtonId="supplier-settlements-list-empty-create"
                        createLabel="新建结算草稿"
                        emptyCopy={{
                            noScopeTitle: "当前账号没有可查看的结算范围",
                            emptyTitle: "当前范围没有结算单",
                            emptyDescription:
                                canCreate && canCreateInScope
                                    ? "可调整筛选条件，或新建结算草稿。"
                                    : "可调整筛选条件后重新查询。",
                        }}
                        highlightedRowId={urlState.preview}
                        onRowPreview={(row) => openPreview(row.statementId)}
                        onRowOpen={(row) => onOpen(row.statementId)}
                    />
                ),
            }}
        >
            <SettlementListPreviewSheet
                open={Boolean(urlState.preview)}
                row={previewRow}
                onOpenChange={(open) => {
                    if (open) return
                    lastFocusedRowId.current ??= urlState.preview ?? null
                    patchUrl({ preview: undefined })
                }}
                finalFocus={() => {
                    const id = lastFocusedRowId.current
                    lastFocusedRowId.current = null
                    return (
                        (id
                            ? document.querySelector<HTMLElement>(
                                  `#${TABLE_ID} [data-row-id="${CSS.escape(id)}"]`,
                              )
                            : null) ?? searchInputRef.current
                    )
                }}
                onOpen={onOpen}
                patchUrl={patchUrl}
            />

            <CreateDraftDialog
                open={createOpen && canCreate && canCreateInScope}
                onOpenChange={setCreateOpen}
                onCreated={(outcome) => {
                    setResult(outcomeToResult(outcome))
                    if (outcome.status === "succeeded" && outcome.statementId) {
                        onOpen(outcome.statementId)
                    }
                }}
            />
        </ListWorkspacePage>
    )
}

export { SettlementList }
