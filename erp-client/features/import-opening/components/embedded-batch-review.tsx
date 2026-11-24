"use client"

import { useEffect, useState } from "react"
import {
    ChevronRightIcon,
    CircleHelpIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
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
import { cn } from "@/lib/utils"

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
                <div
                    role={severe ? "alert" : "status"}
                    className={cn(
                        "flex min-w-0 flex-wrap items-start justify-between gap-x-3 gap-y-1.5 rounded-r-md border-l-2 px-3 py-2",
                        severe
                            ? "border-l-destructive bg-destructive-soft text-destructive-soft-foreground"
                            : "border-l-warning bg-warning-soft text-warning-soft-foreground",
                    )}
                >
                    <div className="flex min-w-0 flex-[1_1_15rem] items-start gap-2 text-xs">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0"
                        />
                        <div className="min-w-0 space-y-1 wrap-anywhere">
                            {invalid > 0 || anomalies.length > 0 ? (
                                <p className="num font-medium">
                                    {totals
                                        ? `整批校验 · 无效 ${invalid} 项 · 警示 ${warning} 项`
                                        : `整批导入失败 ${invalid} 项`}
                                </p>
                            ) : null}
                            {blockers.length > 0 ? (
                                <ul className="space-y-0.5">
                                    {blockers.map((message) => (
                                        <li key={message}>{message}</li>
                                    ))}
                                </ul>
                            ) : null}
                        </div>
                    </div>
                    {anomalies.length > 0 ? (
                        <Button
                            id={`${idPrefix}-banner-locate`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            className="num text-inherit"
                            onClick={locateAnomaly}
                        >
                            仅看异常项（{anomalies.length}）
                            <ChevronRightIcon
                                aria-hidden="true"
                                data-icon="inline-end"
                            />
                        </Button>
                    ) : null}
                </div>
            ) : null}

            <TooltipProvider>
                <dl
                    aria-label="导入业务确认关键指标"
                    className="grid grid-cols-1 gap-x-4 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/import-task:grid-cols-2 @[840px]/import-task:grid-cols-4"
                >
                    <div className="min-w-0 space-y-1">
                        <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>本范围待确认条目</span>
                            <span className="text-tiny text-muted-foreground/80">
                                按归属统计
                            </span>
                            <Tooltip>
                                <TooltipTrigger
                                    id={`${idPrefix}-scope-kpi-help`}
                                    type="button"
                                    aria-label="查看待确认条目统计口径"
                                    className="inline-flex size-5 shrink-0 cursor-help items-center justify-center rounded-md hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                                >
                                    <CircleHelpIcon
                                        aria-hidden="true"
                                        className="size-3.5"
                                    />
                                </TooltipTrigger>
                                <TooltipContent>
                                    按来源数据类型归属统计当前待确认范围内的导入行。导入行明细未提供金额。
                                </TooltipContent>
                            </Tooltip>
                        </dt>
                        <dd className="num text-value-section font-semibold tracking-tight">
                            {pendingScopeRows ?? "—"}
                        </dd>
                        <dd className="text-xs text-muted-foreground">
                            涉及金额未提供
                        </dd>
                    </div>

                    <div className="min-w-0 space-y-1">
                        <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>整批校验通过 / 异常</span>
                            <span className="text-tiny text-muted-foreground/80">
                                整批明细
                            </span>
                            <Tooltip>
                                <TooltipTrigger
                                    id={`${idPrefix}-validation-kpi-help`}
                                    type="button"
                                    aria-label="查看整批校验统计口径"
                                    className="inline-flex size-5 shrink-0 cursor-help items-center justify-center rounded-md hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                                >
                                    <CircleHelpIcon
                                        aria-hidden="true"
                                        className="size-3.5"
                                    />
                                </TooltipTrigger>
                                <TooltipContent>
                                    按完整预检明细行统计：异常项 =
                                    无效（解析失败/映射冲突/导入失败）+
                                    警示（待解析/待映射/已跳过）。
                                </TooltipContent>
                            </Tooltip>
                        </dt>
                        <dd className="num text-value-compact font-semibold">
                            {totals
                                ? `${totals.valid} / ${anomalies.length}`
                                : "— / —"}
                        </dd>
                        {totals && rows && rows.length > 0 ? (
                            <>
                                <dd
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
                                </dd>
                                <dd>
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
                                </dd>
                            </>
                        ) : null}
                    </div>

                    <div className="min-w-0 space-y-1">
                        <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>部门责任确认</span>
                            <span className="text-tiny text-muted-foreground/80">
                                本轮确认
                            </span>
                            <Tooltip>
                                <TooltipTrigger
                                    id={`${idPrefix}-confirm-kpi-help`}
                                    type="button"
                                    aria-label="查看部门责任确认统计口径"
                                    className="inline-flex size-5 shrink-0 cursor-help items-center justify-center rounded-md hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                                >
                                    <CircleHelpIcon
                                        aria-hidden="true"
                                        className="size-3.5"
                                    />
                                </TooltipTrigger>
                                <TooltipContent>
                                    统计当前预检版本下所有需确认部门范围中，已完成确认的数量。
                                </TooltipContent>
                            </Tooltip>
                        </dt>
                        <dd className="num text-value-compact font-semibold">
                            已确认 {confirmed} / {currentConfirmations.length}
                        </dd>
                        {currentConfirmations.length > 0 ? (
                            <dd
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
                            </dd>
                        ) : null}
                        <dd className="text-xs text-muted-foreground">
                            本轮已登记范围
                        </dd>
                    </div>

                    <div className="min-w-0 space-y-1">
                        <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>批次环境与状态</span>
                            <span className="text-tiny text-muted-foreground/80">
                                目标环境
                            </span>
                        </dt>
                        <dd className="pt-0.5 text-body-compact font-semibold">
                            {ENVIRONMENT_LABEL[batch.environment]}
                        </dd>
                        <dd className="pt-0.5">
                            <BusinessStatusBadge
                                context="detail"
                                label={BATCH_STATUS_LABEL[batch.status]}
                                tone={BATCH_STATUS_TONE[batch.status]}
                            />
                        </dd>
                    </div>
                </dl>
            </TooltipProvider>

            <div
                className="flex flex-wrap gap-x-5 gap-y-1.5 text-xs text-muted-foreground wrap-anywhere"
                aria-label="批次资料"
            >
                <span>来源系统 {batch.sourceSystem.name}</span>
                <span>
                    基准日 <span className="num">{batch.baselineDate}</span>
                </span>
                <span>
                    导入规则{" "}
                    <span className="num">{batch.importRuleVersion}</span>
                </span>
                <span>
                    预检版本{" "}
                    <span className="num">
                        {confirmation?.trialVersion ?? batch.trialVersion}
                    </span>
                </span>
            </div>

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
