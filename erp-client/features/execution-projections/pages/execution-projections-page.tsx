"use client"

import * as React from "react"
import Link from "next/link"
import type { PaginationState, RowSelectionState } from "@tanstack/react-table"
import { ShieldAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    FormalActionResult,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { BULK_SELECTION_LIMIT } from "@/features/execution-projections/api/projections"
import { BulkProjectionJobProgress } from "@/features/execution-projections/components/bulk-projection-job-progress"
import {
    ExecutionProjectionConfirmDialog,
    type PendingAction,
} from "@/features/execution-projections/components/execution-projection-confirm-dialog"
import { ExecutionProjectionDetailSheet } from "@/features/execution-projections/components/execution-projection-detail-sheet"
import { ExecutionProjectionListPanel } from "@/features/execution-projections/components/execution-projection-list-panel"
import { ExecutionProjectionMetricStrip } from "@/features/execution-projections/components/execution-projection-metric-strip"
import { ExecutionProjectionPageHeader } from "@/features/execution-projections/components/execution-projection-page-header"
import {
    useBulkProjectionCommandMutation,
    useExecutionProjectionDetailQuery,
    useExecutionProjectionListQuery,
    useProjectionDeliveryCommandMutation,
} from "@/features/execution-projections/hooks/queries"
import { useExecutionProjectionSearch } from "@/features/execution-projections/hooks/use-execution-projection-search"
import {
    useExecutionProjectionColumns,
    type ProjectionRowCommandAction,
} from "@/features/execution-projections/hooks/use-execution-projection-columns"
import { useExecutionProjectionUrlState } from "@/features/execution-projections/hooks/use-execution-projection-url-state"
import { commandToResultState } from "@/features/execution-projections/lib/result-state"
import type {
    BulkProjectionJob,
    ExecutionProjectionRow,
} from "@/features/execution-projections/types"
import { getErrorMessage } from "@/lib/api/errors"
import { openWorkspaceLabel, resultText } from "@/lib/ui-text"
import { type ResultState } from "@/components/business/feedback"

export function ExecutionProjectionsPage() {
    const {
        q,
        mallId,
        deliveryStatus,
        source,
        latency,
        reconciliation,
        metric,
        projectionId,
        revisionId,
        page,
        pageSize,
        hasActiveFilters,
        replaceParams,
        setPageState,
        clearFilters,
    } = useExecutionProjectionUrlState()

    const { searchDraft, setSearchDraft, searchInputRef } =
        useExecutionProjectionSearch({ q, replaceParams })

    const listQueryInput = React.useMemo(
        () => ({
            q: q || undefined,
            mallId: mallId === "all" ? undefined : mallId,
            deliveryStatus:
                deliveryStatus === "all" ? undefined : deliveryStatus,
            source,
            latency,
            reconciliation,
            metric,
            page,
            pageSize,
        }),
        [
            q,
            mallId,
            deliveryStatus,
            source,
            latency,
            reconciliation,
            metric,
            page,
            pageSize,
        ],
    )

    const listQuery = useExecutionProjectionListQuery(listQueryInput)
    const detailQuery = useExecutionProjectionDetailQuery(
        projectionId,
        revisionId,
    )
    const commandMutation = useProjectionDeliveryCommandMutation()
    const bulkMutation = useBulkProjectionCommandMutation()

    const [rowSelection, setRowSelection] = React.useState<RowSelectionState>(
        {},
    )
    const [result, setResult] = React.useState<ResultState>(null)
    const [bulkJob, setBulkJob] = React.useState<BulkProjectionJob | null>(null)
    const [pendingAction, setPendingAction] =
        React.useState<PendingAction>(null)
    const resultRef = React.useRef<HTMLDivElement>(null)

    const view = listQuery.data
    const rows = view?.rows ?? []
    const metrics = view?.metrics ?? []
    const total = view?.pageInfo.total ?? 0
    const selectedIds = React.useMemo(
        () => Object.keys(rowSelection).filter((id) => rowSelection[id]),
        [rowSelection],
    )
    const bulkOverLimit = selectedIds.length > BULK_SELECTION_LIMIT

    const pagination: PaginationState = {
        pageIndex: page - 1,
        pageSize,
    }

    React.useEffect(() => {
        if (result) {
            resultRef.current?.focus()
        }
    }, [result])

    const handleRowCommand = React.useCallback(
        (action: ProjectionRowCommandAction) => {
            setPendingAction(action)
        },
        [],
    )

    const columns = useExecutionProjectionColumns({
        replaceParams,
        commandPending: commandMutation.isPending,
        onRowCommand: handleRowCommand,
    })

    const openConfirmForRow = async (
        kind: "QUERY_RESULT" | "RETRY" | "ESCALATE",
        row: ExecutionProjectionRow,
        objectVersion: string,
    ) => {
        try {
            const detail =
                detailQuery.data?.identity.projectionId === row.projectionId
                    ? detailQuery.data
                    : null
            const version = detail?.objectVersion ?? objectVersion
            const result = await commandMutation.mutateAsync({
                projectionId: row.projectionId,
                projectionRevisionId: row.projectionRevisionId,
                deliveryId: row.delivery.deliveryId,
                action: kind,
                expectedObjectVersion: version,
                requestId: `req-${Date.now().toString(36)}`,
            })
            setResult(commandToResultState(result))
            setPendingAction(null)
        } catch (err) {
            const actionLabel =
                kind === "QUERY_RESULT"
                    ? "查询结果"
                    : kind === "RETRY"
                      ? "重试发送"
                      : "升级到接口错误中心"
            setResult({
                status: "blocked",
                title: resultText.operationBlocked,
                description: getErrorMessage(err, "请刷新后重试"),
                reference: row.projectionNo,
                facts: [
                    { label: "对象", value: row.salesOrderNo },
                    { label: "动作", value: actionLabel },
                ],
            })
            setPendingAction(null)
        }
    }

    const runBulk = async (kind: "BULK_QUERY" | "BULK_RETRY") => {
        try {
            const job = await bulkMutation.mutateAsync({
                action: kind,
                projectionIds: selectedIds,
                requestId: `bulk-${Date.now().toString(36)}`,
            })
            setBulkJob(job)
            setRowSelection({})
            setPendingAction(null)
            if (job.status === "failed") {
                setResult({
                    status: "blocked",
                    title: "批量操作被阻断",
                    description: job.nextAction,
                    reference: "bulk",
                    facts: [
                        {
                            label: "成功/跳过/失败/仍未知",
                            value: `${job.succeeded}/${job.skipped}/${job.failed}/${job.stillUnknown}`,
                        },
                    ],
                })
            }
        } catch (err) {
            setResult({
                status: "blocked",
                title: "批量操作被阻断",
                description: getErrorMessage(err, "请重试"),
                reference: "bulk",
                facts: [],
            })
            setPendingAction(null)
        }
    }

    const detail = detailQuery.data ?? undefined
    const objectOpen = Boolean(projectionId)

    if (listQuery.isPending && !view) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="执行信息" description="正在加载列表…" />
                <div className="space-y-3" aria-busy="true" aria-label="加载中">
                    <div className="h-20 animate-pulse rounded-lg bg-muted" />
                    <div className="h-64 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (listQuery.isError) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="执行信息" description="列表加载失败" />
                <BusinessFailureState
                    error={listQuery.error}
                    onRetry={() => void listQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact">
            <ExecutionProjectionPageHeader
                queriedAt={view?.queriedAt}
                isFetching={listQuery.isFetching}
                onRefresh={() => void listQuery.refetch()}
                selectedCount={selectedIds.length}
                bulkOverLimit={bulkOverLimit}
                bulkPending={bulkMutation.isPending}
                onBulkQuery={() =>
                    setPendingAction({ kind: "BULK_QUERY", ids: selectedIds })
                }
                onBulkRetry={() =>
                    setPendingAction({ kind: "BULK_RETRY", ids: selectedIds })
                }
            />

            <Alert>
                <ShieldAlertIcon aria-hidden="true" />
                <AlertTitle>本页只读</AlertTitle>
                <AlertDescription>
                    执行信息由已生效销售版本自动形成；接收失败不影响销售记录与应收，内容变更须走销售变更单。本页支持查询结果、重试与升级到接口错误中心，不展示金额、税率、开票、应收与玩法等销售明细。
                </AlertDescription>
            </Alert>

            <div ref={resultRef} tabIndex={-1} className="outline-none">
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
                            result.w29Href ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    render={<Link href={result.w29Href} />}
                                >
                                    {openWorkspaceLabel("W29")}
                                </Button>
                            ) : null
                        }
                    />
                ) : null}
            </div>

            {bulkJob ? <BulkProjectionJobProgress job={bulkJob} /> : null}

            <ExecutionProjectionMetricStrip
                metrics={metrics}
                metric={metric}
                replaceParams={replaceParams}
            />

            <ExecutionProjectionListPanel
                rows={rows}
                columns={columns}
                total={total}
                rowSelection={rowSelection}
                onRowSelectionChange={setRowSelection}
                pagination={pagination}
                onPaginationChange={setPageState}
                hasActiveFilters={hasActiveFilters}
                clearFilters={clearFilters}
                filterSummary={view?.filterSummary}
                replaceParams={replaceParams}
                searchInputRef={searchInputRef}
                searchDraft={searchDraft}
                onSearchDraftChange={setSearchDraft}
                mallId={mallId}
                deliveryStatus={deliveryStatus}
                latency={latency}
                reconciliation={reconciliation}
                source={source}
                malls={view?.malls ?? []}
                selectedCount={selectedIds.length}
                bulkOverLimit={bulkOverLimit}
                bulkPending={bulkMutation.isPending}
                onClearSelection={() => setRowSelection({})}
                onBulkQuery={() =>
                    setPendingAction({ kind: "BULK_QUERY", ids: selectedIds })
                }
                onBulkRetry={() =>
                    setPendingAction({ kind: "BULK_RETRY", ids: selectedIds })
                }
            />

            <p className="text-xs text-muted-foreground">
                结果未知不计入「已确认」指标。
                {view?.defaultViewNote}
            </p>

            <ExecutionProjectionDetailSheet
                open={objectOpen}
                onOpenChange={(open) => {
                    if (!open)
                        replaceParams({ projectionId: null, revision: null })
                }}
                detail={detail}
                isPending={detailQuery.isPending}
                isError={detailQuery.isError}
                error={detailQuery.error}
                onRetry={() => void detailQuery.refetch()}
                rows={rows}
                replaceParams={replaceParams}
                commandPending={commandMutation.isPending}
                onRequestRowCommand={(action) => {
                    setPendingAction({
                        kind: action.kind,
                        row: action.row,
                        objectVersion: action.objectVersion,
                    })
                }}
            />

            <ExecutionProjectionConfirmDialog
                pendingAction={pendingAction}
                onOpenChange={() => setPendingAction(null)}
                pending={commandMutation.isPending || bulkMutation.isPending}
                onConfirm={async () => {
                    const action = pendingAction
                    if (!action) return
                    if (
                        action.kind === "BULK_QUERY" ||
                        action.kind === "BULK_RETRY"
                    ) {
                        await runBulk(action.kind)
                        return
                    }
                    await openConfirmForRow(
                        action.kind,
                        action.row,
                        action.objectVersion,
                    )
                }}
            />
        </PageScaffold>
    )
}
