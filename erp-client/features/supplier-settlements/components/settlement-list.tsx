"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnPinningState } from "@tanstack/react-table"
import { ExternalLinkIcon, PlusIcon, RefreshCwIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FormalActionResult,
    GuardedBusinessAction,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { CreateDraftDialog } from "@/features/supplier-settlements/components/create-draft-dialog"
import { CrossEntryBanner } from "@/features/supplier-settlements/components/cross-entry-banner"
import { SettlementListEmptyState } from "@/features/supplier-settlements/components/settlement-list-empty"
import { SettlementMetricsStrip } from "@/features/supplier-settlements/components/settlement-list-metrics"
import { SettlementListPreviewSheet } from "@/features/supplier-settlements/components/settlement-list-preview"
import { SettlementListToolbar } from "@/features/supplier-settlements/components/settlement-list-toolbar"
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

    const canCreate = data?.hasModulePermission && data?.hasDataScope
    const listLoadFailed = listQuery.isError || !listQuery.data

    if (listQuery.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    const total = data?.total ?? 0
    const empty = data?.emptyReason

    return (
        <PageScaffold>
            <PageHeader
                title="API 供应商结算"
                metadata={
                    <DataFreshness
                        updatedAt={
                            data?.sourceAsOf
                                ? formatDateTime(data.sourceAsOf, "default")
                                : "—"
                        }
                        dateTime={data?.sourceAsOf}
                        label="结算数据更新时间"
                        state={
                            listLoadFailed
                                ? "failed"
                                : listQuery.isFetching
                                  ? "syncing"
                                  : "fresh"
                        }
                    />
                }
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="text-muted-foreground hover:text-foreground"
                            onClick={() => void listQuery.refetch()}
                        >
                            <RefreshCwIcon
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            刷新
                        </Button>
                        <div className="max-sm:hidden">
                            <GuardedBusinessAction
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
                                <PlusIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                新建结算草稿
                            </GuardedBusinessAction>
                        </div>
                    </div>
                }
            />

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
                                type="button"
                                size="sm"
                                render={<Link href={result.w12Href} />}
                            >
                                打开供应商往来应付
                                <ExternalLinkIcon className="size-3.5" />
                            </Button>
                        ) : null
                    }
                />
            ) : null}

            {data?.hasModulePermission && data.hasDataScope ? (
                <SettlementMetricsStrip
                    pendingReconcile={data.totals.pendingReconcile}
                    hasDifference={data.metrics.hasDifference}
                    pendingReview={data.metrics.pendingReview}
                    confirmedAmount={data.metrics.confirmedAmount}
                    urlState={urlState}
                    patchUrl={patchUrl}
                />
            ) : null}

            <Tabs
                value={urlState.view}
                onValueChange={(v) =>
                    patchUrl({
                        view: v as SettlementsUrlState["view"],
                        status: undefined,
                        differenceType: undefined,
                        page: 1,
                    })
                }
            >
                <TabsList>
                    {(
                        Object.keys(VIEW_LABEL) as Array<
                            keyof typeof VIEW_LABEL
                        >
                    ).map((k) => (
                        <TabsTrigger key={k} value={k}>
                            {VIEW_LABEL[k]}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>

            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        结算单列表
                        <span
                            aria-live="polite"
                            className="font-normal text-muted-foreground"
                        >
                            {total.toLocaleString("zh-CN")} 条
                        </span>
                    </span>
                }
                description={data?.filterSummary ?? "默认待处理"}
                toolbar={
                    <SettlementListToolbar
                        urlState={urlState}
                        suppliers={data?.suppliers ?? []}
                        searchInputRef={searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        panelOpen={filters.panelOpen}
                        setPanelOpen={filters.setPanelOpen}
                        hasActiveFilters={filters.hasActiveFilters}
                        applyFilters={filters.applyFilters}
                        removeFilter={filters.removeFilter}
                        resetMoreFilters={filters.resetMoreFilters}
                        clearAllFilters={filters.clearAllFilters}
                        supplierIdDraft={filters.supplierIdDraft}
                        setSupplierIdDraft={filters.setSupplierIdDraft}
                        statusDraft={filters.statusDraft}
                        setStatusDraft={filters.setStatusDraft}
                        differenceTypeDraft={filters.differenceTypeDraft}
                        setDifferenceTypeDraft={
                            filters.setDifferenceTypeDraft
                        }
                        periodFromDraft={filters.periodFromDraft}
                        setPeriodFromDraft={filters.setPeriodFromDraft}
                        periodToDraft={filters.periodToDraft}
                        setPeriodToDraft={filters.setPeriodToDraft}
                        periodError={filters.periodError}
                        setPeriodError={filters.setPeriodError}
                    />
                }
                table={
                    <DataTable
                        data={data?.rows ?? []}
                        columns={columns}
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
                                <SettlementListEmptyState
                                    empty={empty ?? "NO_STATEMENTS"}
                                    canCreate={Boolean(canCreate)}
                                    onClearFilters={filters.clearAllFilters}
                                    onCreateDraft={() => setCreateOpen(true)}
                                />
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
