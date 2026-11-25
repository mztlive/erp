"use client"

import { useEffect, useState } from "react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    DecisionKpiBar,
    DecisionKpiCell,
    TaskRiskSummaryBar,
    WorkspaceFactGrid,
    WorkspaceFactItem,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    BATCH_STATUS_LABEL,
    BATCH_STATUS_TONE,
    CONFIRMATION_SCOPE_LABEL,
    ENVIRONMENT_LABEL,
    type ConfirmationScope,
    type ImportBatchView,
    type ImportConfirmationView,
} from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"
import { hasPermission } from "@/lib/permissions"

import { useBatchReviewQuery } from "./batch-review-data"
import {
    BatchReviewTable,
    batchReviewRowId,
    reviewPageSize,
} from "./batch-review-table"
import { EmbeddedConfirmActions } from "./embedded-confirm-actions"

const pulseDuration = 2200

const confirmationStatus = {
    PENDING: { label: "待确认", tone: "warning" },
    CONFIRMED: { label: "已确认", tone: "success" },
    REJECTED: { label: "已退回", tone: "destructive" },
    INVALIDATED: { label: "已失效", tone: "neutral" },
} as const

export function EmbeddedBatchReview({
    batch,
    scope,
    idPrefix,
    onRefreshBatch,
    onTaskCompleted,
}: {
    batch: ImportBatchView
    scope?: ConfirmationScope
    idPrefix: string
    onRefreshBatch: () => Promise<void>
    onTaskCompleted?: (workItemId: string) => void
}) {
    const { data: profile } = useAccountProfileQuery()
    const canReview = hasPermission(
        profile?.permissions,
        "legacy_import_row:list",
    )
    const canComplete = hasPermission(
        profile?.permissions,
        "legacy_import_confirmation:complete",
    )
    const review = useBatchReviewQuery(batch, canReview)
    const [onlyAnomalies, setOnlyAnomalies] = useState(false)
    const [search, setSearch] = useState("")
    const [page, setPage] = useState(1)
    const [highlight, setHighlight] = useState<{ rowId: string }>()
    const rows = canReview ? review.data?.rows : undefined
    const totals = canReview ? review.data?.totals : undefined
    const confirmation = batch.confirmations.find((entry) => entry.focused)
    const resolvedScope = confirmation?.scope ?? scope
    const currentConfirmations = batch.confirmations.filter(
        (entry) =>
            entry.trialVersion ===
            (confirmation?.trialVersion ?? batch.trialVersion),
    )
    const confirmed = currentConfirmations.filter(
        (entry) => entry.result === "CONFIRMED",
    ).length
    const scopeRows =
        rows && resolvedScope && rows.every((row) => row.scope)
            ? rows.filter((row) => row.scope === resolvedScope).length
            : undefined
    const pendingScopeRows =
        confirmation?.result === "PENDING" ? scopeRows : undefined

    const anomalies = rows?.filter((row) => row.result !== "valid") ?? []
    const term = search.trim().toLocaleLowerCase()
    const filteredRows = (onlyAnomalies ? anomalies : (rows ?? [])).filter(
        (row) =>
            !term ||
            [row.sourceKey, row.objectLabel, ...row.problems].some((value) =>
                value.toLocaleLowerCase().includes(term),
            ),
    )
    const currentPage = Math.min(
        page,
        Math.max(1, Math.ceil(filteredRows.length / reviewPageSize)),
    )
    const confirmationBlockers = batch.actionBlockers
        .filter((blocker) => blocker.action === "CONFIRM_SCOPE")
        .map((blocker) => blocker.message)
    const blockers = [
        ...new Set([
            ...confirmationBlockers,
            ...(confirmation?.result === "PENDING"
                ? (confirmation.workItem?.actionBlockers ?? [])
                : []),
            ...(batch.invalidation ? [batch.invalidation.reason] : []),
        ]),
    ]
    const blocked =
        !batch.productionGates.workItemTypeRegistered ||
        confirmationBlockers.length > 0
    if (blocked && blockers.length === 0) {
        blockers.push("责任确认任务不完整，请联系管理员。")
    }

    useEffect(() => {
        if (!highlight) return
        let frame2: number | null = null
        let animation: Animation | undefined
        const frame1 = requestAnimationFrame(() => {
            frame2 = requestAnimationFrame(() => {
                const target = document.getElementById(
                    batchReviewRowId(idPrefix, highlight.rowId),
                )
                const reducedMotion = window.matchMedia(
                    "(prefers-reduced-motion: reduce)",
                ).matches
                target?.focus({ preventScroll: true })
                target?.scrollIntoView({
                    behavior: reducedMotion ? "instant" : "smooth",
                    block: "center",
                    inline: "nearest",
                })
                animation = reducedMotion
                    ? undefined
                    : target?.animate(
                          [{ opacity: 1 }, { opacity: 0.65 }, { opacity: 1 }],
                          {
                              duration: pulseDuration / 2,
                              iterations: 2,
                              easing: "ease-in-out",
                          },
                      )
            })
        })
        const timer = window.setTimeout(
            () => setHighlight(undefined),
            pulseDuration,
        )
        return () => {
            cancelAnimationFrame(frame1)
            if (frame2 !== null) cancelAnimationFrame(frame2)
            animation?.cancel()
            window.clearTimeout(timer)
        }
    }, [highlight, idPrefix])

    function locateAnomaly() {
        const invalidIndex = anomalies.findIndex(
            (row) => row.result === "invalid",
        )
        const index = Math.max(0, invalidIndex)
        const row = anomalies[index]
        if (!row) return
        setOnlyAnomalies(true)
        setSearch("")
        setPage(Math.floor(index / reviewPageSize) + 1)
        setHighlight({ rowId: row.id })
    }

    function resetPage() {
        setPage(1)
        setHighlight(undefined)
    }

    const invalid = totals?.invalid ?? batch.metrics.failed
    const warning = totals?.warning
    const severe = invalid > 0 || blocked
    const unconfirmedCount = Math.max(
        0,
        currentConfirmations.length - confirmed,
    )
    return (
        <div className="min-w-0 space-y-3.5 bg-card py-3.5">
            {invalid > 0 || anomalies.length > 0 || blockers.length > 0 ? (
                <TaskRiskSummaryBar
                    role={severe ? "alert" : "status"}
                    severity={severe ? "destructive" : "warning"}
                    leadLabel={null}
                    summary={
                        <span className="inline-flex flex-col gap-1">
                            {invalid > 0 || anomalies.length > 0 ? (
                                <span className="num font-medium">
                                    {totals
                                        ? `整批校验 · 无效 ${invalid} 项 · 警示 ${warning} 项`
                                        : `整批导入失败 ${invalid} 项`}
                                </span>
                            ) : null}
                            {blockers.length > 0 ? (
                                <span className="flex flex-col gap-0.5">
                                    {blockers.map((message) => (
                                        <span key={message}>{message}</span>
                                    ))}
                                </span>
                            ) : null}
                        </span>
                    }
                    actions={
                        anomalies.length > 0
                            ? [
                                  {
                                      id: "locate",
                                      buttonId: `${idPrefix}-banner-locate`,
                                      label: "仅看异常项",
                                      count: anomalies.length,
                                      onClick: locateAnomaly,
                                  },
                              ]
                            : undefined
                    }
                />
            ) : null}

            <DecisionKpiBar
                columns={4}
                ariaLabel="导入业务确认关键指标"
                gridClassName="gap-x-4 gap-y-3"
            >
                <DecisionKpiCell
                    label="本范围待确认条目"
                    basis="按归属统计"
                    emphasis="primary"
                    formulaTooltipId={`${idPrefix}-scope-kpi-help`}
                    formulaTooltipLabel="查看待确认条目统计口径"
                    formulaTooltip="按来源数据类型归属统计当前待确认范围内的导入行。导入行明细未提供金额。"
                    value={pendingScopeRows ?? "—"}
                    footnote="涉及金额未提供"
                />

                <DecisionKpiCell
                    label="整批校验通过 / 异常"
                    basis="整批明细"
                    emphasis="secondary"
                    formulaTooltipId={`${idPrefix}-validation-kpi-help`}
                    formulaTooltipLabel="查看整批校验统计口径"
                    formulaTooltip="按完整预检明细行统计：异常项 = 无效（解析失败/映射冲突/导入失败）+ 警示（待解析/待映射/已跳过）。"
                    value={
                        totals
                            ? `${totals.valid} / ${anomalies.length}`
                            : "— / —"
                    }
                    extra={
                        totals && rows && rows.length > 0 ? (
                            <div className="space-y-1">
                                <div
                                    aria-hidden="true"
                                    className="flex h-1.5 overflow-hidden rounded-full bg-muted"
                                >
                                    {totals.valid > 0 ? (
                                        <span
                                            className="bg-success"
                                            style={{ flexGrow: totals.valid }}
                                        />
                                    ) : null}
                                    {totals.warning > 0 ? (
                                        <span
                                            className="bg-warning"
                                            style={{ flexGrow: totals.warning }}
                                        />
                                    ) : null}
                                    {totals.invalid > 0 ? (
                                        <span
                                            className="bg-destructive"
                                            style={{ flexGrow: totals.invalid }}
                                        />
                                    ) : null}
                                </div>
                                <ul className="flex flex-wrap gap-x-2.5 gap-y-0.5 text-tiny text-muted-foreground">
                                    <li className="inline-flex items-center gap-1">
                                        <span
                                            className="size-1.5 rounded-full bg-success"
                                            aria-hidden="true"
                                        />
                                        通过{" "}
                                        <span className="num">
                                            {totals.valid}
                                        </span>
                                    </li>
                                    <li className="inline-flex items-center gap-1">
                                        <span
                                            className="size-1.5 rounded-full bg-warning"
                                            aria-hidden="true"
                                        />
                                        警示{" "}
                                        <span className="num">
                                            {totals.warning}
                                        </span>
                                    </li>
                                    <li className="inline-flex items-center gap-1">
                                        <span
                                            className="size-1.5 rounded-full bg-destructive"
                                            aria-hidden="true"
                                        />
                                        无效{" "}
                                        <span className="num">
                                            {totals.invalid}
                                        </span>
                                    </li>
                                </ul>
                            </div>
                        ) : undefined
                    }
                />

                <DecisionKpiCell
                    label="部门责任确认"
                    basis="本轮确认"
                    emphasis="secondary"
                    formulaTooltipId={`${idPrefix}-confirm-kpi-help`}
                    formulaTooltipLabel="查看部门责任确认统计口径"
                    formulaTooltip="统计当前预检版本下所有需确认部门范围中，已完成确认的数量。"
                    value={`已确认 ${confirmed} / ${currentConfirmations.length}`}
                    extra={
                        currentConfirmations.length > 0 ? (
                            <div
                                aria-hidden="true"
                                className="flex h-1.5 overflow-hidden rounded-full bg-muted"
                            >
                                {confirmed > 0 ? (
                                    <span
                                        className="bg-success"
                                        style={{ flexGrow: confirmed }}
                                    />
                                ) : null}
                                {unconfirmedCount > 0 ? (
                                    <span
                                        className="bg-transparent"
                                        style={{ flexGrow: unconfirmedCount }}
                                    />
                                ) : null}
                            </div>
                        ) : undefined
                    }
                    footnote="本轮已登记范围"
                />

                <DecisionKpiCell
                    label="批次环境与状态"
                    basis="目标环境"
                    emphasis="terms"
                    value={ENVIRONMENT_LABEL[batch.environment]}
                    extra={
                        <BusinessStatusBadge
                            context="detail"
                            label={BATCH_STATUS_LABEL[batch.status]}
                            tone={BATCH_STATUS_TONE[batch.status]}
                        />
                    }
                />
            </DecisionKpiBar>

            <WorkspaceFactGrid columns={3} ariaLabel="批次资料">
                <WorkspaceFactItem
                    label="来源系统"
                    value={batch.sourceSystem.name}
                />
                <WorkspaceFactItem
                    label="基准日"
                    value={batch.baselineDate}
                    numeric
                />
                <WorkspaceFactItem
                    label="导入规则"
                    value={batch.importRuleVersion}
                    numeric
                />
                <WorkspaceFactItem
                    label="预检版本"
                    value={confirmation?.trialVersion ?? batch.trialVersion}
                    numeric
                />
            </WorkspaceFactGrid>

            {canReview && rows ? (
                <BatchReviewTable
                    idPrefix={idPrefix}
                    total={batch.metrics.total}
                    anomalyCount={anomalies.length}
                    rows={filteredRows.slice(
                        (currentPage - 1) * reviewPageSize,
                        currentPage * reviewPageSize,
                    )}
                    page={currentPage}
                    filteredCount={filteredRows.length}
                    onlyAnomalies={onlyAnomalies}
                    search={search}
                    highlightedRowId={highlight?.rowId}
                    onFilter={(only) => {
                        setOnlyAnomalies(only)
                        resetPage()
                    }}
                    onSearch={(value) => {
                        setSearch(value)
                        resetPage()
                    }}
                    onPage={(nextPage) => {
                        setPage(nextPage)
                        setHighlight(undefined)
                    }}
                />
            ) : null}
            {canReview && review.isPending ? (
                <p
                    role="status"
                    className="flex items-center gap-2 text-xs text-muted-foreground"
                >
                    <Spinner aria-hidden="true" className="size-3.5" />{" "}
                    正在加载批次核对明细…
                </p>
            ) : null}
            {canReview && review.isError ? (
                <BusinessFailureState
                    title="核对明细加载失败"
                    error={review.error}
                    action={
                        <Button
                            id={`${idPrefix}-retry-rows`}
                            type="button"
                            size="xs"
                            variant="outline"
                            onClick={() => void review.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            ) : null}

            <ConfirmationHistory
                idPrefix={idPrefix}
                confirmations={currentConfirmations}
            />

            <EmbeddedConfirmActions
                batch={batch}
                confirmation={confirmation}
                scope={resolvedScope}
                idPrefix={idPrefix}
                canComplete={canComplete}
                blocked={blocked}
                anomalyCount={anomalies.length}
                onLocateAnomaly={locateAnomaly}
                onRefresh={onRefreshBatch}
                onTaskCompleted={onTaskCompleted}
            />
        </div>
    )
}

function ConfirmationHistory({
    idPrefix,
    confirmations,
}: {
    idPrefix: string
    confirmations: readonly ImportConfirmationView[]
}) {
    if (!confirmations.length) return null

    return (
        <details className="border-t border-border/60 pt-2.5 text-xs">
            <summary
                id={`${idPrefix}-confirmation-history`}
                className="w-fit cursor-pointer font-medium text-foreground"
            >
                查看本轮部门确认状态
            </summary>
            <ul className="mt-1.5 divide-y divide-border/60">
                {confirmations.map((item) => (
                    <li
                        key={item.confirmationId}
                        className="space-y-1 py-2 wrap-anywhere"
                    >
                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                            <span className="font-medium">
                                {CONFIRMATION_SCOPE_LABEL[item.scope]}
                            </span>
                            <BusinessStatusBadge
                                context="detail"
                                {...confirmationStatus[item.result]}
                            />
                            {item.confirmedByLabel ? (
                                <span className="text-muted-foreground">
                                    {item.confirmedByLabel}
                                </span>
                            ) : null}
                            {item.confirmedAt ? (
                                <span className="num text-muted-foreground">
                                    {formatDateTime(
                                        item.confirmedAt,
                                        "dateStyle",
                                        "passthrough",
                                    )}
                                </span>
                            ) : null}
                        </div>
                        {item.comment ? (
                            <p className="text-muted-foreground">
                                {item.comment}
                            </p>
                        ) : null}
                    </li>
                ))}
            </ul>
        </details>
    )
}
