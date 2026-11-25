"use client"

import * as React from "react"
import { flushSync } from "react-dom"
import { useSelector } from "@tanstack/react-form"
import { CircleAlertIcon, SearchIcon } from "lucide-react"

import {
    ReasonTextareaField,
    TaskRiskSummaryBar,
    WorkspaceTaskFooter,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import { parseDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { utf8ByteLength } from "@/lib/validation/reason-length"
import { isWorkspaceFrameActive } from "@/lib/workspace-hotkeys"
import type { SalesOrderDetailView } from "../api/contracts"
import type { AcceptanceFormApi } from "../hooks/use-acceptance-form"
import type { AcceptanceSelectionApi } from "../hooks/use-acceptance-selection"
import {
    acceptanceConfirmLines,
    buildDraftLines,
    collectValidationIssues,
    defaultBatchDraft,
    exceptionQuantityLabel,
    type AcceptanceBatchSelection,
    type AcceptanceOrderProgress,
    type FormalResultState,
} from "../lib/acceptance-model"
import {
    FACT_ONLY_NOTICE,
    type AcceptanceEligibleFact,
} from "../lib/acceptance-types"
import { AcceptanceConfirmSelection } from "./acceptance-dialogs"
import {
    AcceptanceTaskFooterTotals,
    AcceptanceTaskKpis,
    buildAcceptanceTaskTotals,
} from "./acceptance-task-totals"

import {
    ACCEPTANCE_REASON_MAX_BYTES,
    AcceptanceTaskTable,
    acceptanceTaskRowId,
    type AcceptanceTaskIssue,
} from "./acceptance-task-table"

const evidenceTypes = [
    "application/pdf",
    "image/jpeg",
    "image/png",
    "image/webp",
]
const maxEvidenceSize = 5 * 1024 * 1024
const FILTER_THRESHOLD = 8
const PAGE_SIZE = 20
type Filter = "all" | "issues" | "included" | "skipped"

export function AcceptanceTaskRegister({
    idPrefix,
    facts,
    progress,
    referenceOrder,
    selection,
    confirmSelected,
    form,
    blocker,
    canPost,
    busy,
    reviewing,
    pendingOutcome,
    idempotencyKey,
    result,
    onConfirm,
    onBack,
    relatedSection,
}: {
    idPrefix: string
    facts: AcceptanceEligibleFact[]
    progress: AcceptanceOrderProgress
    referenceOrder?: SalesOrderDetailView | null
    selection: AcceptanceSelectionApi
    confirmSelected?: AcceptanceBatchSelection
    form: AcceptanceFormApi
    blocker?: string
    canPost: boolean
    busy: boolean
    reviewing: boolean
    pendingOutcome: boolean
    idempotencyKey?: string
    result: FormalResultState | null
    onConfirm: () => Promise<void>
    onBack: () => void
    relatedSection?: { id: string; label: string }
}) {
    const [checked, setChecked] = React.useState<Set<string>>(() => new Set())
    const [filter, setFilter] = React.useState<Filter>("all")
    const [search, setSearch] = React.useState("")
    const [page, setPage] = React.useState(0)
    const [highlight, setHighlight] = React.useState<string>()
    const pulseTimer = React.useRef<ReturnType<typeof setTimeout> | undefined>(
        undefined,
    )
    const values = useSelector(form.store, (state) => state.values)
    const readOnly = !canPost || busy || pendingOutcome || reviewing
    const rowId = (factId: string) => acceptanceTaskRowId(idPrefix, factId)
    const tableId = `${idPrefix}-lines`
    const dateId = `${idPrefix}-date`
    const fileId = `${idPrefix}-evidence`
    const commentId = `${idPrefix}-comment`
    const blockerId = `${idPrefix}-blocker`
    const reviewId = `${idPrefix}-review`

    React.useEffect(() => () => clearTimeout(pulseTimer.current), [])
    React.useEffect(() => {
        if (reviewing) document.getElementById(reviewId)?.focus()
    }, [reviewing, reviewId])

    const totals = buildAcceptanceTaskTotals(
        progress,
        facts,
        selection.selected,
        referenceOrder,
    )
    const issues: AcceptanceTaskIssue[] = blocker
        ? [{ target: blockerId, message: blocker }]
        : []
    if (canPost && facts.length && !pendingOutcome) {
        const validDrafts = new Map(selection.selected)
        for (const [factId, draft] of selection.selected) {
            const fields =
                draft.result === "PASS"
                    ? ([["qty", draft.qty]] as const)
                    : ([
                          ["qty", draft.qty],
                          ["exc", draft.exceptionQty],
                      ] as const)
            for (const [field, value] of fields) {
                try {
                    parseDecimal(value, { maxScale: 6 })
                } catch {
                    validDrafts.delete(factId)
                    issues.push({
                        factId,
                        target: `${rowId(factId)}-${field}`,
                        message: `${field === "qty" ? "本次数量" : exceptionQuantityLabel(draft.result)}请填写最多 6 位小数的非负数。`,
                    })
                }
            }
        }
        for (const issue of collectValidationIssues(validDrafts)) {
            if (issue.id === "no-source" && selection.selected.size > 0)
                continue
            const field = issue.targetId?.match(/^batch-(qty|exc|reason)-(.+)$/)
            if (field) {
                const [, name, factId] = field
                issues.push({
                    target: `${rowId(factId)}-${name}`,
                    message: issue.message,
                    factId,
                })
            } else if (issue.id.startsWith("line-pass-")) {
                const lineId = issue.id.slice("line-pass-".length)
                const draft = [...selection.selected.values()].find(
                    ({ fact }) => fact.salesOrderLineId === lineId,
                )
                if (draft) {
                    const factId = draft.fact.fulfillmentLineId
                    issues.push({
                        target: `${rowId(factId)}-qty`,
                        message: "该商品至少需要填写一笔通过数量。",
                        factId,
                    })
                }
            } else {
                issues.push({
                    target: facts[0]
                        ? `${rowId(facts[0].fulfillmentLineId)}-qty`
                        : tableId,
                    factId: facts[0]?.fulfillmentLineId,
                    message: "请为至少一批交付记录填写本次数量。",
                })
            }
        }

        for (const line of buildDraftLines(validDrafts)) {
            if (utf8ByteLength(line.reason) <= ACCEPTANCE_REASON_MAX_BYTES)
                continue
            const draft = [...selection.selected.values()].find(
                ({ fact, reason }) =>
                    fact.salesOrderLineId === line.salesOrderLineId &&
                    reason.trim(),
            )
            if (draft) {
                const factId = draft.fact.fulfillmentLineId
                issues.push({
                    target: `${rowId(factId)}-reason`,
                    factId,
                    message: "同一销售单行的异常原因合计不能超过 512 字节。",
                })
            }
        }

        if (
            !values.acceptedAt ||
            !Number.isFinite(Date.parse(values.acceptedAt))
        ) {
            issues.push({
                target: dateId,
                message: "请填写有效的客户验收时间。",
            })
        }
        const evidence = values.evidenceFile
        if (
            !evidence ||
            !evidenceTypes.includes(evidence.type) ||
            evidence.size === 0 ||
            evidence.size > maxEvidenceSize
        ) {
            issues.push({
                target: `${fileId}-input`,
                message:
                    "请上传不超过 5 MB 的 PDF、JPG、PNG 或 WebP 签收单凭证。",
            })
        }
        if (utf8ByteLength(values.comment) > ACCEPTANCE_REASON_MAX_BYTES) {
            issues.push({
                target: commentId,
                message: "内部备注不能超过 512 字节。",
            })
        }
    }

    const issuesByFact = new Map<string, AcceptanceTaskIssue[]>()
    for (const issue of issues) {
        if (!issue.factId) continue
        const rowIssues = issuesByFact.get(issue.factId) ?? []
        rowIssues.push(issue)
        issuesByFact.set(issue.factId, rowIssues)
    }

    const dateIssue = issues.find((issue) => issue.target === dateId)
    const fileIssue = issues.find((issue) => issue.target === `${fileId}-input`)
    const commentIssue = issues.find((issue) => issue.target === commentId)

    const locate = () => {
        const issue = issues[0]
        if (!issue) return
        const factIndex = issue.factId
            ? facts.findIndex((fact) => fact.fulfillmentLineId === issue.factId)
            : -1
        const targetPage =
            factIndex >= 0 ? Math.floor(factIndex / PAGE_SIZE) : 0
        flushSync(() => {
            setFilter("all")
            setSearch("")
            if (factIndex >= 0) setPage(targetPage)
        })
        clearTimeout(pulseTimer.current)
        const target = issue.factId ? rowId(issue.factId) : issue.target
        setHighlight(target)
        requestAnimationFrame(() => {
            const field = document.getElementById(issue.target)
            const row = document.getElementById(target)
            const details = field?.closest("details")
            if (details) details.open = true
            const anchor = row ?? field
            anchor?.scrollIntoView({ block: "center", behavior: "smooth" })
            field?.focus({ preventScroll: true })
        })
        pulseTimer.current = setTimeout(() => setHighlight(undefined), 2200)
    }
    const pulse = (id: string) =>
        highlight === id &&
        "bg-info-soft/50 ring-1 ring-inset ring-info-border motion-safe:animate-pulse"

    const submitShortcutRef = React.useRef<() => void>(() => undefined)
    submitShortcutRef.current = () => {
        if (busy) return
        if (pendingOutcome) {
            void onConfirm()
            return
        }
        if (reviewing) {
            if (canPost && issues.length === 0) void onConfirm()
            return
        }
        if (!canPost || facts.length === 0) return
        if (issues.length > 0) locate()
        else void form.handleSubmit()
    }

    React.useEffect(() => {
        const handleKeyDown = (event: KeyboardEvent) => {
            if (
                !isWorkspaceFrameActive() ||
                event.defaultPrevented ||
                event.isComposing ||
                event.keyCode === 229
            ) {
                return
            }
            if (!(event.ctrlKey || event.metaKey) || event.altKey) return
            if (event.key !== "Enter") return
            if (
                document.querySelector(
                    '[role="dialog"]:not([data-closed]):not([data-slot="toast"]):not([data-business-component="task-action-sheet"]), [role="alertdialog"]:not([data-closed])',
                )
            ) {
                return
            }
            event.preventDefault()
            submitShortcutRef.current()
        }
        window.addEventListener("keydown", handleKeyDown)
        return () => window.removeEventListener("keydown", handleKeyDown)
    }, [])

    const showFilters =
        facts.length > FILTER_THRESHOLD ||
        issuesByFact.size > 0 ||
        filter !== "all" ||
        Boolean(search.trim())

    const matching = showFilters
        ? facts.filter((fact) => {
              const id = fact.fulfillmentLineId
              if (filter === "issues" && !issuesByFact.has(id)) return false
              if (filter === "included" && !selection.selected.has(id))
                  return false
              if (filter === "skipped" && selection.selected.has(id))
                  return false
              return `${fact.itemSnapshot} ${fact.fulfillmentNo}`
                  .toLocaleLowerCase()
                  .includes(search.trim().toLocaleLowerCase())
          })
        : facts

    const pageCount = Math.max(1, Math.ceil(matching.length / PAGE_SIZE))
    const currentPage = Math.min(page, pageCount - 1)
    const pageRows = matching.slice(
        currentPage * PAGE_SIZE,
        (currentPage + 1) * PAGE_SIZE,
    )

    const checkedFacts = facts.filter((fact) =>
        checked.has(fact.fulfillmentLineId),
    )
    const checkedCount = checkedFacts.length
    const setCheckedFacts = (
        rows: readonly AcceptanceEligibleFact[],
        next: boolean,
    ) =>
        setChecked((current) => {
            const changed = new Set(current)
            for (const fact of rows) {
                if (next) changed.add(fact.fulfillmentLineId)
                else changed.delete(fact.fulfillmentLineId)
            }
            return changed
        })

    const fill = (rows: readonly AcceptanceEligibleFact[]) => {
        const next = new Map(selection.selected)
        for (const fact of rows) {
            const existing = next.get(fact.fulfillmentLineId)
            next.set(fact.fulfillmentLineId, {
                ...(existing ?? defaultBatchDraft(fact)),
                fact,
                qty: fact.eligibleQuantity,
            })
        }
        selection.replace(next)
    }
    const clear = (rows: readonly AcceptanceEligibleFact[]) => {
        const next = new Map(selection.selected)
        for (const fact of rows) next.delete(fact.fulfillmentLineId)
        selection.replace(next)
    }
    const updateQuantity = (fact: AcceptanceEligibleFact, value: string) => {
        const id = fact.fulfillmentLineId
        if (!value.trim()) {
            selection.skipFact(id)
        } else {
            const next = new Map(selection.selected)
            next.set(id, {
                ...(next.get(id) ?? defaultBatchDraft(fact)),
                qty: value,
            })
            selection.replace(next)
        }
    }

    const openRelatedSection = () => {
        if (!relatedSection) return
        const section = document.getElementById(relatedSection.id)
        if (section instanceof HTMLDetailsElement) section.open = true
        section?.scrollIntoView({ block: "start", behavior: "smooth" })
        section?.querySelector("summary")?.focus({ preventScroll: true })
    }
    const summary = <AcceptanceTaskFooterTotals totals={totals} />
    const footerActions = (
        <>
            {issues.length > 0 && !reviewing && !pendingOutcome ? (
                <Button
                    id={`${idPrefix}-footer-locate`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={locate}
                >
                    {issues.length} 项待完善 · 定位
                </Button>
            ) : null}
            {reviewing && !pendingOutcome ? (
                <Button
                    id={`${idPrefix}-edit`}
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={busy}
                    onClick={onBack}
                >
                    返回修改
                </Button>
            ) : relatedSection ? (
                <Button
                    id={`${idPrefix}-related-jump`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={openRelatedSection}
                >
                    {relatedSection.label}
                </Button>
            ) : null}
            {pendingOutcome || reviewing ? (
                <LoadingButton
                    id={`${idPrefix}-confirm`}
                    type="button"
                    variant="default"
                    size="sm"
                    loading={busy}
                    disabled={
                        busy ||
                        (!pendingOutcome && (!canPost || issues.length > 0))
                    }
                    onClick={() => void onConfirm()}
                >
                    {pendingOutcome ? "核对提交结果" : "提交本次验收"}
                </LoadingButton>
            ) : canPost && facts.length > 0 ? (
                <Button
                    id={`${idPrefix}-review-submit`}
                    type="submit"
                    form={`${idPrefix}-form`}
                    variant="default"
                    size="sm"
                    disabled={busy}
                >
                    核对本次验收
                </Button>
            ) : null}
        </>
    )

    const effectiveConfirmSelection = confirmSelected ?? selection.selected

    return (
        <div className="flex min-w-0 flex-col gap-3 bg-card py-3">
            {pendingOutcome ? (
                <div
                    role="status"
                    className="flex min-w-0 items-start gap-2 rounded-r-md border-l-2 border-l-warning bg-warning-soft px-3 py-2 text-xs text-warning-soft-foreground"
                >
                    <CircleAlertIcon
                        aria-hidden="true"
                        className="mt-0.5 size-4 shrink-0"
                    />
                    <div className="min-w-0 space-y-0.5 wrap-anywhere">
                        <p className="font-semibold">验收提交结果待确认</p>
                        <p>
                            请点击「核对提交结果」，按原提交内容与原凭证查询结果，避免重复登记。
                        </p>
                        {result?.description ? (
                            <p>{result.description}</p>
                        ) : null}
                        {idempotencyKey ? (
                            <p className="num select-all text-tiny">
                                原任务号：{idempotencyKey}
                            </p>
                        ) : null}
                    </div>
                </div>
            ) : issues.length > 0 ? (
                <TaskRiskSummaryBar
                    id={blockerId}
                    tabIndex={-1}
                    role={blocker ? "alert" : "status"}
                    severity={blocker ? "destructive" : "warning"}
                    leadLabel={
                        blocker
                            ? "当前存在办理限制"
                            : `${issues.length} 项待完善`
                    }
                    summary={issues[0].message}
                    actions={
                        !blocker
                            ? [
                                  {
                                      id: "locate",
                                      buttonId: `${idPrefix}-issue-locate`,
                                      label: "定位",
                                      onClick: locate,
                                  },
                              ]
                            : undefined
                    }
                    className={cn("outline-none", pulse(blockerId))}
                />
            ) : null}

            <AcceptanceTaskKpis
                totals={totals}
                idPrefix={idPrefix}
                fileName={values.evidenceFile?.name}
                acceptedAt={values.acceptedAt}
            />

            {reviewing || pendingOutcome ? (
                <section
                    id={reviewId}
                    tabIndex={-1}
                    className="space-y-2 outline-none"
                    aria-label="核对本次验收"
                >
                    <h2 className="text-section-title font-semibold">
                        核对本次验收
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        验收时间{" "}
                        <span className="num">
                            {values.acceptedAt.replace("T", " ")}
                        </span>
                        {" · "}签收单凭证{" "}
                        <span className="wrap-anywhere">
                            {values.evidenceFile?.name}
                        </span>
                    </p>
                    {values.comment.trim() ? (
                        <p className="wrap-anywhere text-xs text-muted-foreground">
                            内部备注：{values.comment.trim()}
                        </p>
                    ) : null}
                    {selection.hasExceptionResult ? (
                        <p className="text-xs text-warning-soft-foreground">
                            {FACT_ONLY_NOTICE}
                        </p>
                    ) : null}
                    <AcceptanceConfirmSelection
                        lines={acceptanceConfirmLines(
                            effectiveConfirmSelection,
                        )}
                    />
                </section>
            ) : facts.length > 0 ? (
                <form
                    id={`${idPrefix}-form`}
                    className="flex min-w-0 flex-col gap-3"
                    onSubmit={(event) => {
                        event.preventDefault()
                        if (readOnly) return
                        if (issues.length > 0) locate()
                        else void form.handleSubmit()
                    }}
                >
                    <div
                        className={cn(
                            "flex min-h-10 min-w-0 flex-wrap items-center justify-between gap-2 rounded-md border px-2 py-1 transition-colors",
                            checkedCount > 0
                                ? "border-info-border/70 bg-info-soft/50"
                                : "border-transparent px-0",
                        )}
                    >
                        {checkedCount > 0 ? (
                            <>
                                <div className="flex flex-wrap items-center gap-2">
                                    <span className="num text-xs font-medium text-info-soft-foreground">
                                        已勾选 {checkedCount} 行
                                    </span>
                                    {checkedCount < matching.length ? (
                                        <Button
                                            id={`${idPrefix}-check-all`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            onClick={() =>
                                                setCheckedFacts(matching, true)
                                            }
                                        >
                                            全选 ({matching.length})
                                        </Button>
                                    ) : null}
                                    <Button
                                        id={`${idPrefix}-uncheck`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={() => setChecked(new Set())}
                                    >
                                        取消勾选
                                    </Button>
                                </div>
                                <div className="flex flex-wrap items-center gap-1">
                                    <Button
                                        id={`${idPrefix}-fill-checked`}
                                        type="button"
                                        variant="secondary"
                                        size="xs"
                                        disabled={readOnly}
                                        onClick={() => fill(checkedFacts)}
                                    >
                                        勾选项填余量
                                    </Button>
                                    <Button
                                        id={`${idPrefix}-clear-checked`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={readOnly}
                                        onClick={() => clear(checkedFacts)}
                                    >
                                        清空勾选项数量
                                    </Button>
                                </div>
                            </>
                        ) : (
                            <>
                                <div className="flex min-w-0 flex-wrap items-center gap-1">
                                    {showFilters ? (
                                        <>
                                            {(
                                                [
                                                    [
                                                        "all",
                                                        "全部",
                                                        facts.length,
                                                    ],
                                                    [
                                                        "issues",
                                                        "待完善",
                                                        issuesByFact.size,
                                                    ],
                                                    [
                                                        "included",
                                                        "本次纳入",
                                                        totals.rowCount,
                                                    ],
                                                    [
                                                        "skipped",
                                                        "本次不验",
                                                        facts.length -
                                                            totals.rowCount,
                                                    ],
                                                ] as const
                                            ).map(([key, label, count]) =>
                                                count > 0 || filter === key ? (
                                                    <Button
                                                        key={key}
                                                        id={`${idPrefix}-filter-${key}`}
                                                        type="button"
                                                        variant={
                                                            filter === key
                                                                ? "secondary"
                                                                : "ghost"
                                                        }
                                                        size="xs"
                                                        aria-pressed={
                                                            filter === key
                                                        }
                                                        onClick={() => {
                                                            setFilter(key)
                                                            setPage(0)
                                                        }}
                                                    >
                                                        {label}{" "}
                                                        <span className="num">
                                                            {count}
                                                        </span>
                                                    </Button>
                                                ) : null,
                                            )}
                                            <div className="relative min-w-24 flex-[1_1_8rem]">
                                                <SearchIcon
                                                    aria-hidden="true"
                                                    className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground"
                                                />
                                                <Input
                                                    id={`${idPrefix}-search`}
                                                    size="sm"
                                                    aria-label="搜索商品或交付单号"
                                                    placeholder="搜索商品或单号"
                                                    className="pl-7"
                                                    value={search}
                                                    onChange={(event) => {
                                                        setSearch(
                                                            event.target.value,
                                                        )
                                                        setPage(0)
                                                    }}
                                                />
                                            </div>
                                        </>
                                    ) : (
                                        <h2 className="text-body-compact font-semibold">
                                            待验交付批次{" "}
                                            <span className="num text-xs font-normal text-muted-foreground">
                                                {facts.length} 批
                                            </span>
                                        </h2>
                                    )}
                                </div>
                                <div className="flex flex-wrap items-center gap-1">
                                    <Button
                                        id={`${idPrefix}-fill-all`}
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        disabled={readOnly}
                                        onClick={() => fill(facts)}
                                    >
                                        全量填入
                                    </Button>
                                    <Button
                                        id={`${idPrefix}-clear-all`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={readOnly || !totals.rowCount}
                                        onClick={selection.reset}
                                    >
                                        清空
                                    </Button>
                                </div>
                            </>
                        )}
                    </div>

                    <AcceptanceTaskTable
                        idPrefix={idPrefix}
                        id={tableId}
                        rows={pageRows}
                        totalCount={facts.length}
                        filteredCount={matching.length}
                        page={currentPage}
                        pageCount={pageCount}
                        onPageChange={setPage}
                        totals={totals}
                        selection={selection}
                        checked={checked}
                        setChecked={setCheckedFacts}
                        issuesByFact={issuesByFact}
                        readOnly={readOnly}
                        pulse={pulse}
                        onFill={fill}
                        onQuantityChange={updateQuantity}
                    />

                    <div className="grid min-w-0 gap-3 border-t border-border/60 pt-3 @[480px]/acceptance-task:grid-cols-2">
                        <form.AppField name="acceptedAt">
                            {(field) => (
                                <Field
                                    className={cn(
                                        "min-w-0 rounded-md",
                                        pulse(dateId),
                                    )}
                                >
                                    <FieldLabel htmlFor={dateId}>
                                        客户验收时间
                                        <span
                                            aria-hidden="true"
                                            className="text-destructive"
                                        >
                                            *
                                        </span>
                                    </FieldLabel>
                                    <Input
                                        id={dateId}
                                        type="datetime-local"
                                        className="num w-full min-w-0"
                                        disabled={readOnly}
                                        aria-invalid={
                                            Boolean(dateIssue) || undefined
                                        }
                                        aria-describedby={
                                            dateIssue
                                                ? `${dateId}-error`
                                                : undefined
                                        }
                                        value={field.state.value}
                                        onBlur={field.handleBlur}
                                        onChange={(event) =>
                                            field.handleChange(
                                                event.target.value,
                                            )
                                        }
                                    />
                                    {dateIssue ? (
                                        <p
                                            id={`${dateId}-error`}
                                            className="text-xs text-destructive"
                                        >
                                            {dateIssue.message}
                                        </p>
                                    ) : null}
                                </Field>
                            )}
                        </form.AppField>
                        <form.AppField name="evidenceFile">
                            {(field) => (
                                <Field
                                    className={cn(
                                        "min-w-0 rounded-md",
                                        pulse(`${fileId}-input`),
                                    )}
                                >
                                    <FieldLabel htmlFor={`${fileId}-input`}>
                                        签收单凭证
                                        <span
                                            aria-hidden="true"
                                            className="text-destructive"
                                        >
                                            *
                                        </span>
                                    </FieldLabel>
                                    <Input
                                        id={`${fileId}-input`}
                                        type="file"
                                        className="max-w-full min-w-0"
                                        accept="application/pdf,image/jpeg,image/png,image/webp,.pdf,.jpg,.jpeg,.png,.webp"
                                        disabled={readOnly}
                                        aria-describedby={
                                            fileIssue
                                                ? `${fileId}-help ${fileId}-error`
                                                : `${fileId}-help`
                                        }
                                        aria-invalid={
                                            Boolean(fileIssue) || undefined
                                        }
                                        onChange={(event) => {
                                            const file =
                                                event.currentTarget.files?.[0]
                                            if (file) {
                                                field.handleChange(file)
                                                field.handleBlur()
                                            }
                                            event.currentTarget.value = ""
                                        }}
                                    />
                                    <p
                                        id={`${fileId}-help`}
                                        className="text-xs text-muted-foreground"
                                    >
                                        PDF、JPG、PNG 或 WebP，最大 5 MB
                                    </p>
                                    {fileIssue ? (
                                        <p
                                            id={`${fileId}-error`}
                                            className="text-xs text-destructive"
                                        >
                                            {fileIssue.message}
                                        </p>
                                    ) : null}
                                    {field.state.value ? (
                                        <div className="flex min-w-0 items-center justify-between gap-2 text-xs">
                                            <span className="wrap-anywhere">
                                                {field.state.value.name}
                                            </span>
                                            <Button
                                                id={`${fileId}-remove`}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                disabled={readOnly}
                                                onClick={() =>
                                                    field.handleChange(null)
                                                }
                                            >
                                                移除
                                            </Button>
                                        </div>
                                    ) : null}
                                </Field>
                            )}
                        </form.AppField>
                    </div>

                    <details className="border-t border-border/60 pt-2">
                        <summary
                            id={`${commentId}-toggle`}
                            className="w-fit cursor-pointer text-xs text-muted-foreground hover:text-foreground"
                        >
                            添加内部备注（选填）
                        </summary>
                        <form.AppField name="comment">
                            {(field) => (
                                <ReasonTextareaField
                                    id={commentId}
                                    className={cn(
                                        "mt-2 rounded-md",
                                        pulse(commentId),
                                    )}
                                    mode="utf8-bytes"
                                    max={ACCEPTANCE_REASON_MAX_BYTES}
                                    label="内部备注"
                                    rows={2}
                                    disabled={readOnly}
                                    value={field.state.value}
                                    error={commentIssue?.message}
                                    placeholder="可不填"
                                    onBlur={field.handleBlur}
                                    onChange={(next) =>
                                        field.handleChange(next)
                                    }
                                />
                            )}
                        </form.AppField>
                    </details>
                </form>
            ) : (
                <p className="text-xs text-muted-foreground">
                    当前没有可登记的交付批次。
                </p>
            )}

            {canPost ||
            pendingOutcome ||
            relatedSection ||
            issues.length > 0 ? (
                <WorkspaceTaskFooter
                    summary={summary}
                    fallback={
                        <>
                            <div className="mr-auto min-w-0 flex-[1_1_16rem]">
                                {summary}
                            </div>
                            {footerActions}
                        </>
                    }
                >
                    {footerActions}
                </WorkspaceTaskFooter>
            ) : null}
        </div>
    )
}
