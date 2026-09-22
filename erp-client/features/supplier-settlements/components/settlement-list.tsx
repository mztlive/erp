"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnPinningState } from "@tanstack/react-table"
import { ExternalLinkIcon, PlusIcon, RefreshCwIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    FormalActionResult,
    GuardedBusinessAction,
    PageScaffold,
} from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { CreateDraftDialog } from "@/features/supplier-settlements/components/create-draft-dialog"
import { CrossEntryBanner } from "@/features/supplier-settlements/components/cross-entry-banner"
import { SettlementMetricsStrip } from "@/features/supplier-settlements/components/settlement-list-metrics"
import { SettlementListPreviewSheet } from "@/features/supplier-settlements/components/settlement-list-preview"
import { SettlementListToolbar } from "@/features/supplier-settlements/components/settlement-list-toolbar"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useSettlementListQuery } from "@/features/supplier-settlements/hooks/queries"
import { useSettlementListColumns } from "@/features/supplier-settlements/hooks/use-settlement-list-columns"
import { useSettlementListSearchHotkey } from "@/features/supplier-settlements/hooks/use-settlement-list-search-hotkey"
import { useSettlementListState } from "@/features/supplier-settlements/hooks/use-settlement-list-state"
import { outcomeToResult } from "@/features/supplier-settlements/lib/operations"
import {
    joinSettlementStatusParam,
    parseSettlementStatusParam,
} from "@/features/supplier-settlements/lib/settlement-list-filters"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import { VIEW_LABEL } from "@/features/supplier-settlements/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"

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
    const [columnPinning] = React.useState<ColumnPinningState>({
        left: ["statementNo"],
        right: ["actions"],
    })

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
    const filters = useSettlementListState(urlState, patchUrl, searchInputRef)
    const { pagination } = filters

    // 与加载/错误早返回无关，热键始终挂载（原行为：早返回前已注册）。
    useSettlementListSearchHotkey()

    const previewRow =
        data?.rows.find((r) => r.statementId === urlState.preview) ?? null

    const columns = useSettlementListColumns(patchUrl, onOpen)

    React.useEffect(() => {
        if (data?.scopeVersion && data.scopeVersion !== urlState.scopeVersion) {
            patchUrl({ scopeVersion: data.scopeVersion })
        }
    }, [data?.scopeVersion, patchUrl, urlState.scopeVersion])

    const canCreate =
        data?.hasModulePermission && data.emptyReason !== "NO_SCOPE"
    const listLoadFailed = listQuery.isError || !listQuery.data

    if (listQuery.isPending) {
        return (
            <PageScaffold
                density="compact"
                className={listWorkspaceStyles.page}
            >
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    const total = data?.total ?? 0
    const empty = data?.emptyReason

    return (
        <PageScaffold density="compact" className={listWorkspaceStyles.page}>
            <ListWorkspaceHeader
                className="pb-6 md:pb-6"
                eyebrow="供应商"
                title="供应商结算"
                description={
                    <>
                        查看结算单、对账差异与期间。
                        <span className="ml-3 text-xs" role="status">
                            {listLoadFailed ? (
                                "查询失败"
                            ) : listQuery.isFetching ? (
                                "正在更新…"
                            ) : data?.sourceAsOf ? (
                                <time dateTime={data.sourceAsOf}>
                                    更新于{" "}
                                    {formatDateTime(data.sourceAsOf, "default")}
                                </time>
                            ) : (
                                "正在查询"
                            )}
                        </span>
                    </>
                }
            >
                <div className="flex flex-wrap items-center gap-2">
                    <Button
                        id="supplier-settlements-list-refresh"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className="text-muted-foreground hover:text-foreground"
                        onClick={() => void listQuery.refetch()}
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        刷新
                    </Button>
                    <GuardedBusinessAction
                        id="supplier-settlements-list-create"
                        type="button"
                        size="sm"
                        disabled={!canCreate}
                        reason={
                            canCreate
                                ? undefined
                                : "当前账号无模块权限或数据范围"
                        }
                        onClick={() => setCreateOpen(true)}
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden="true" />
                        新建结算草稿
                    </GuardedBusinessAction>
                </div>
            </ListWorkspaceHeader>

            {returnTo ? <CrossEntryBanner returnTo={returnTo} /> : null}

            {result ? (
                <FormalActionResult
                    status={
                        result.status === "failed" ? "blocked" : result.status
                    }
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={result.facts}
                    actions={
                        result.w12Href ? (
                            <Button
                                id="supplier-settlements-list-result-w12"
                                type="button"
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

            {data?.hasModulePermission && data.emptyReason !== "NO_SCOPE" ? (
                <SettlementMetricsStrip
                    pendingReconcile={data.totals.pendingReconcile}
                    hasDifference={data.metrics.hasDifference}
                    pendingReview={data.metrics.pendingReview}
                    confirmedAmount={data.metrics.confirmedAmount}
                />
            ) : null}

            <ListWorkSurface
                toolbarClassName="pt-3 pb-2"
                ariaLabel="供应商结算列表"
                views={
                    <ListWorkspaceViews
                        ariaLabel="供应商结算工作视图"
                        items={(
                            Object.keys(VIEW_LABEL) as Array<
                                keyof typeof VIEW_LABEL
                            >
                        ).map((item) => ({
                            id: `supplier-settlements-list-view-${toAutomationIdSegment(item)}`,
                            label: VIEW_LABEL[item],
                            count:
                                item === urlState.view
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
                        }))}
                    />
                }
                toolbar={
                    <SettlementListToolbar
                        urlState={urlState}
                        suppliers={data?.suppliers ?? []}
                        ownerOptions={data?.ownerOptions ?? []}
                        operatorOptions={data?.operatorOptions ?? []}
                        handlerOptions={data?.handlerOptions ?? []}
                        searchInputRef={searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        panelOpen={filters.panelOpen}
                        setPanelOpen={filters.setPanelOpen}
                        applyFilters={filters.applyFilters}
                        removeFilter={filters.removeFilter}
                        resetMoreFilters={filters.resetMoreFilters}
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
                        resultCount={data?.total}
                        loading={listQuery.isFetching}
                        failed={listQuery.isError}
                    />
                }
                table={
                    <DataTable
                        id="supplier-settlements-list-table"
                        data={data?.rows ?? []}
                        columns={columns}
                        defaultColumnVisibility={{ actors: false }}
                        getRowId={(row) => row.statementId}
                        rowCount={total}
                        pagination={pagination}
                        onPaginationChange={(next) => {
                            // 只写 URL，分页由 URL 派生，消除本地/URL 双写漂移
                            patchUrl({ page: next.pageIndex + 1 })
                        }}
                        columnPinning={columnPinning}
                        enableColumnPinning
                        manualPagination
                        layout="flush"
                        loading={listQuery.isFetching}
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    title="结算列表加载失败"
                                    error={listQuery.error}
                                    action={
                                        <Button
                                            id="supplier-settlements-list-error-retry"
                                            type="button"
                                            onClick={() =>
                                                void listQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                        emptyState={
                            !listLoadFailed && total === 0 ? (
                                empty === "NO_SCOPE" ? (
                                    <BusinessEmptyState
                                        kind="no-scope"
                                        className={
                                            listWorkspaceEmptyStateClassName
                                        }
                                        title="当前账号没有可查看的结算范围"
                                        description="角色未配置结算数据范围时列表为空，不会回退为全公司。"
                                    />
                                ) : empty === "FILTER_NO_RESULT" ? (
                                    <BusinessEmptyState
                                        kind="filter"
                                        className={
                                            listWorkspaceEmptyStateClassName
                                        }
                                        title="当前筛选无结果"
                                        description="没有记录符合当前筛选条件，可清除筛选后重试。"
                                        action={
                                            <Button
                                                id="supplier-settlements-list-empty-clear"
                                                type="button"
                                                variant="secondary"
                                                className="rounded-lg shadow-none"
                                                onClick={
                                                    filters.clearAllFilters
                                                }
                                            >
                                                清除筛选
                                            </Button>
                                        }
                                    />
                                ) : (
                                    <BusinessEmptyState
                                        kind="no-data"
                                        className={
                                            listWorkspaceEmptyStateClassName
                                        }
                                        title="当前范围没有结算单"
                                        description="可选择供应商与期间后重查，或新建结算草稿。"
                                        action={
                                            canCreate ? (
                                                <Button
                                                    id="supplier-settlements-list-empty-create"
                                                    type="button"
                                                    onClick={() =>
                                                        setCreateOpen(true)
                                                    }
                                                >
                                                    新建结算草稿
                                                </Button>
                                            ) : null
                                        }
                                    />
                                )
                            ) : undefined
                        }
                        onRowPreview={(row) =>
                            patchUrl({ preview: row.statementId })
                        }
                        onRowOpen={(row) => onOpen(row.statementId)}
                    />
                }
            />

            <SettlementListPreviewSheet
                open={Boolean(urlState.preview)}
                row={previewRow}
                onOpenChange={(open) => {
                    if (!open) patchUrl({ preview: undefined })
                }}
                onOpen={onOpen}
                patchUrl={patchUrl}
            />

            <CreateDraftDialog
                open={createOpen}
                onOpenChange={setCreateOpen}
                onCreated={(outcome) => {
                    setResult(outcomeToResult(outcome))
                    if (outcome.status === "succeeded" && outcome.statementId) {
                        onOpen(outcome.statementId)
                    }
                }}
            />
        </PageScaffold>
    )
}

export { SettlementList }
