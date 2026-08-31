"use client"

import * as React from "react"
import Link from "next/link"
import type { PaginationState, RowSelectionState } from "@tanstack/react-table"
import { ShieldAlertIcon } from "lucide-react"

import { FormalActionResult, PageScaffold } from "@/components/business"
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
import {
    useExecutionProjectionColumns,
    type ProjectionRowCommandAction,
} from "@/features/execution-projections/hooks/use-execution-projection-columns"
import {
    useExecutionProjectionFilters,
    type ExecutionProjectionAppliedChip,
} from "@/features/execution-projections/hooks/use-execution-projection-filters"
import { commandToResultState } from "@/features/execution-projections/lib/result-state"
import type {
    BulkProjectionJob,
    DeliveryStatus,
    ExecutionProjectionMetricKey,
    ExecutionProjectionRow,
} from "@/features/execution-projections/types"
import {
    DELIVERY_STATUS_LABEL,
    LATENCY_LABEL,
    RECONCILIATION_LABEL,
    SOURCE_LABEL,
} from "@/features/execution-projections/types"
import { getErrorMessage } from "@/lib/api/errors"
import { openWorkspaceLabel, resultText } from "@/lib/ui-text"
import { type ResultState } from "@/components/business/feedback"

/** 指标快捷筛选的 chip 文案兜底；正常以服务端指标 label 为准。 */
const METRIC_CHIP_LABELS: Record<ExecutionProjectionMetricKey, string> = {
    pending_send: "待发送",
    inflight: "发送中",
    timeout: "已超时",
    fail_manual: "失败/转人工",
    acked: "已确认",
}

/** 接收状态可多值逗号拼接（如 未知+失败+转人工 组合值），chip 逐个映射为业务文案。 */
function deliveryStatusChipLabel(value: string): string {
    if (value === "UNKNOWN,FAILED,ESCALATED_MANUAL") return "未知+失败+转人工"
    return value
        .split(",")
        .map(
            (part) =>
                DELIVERY_STATUS_LABEL[part.trim() as DeliveryStatus] ??
                part.trim(),
        )
        .join("、")
}

export function ExecutionProjectionsPage() {
    const filters = useExecutionProjectionFilters()
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
    } = filters

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

    const appliedChips = React.useMemo<
        readonly ExecutionProjectionAppliedChip[]
    >(() => {
        const chips: ExecutionProjectionAppliedChip[] = []
        if (filters.q.trim()) {
            chips.push({ key: "q", label: `搜索：${filters.q.trim()}` })
        }
        if (filters.mallId !== "all") {
            const mallName = view?.malls.find(
                (mall) => mall.id === filters.mallId,
            )?.name
            chips.push({
                key: "mall",
                label: `商城：${mallName ?? filters.mallId}`,
            })
        }
        if (filters.deliveryStatus !== "all") {
            chips.push({
                key: "deliveryStatus",
                label: `接收状态：${deliveryStatusChipLabel(
                    filters.deliveryStatus,
                )}`,
            })
        }
        if (filters.latency !== "all") {
            chips.push({
                key: "latency",
                label: `等待时长：${LATENCY_LABEL[filters.latency]}`,
            })
        }
        if (filters.reconciliation !== "all") {
            chips.push({
                key: "reconciliation",
                label: `版本核对：${
                    RECONCILIATION_LABEL[filters.reconciliation]
                }`,
            })
        }
        if (filters.source !== "all") {
            chips.push({
                key: "source",
                label: `数据来源：${SOURCE_LABEL[filters.source]}`,
            })
        }
        if (filters.metric !== "all") {
            const metricLabel =
                view?.metrics.find((item) => item.key === filters.metric)
                    ?.label ?? METRIC_CHIP_LABELS[filters.metric]
            chips.push({ key: "metric", label: `指标：${metricLabel}` })
        }
        return chips
    }, [
        filters.deliveryStatus,
        filters.latency,
        filters.mallId,
        filters.metric,
        filters.q,
        filters.reconciliation,
        filters.source,
        view?.malls,
        view?.metrics,
    ])
    const hasChips = hasActiveFilters && appliedChips.length > 0

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
                description: getErrorMessage(err, "网络连接异常，请刷新后重试"),
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
                description: getErrorMessage(err, "操作未完成，请重试"),
                reference: "bulk",
                facts: [],
            })
            setPendingAction(null)
        }
    }

    const detail = detailQuery.data ?? undefined
    const objectOpen = Boolean(projectionId)

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
                                    id="execution-projections-result-w29"
                                    type="button"
                                    size="sm"
                                    render={
                                        <Link
                                            id="execution-projections-result-w29"
                                            href={result.w29Href}
                                        />
                                    }
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
                listLoading={listQuery.isFetching}
                listLoadFailed={listQuery.isError}
                queryError={listQuery.error}
                onRetry={() => void listQuery.refetch()}
                hasActiveFilters={hasActiveFilters}
                clearAllFilters={filters.clearAllFilters}
                filterSummary={view?.filterSummary}
                filters={filters}
                appliedChips={appliedChips}
                hasChips={hasChips}
                malls={view?.malls ?? []}
                replaceParams={replaceParams}
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
