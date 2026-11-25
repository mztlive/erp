"use client"

import * as React from "react"
import { PlusIcon, ReceiptTextIcon } from "lucide-react"
import { useStore } from "@tanstack/react-form"
import { useQueryClient } from "@tanstack/react-query"
import { InquiryExtraCostsDialog } from "./inquiry-extra-costs-dialog"
import { InquiryRatioPricing } from "./inquiry-ratio-pricing"
import { InquiryFeedbackTable } from "./inquiry-feedback-table"
import {
    quoteExclusion,
    quoteReferencePrice,
    type InternalQuote,
} from "../lib/internal-quotes"
import { newFeedbackPrice } from "../lib/feedback-editing"
import { feedbackIssues, feedbackRequestData } from "../lib/feedback-editing"
import {
    feedbackFeeIssues,
    validateFeedbackFees,
} from "../lib/feedback-fee-validation"
import { summarizeFeedback } from "../lib/feedback-summary"
import { InquiryFeedbackResults } from "./inquiry-feedback-results"
import { MoneyValue, detailWorkspaceStyles } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { fetchInquiry, inquiryPath } from "../api"
import { inquiryKeys } from "../hooks/queries"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryFeedbackData,
    type InquiryFeedbackLine,
    type InquiryFee,
    type ProcurementInquiry,
} from "../types"

export type InquiryTask = {
    status?: string
    id: string
    version: number
    owner_user_id?: string
    responsibility_scope_ids?: string[]
}
export type InquiryPublishContext = {
    expected_version: number
    task_id: string
    task_version: number
}
export type InquiryFeedbackEditorHandle = {
    locateLine: (lineId: string) => void
    save: () => void
    recover: () => void
    openFees: () => void
    focusFirstIncomplete: () => void
    validateBeforePublish: () => boolean
    saveForPublish: () => Promise<InquiryPublishContext | null>
    adoptQuote: (quote: InternalQuote) => boolean
}
export type InquiryFeedbackEditorStatus = {
    dirty: boolean
    saving: boolean
    disabled: boolean
    unresolved?: boolean
    completeCount: number
    totalCount: number
    availableCount: number
    partialCount: number
    unavailableCount: number
    pendingCount: number
    pricedCount: number
    goodsTotal: string | null
    feeCount: number
    feesTotal: string | null
    grandTotal: string | null
    saved: boolean
}
type FeedbackBaseline = { scope: string; work: string }
function feedbackBaseline(
    inquiry: ProcurementInquiry,
    task: InquiryTask,
    userId: string,
): FeedbackBaseline {
    return {
        scope: JSON.stringify({
            requirement: inquiry.requirement_version,
            lines: inquiry.draft.lines.map((line) => line.line_id).sort(),
            task: task.id,
            owner: task.owner_user_id,
            feeOwner: inquiry.shared_fee_owner_user_id,
        }),
        work: JSON.stringify(
            inquiry.feedback_work.find(
                (work) => !work.frozen && work.actor_id === userId,
            )?.data ?? null,
        ),
    }
}

export const INQUIRY_FEEDBACK_DRAFT_CHANGED = "inquiry-feedback-draft-changed"

type FeedbackDraft = {
    data: InquiryFeedbackData
    version: number
    task_version: number
    baseline?: FeedbackBaseline
}

export function initialInquiryFeedback(
    inquiry: ProcurementInquiry,
    task: InquiryTask,
    userId: string,
): FeedbackDraft {
    const existing =
        inquiry.feedback_work.find(
            (work) => !work.frozen && work.actor_id === userId,
        )?.data ?? inquiry.feedbacks.at(-1)?.data
    const existingByLine = new Map(
        existing?.lines.map((line) => [line.line_id, line]),
    )
    return {
        version: inquiry.version,
        baseline: feedbackBaseline(inquiry, task, userId),
        task_version: task.version,
        data: {
            lines: inquiry.draft.lines.map(
                (line) =>
                    existingByLine.get(line.line_id) ?? {
                        line_id: line.line_id,
                        conclusion: "pending",
                        available_quantity: "0",
                        prices: [],
                        public_note: "",
                    },
            ),
            fees: existing?.fees ?? [],
        },
    }
}

export function InquiryFeedbackEditor({
    ref,
    publishing = false,
    inquiry,
    task,
    userId,
    onSaved,
    onDirtyChange,
    onStatusChange,
    onLinesChange,
    quoteWorkbookActions,
    onInternalQuote,
    workbench = false,
}: {
    ref?: React.Ref<InquiryFeedbackEditorHandle>
    publishing?: boolean
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    onSaved: () => void
    onDirtyChange: (dirty: boolean) => void
    onStatusChange?: (status: InquiryFeedbackEditorStatus) => void
    onLinesChange?: (lines: readonly InquiryFeedbackLine[]) => void
    quoteWorkbookActions?: React.ReactNode
    onInternalQuote?: (lineId: string) => void
    workbench?: boolean
}) {
    const key = inquiryStorageKey(userId, `${inquiry.id}:${task.id}:feedback`)
    const [initial, setInitial] = React.useState<FeedbackDraft | null>(null)
    React.useEffect(() => {
        setInitial(
            readInquiryLocalValue<FeedbackDraft>(key) ??
                initialInquiryFeedback(inquiry, task, userId),
        )
        // 当前任务的后台刷新不覆盖报价输入。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [key])
    return initial ? (
        <FeedbackEditorContent
            ref={ref}
            publishing={publishing}
            key={key}
            storageKey={key}
            initial={initial}
            inquiry={inquiry}
            task={task}
            userId={userId}
            onSaved={onSaved}
            onDirtyChange={onDirtyChange}
            onStatusChange={onStatusChange}
            onLinesChange={onLinesChange}
            quoteWorkbookActions={quoteWorkbookActions}
            onInternalQuote={onInternalQuote}
            workbench={workbench}
        />
    ) : (
        <p role="status" className="text-xs text-muted-foreground">
            正在恢复采购输入…
        </p>
    )
}

function FeedbackEditorContent({
    ref,
    publishing,
    storageKey,
    initial,
    inquiry,
    task,
    userId,
    onSaved,
    onDirtyChange,
    onStatusChange,
    onLinesChange,
    quoteWorkbookActions,
    onInternalQuote,
    workbench,
}: {
    ref?: React.Ref<InquiryFeedbackEditorHandle>
    publishing: boolean
    storageKey: string
    initial: FeedbackDraft
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    onSaved: () => void
    onDirtyChange: (dirty: boolean) => void
    onStatusChange?: (status: InquiryFeedbackEditorStatus) => void
    onLinesChange?: (lines: readonly InquiryFeedbackLine[]) => void
    quoteWorkbookActions?: React.ReactNode
    onInternalQuote?: (lineId: string) => void
    workbench: boolean
}) {
    const client = useQueryClient()
    const command = useInquiryCommand(userId, inquiry.id)
    const publishSave = React.useRef(false)
    const savedVersion = React.useRef<number | null>(null)
    const [error, setError] = React.useState<string | null>(null)
    const [dirty, setDirty] = React.useState(() =>
        Boolean(readInquiryLocalValue(storageKey)),
    )
    const [saved, setSaved] = React.useState(() => {
        const work = inquiry.feedback_work.find(
            (item) =>
                !item.frozen &&
                item.actor_id === userId &&
                item.requirement_version === inquiry.requirement_version,
        )
        return (
            work != null &&
            initial.version === inquiry.version &&
            JSON.stringify(feedbackRequestData(initial.data)) ===
                JSON.stringify(feedbackRequestData(work.data))
        )
    })
    const [feesOpen, setFeesOpen] = React.useState(false)
    const feesTrigger = React.useRef<HTMLButtonElement>(null)
    const feeFocus = React.useRef<string | null>(null)
    const [feeValidationRequest, setFeeValidationRequest] = React.useState(0)
    const [validationRequest, setValidationRequest] = React.useState(0)
    const [lineLocation, setLineLocation] = React.useState<{ lineId: string }>()
    const [ownCommandRevision, setOwnCommandRevision] = React.useState(0)
    const baseline = React.useRef(
        initial.baseline ??
            (initial.version === inquiry.version
                ? feedbackBaseline(inquiry, task, userId)
                : undefined),
    )
    const [conflict, setConflict] = React.useState(false)
    const version = React.useRef(initial.version)
    const taskVersion = React.useRef(initial.task_version)
    const form = useAppForm({
        defaultValues: initial.data,
        validators: {
            onChange: ({ value }) => validateFeedbackFees(value),
            onSubmit: ({ value }) => validateFeedbackFees(value),
        },
        onSubmitInvalid: ({ value, formApi }) => {
            const issues = feedbackFeeIssues(value.fees)
            const first = issues[0]
            if (!first) return
            for (const issue of issues)
                formApi.setFieldMeta(
                    `fees[${issue.index}].${issue.field}`,
                    (meta) => ({
                        ...meta,
                        isTouched: true,
                    }),
                )
            const suffix = {
                name: "name",
                amount_gross: "gross",
                tax_rate: "tax",
            }[first.field]
            feeFocus.current = `procurement-inquiry-fee-${toAutomationIdSegment(value.fees[first.index].fee_id)}-${suffix}`
            setError(
                `第 ${first.index + 1} 项额外成本未填写完整或格式有误，请补齐标记字段，或移除该费用。`,
            )
            setFeesOpen(true)
            setFeeValidationRequest((request) => request + 1)
        },
        listeners: {
            onChange: ({ formApi }) => {
                setSaved(false)
                setDirty(true)
                if (feeFocus.current) setError(null)
                onDirtyChange(true)
                try {
                    writeInquiryLocalValue(storageKey, {
                        data: formApi.state.values,
                        version: version.current,
                        baseline: baseline.current,
                        task_version: taskVersion.current,
                    })
                } catch {
                    setError(
                        "浏览器无法保留本地输入，请先保存处理进度后再离开。",
                    )
                }
            },
        },
        onSubmit: async ({ value }) => {
            setError(null)
            savedVersion.current = null
            if (conflict) return
            try {
                const result = await command.run<{ version: number }>(
                    `${inquiryPath(true, inquiry.id)}/feedback`,
                    {
                        expected_version: version.current,
                        task_id: task.id,
                        task_version: taskVersion.current,
                        data: feedbackRequestData(value),
                    },
                    "PUT",
                )
                version.current = result.version
                savedVersion.current = result.version
                try {
                    localStorage.removeItem(storageKey)
                } catch {
                    // 服务端保存已成功，本地清理失败不改变提交结果。
                }
                setSaved(true)
                setDirty(false)
                setFeesOpen(false)
                onDirtyChange(false)
                if (!publishSave.current) {
                    toast.add({ title: "处理进度已保存", type: "success" })
                    onSaved()
                }
            } catch (failure) {
                setError(
                    getErrorMessage(
                        failure,
                        "保存采购反馈失败，请保留输入并重试。",
                    ),
                )
            }
        },
    })
    function focusFee(fee: InquiryFee, field?: "name" | "gross" | "tax") {
        const target =
            field ??
            (!fee.name.trim()
                ? "name"
                : !fee.amount_gross.trim()
                  ? "gross"
                  : "name")
        feeFocus.current = `procurement-inquiry-fee-${toAutomationIdSegment(fee.fee_id)}-${target}`
        setFeeValidationRequest((request) => request + 1)
    }
    React.useEffect(() => {
        if (!feesOpen || !feeFocus.current) return
        const frame = requestAnimationFrame(() => {
            const field = document.getElementById(feeFocus.current ?? "")
            field?.focus()
            field?.scrollIntoView({ block: "nearest" })
        })
        return () => cancelAnimationFrame(frame)
    }, [feesOpen, feeValidationRequest])
    React.useEffect(() => {
        const current = feedbackBaseline(inquiry, task, userId)
        const same =
            baseline.current?.scope === current.scope &&
            baseline.current?.work === current.work
        const resolved = command.resolution
        if (resolved?.path === `${inquiryPath(true, inquiry.id)}/feedback`) {
            const result = resolved.result as { version?: number }
            if (
                result.version === inquiry.version &&
                resolved.body.task_id === task.id
            ) {
                baseline.current = current
                version.current = inquiry.version
            }
        }
        if (
            same ||
            (version.current === inquiry.version &&
                baseline.current?.work === current.work)
        ) {
            version.current = inquiry.version
            taskVersion.current = task.version
            baseline.current = current
            setConflict(false)
            if (readInquiryLocalValue(storageKey)) {
                try {
                    writeInquiryLocalValue(storageKey, {
                        data: form.state.values,
                        version: inquiry.version,
                        task_version: task.version,
                        baseline: current,
                    })
                } catch {
                    setError("浏览器无法保留当前采购输入，请先保存处理进度。")
                }
            }
        } else if (!command.isPending && !command.unresolved) {
            setConflict(true)
        }
    }, [
        inquiry,
        task,
        userId,
        command.resolution,
        command.isPending,
        command.unresolved,
        form,
        storageKey,
        ownCommandRevision,
    ])
    React.useEffect(() => {
        const localDirty = Boolean(readInquiryLocalValue(storageKey))
        setDirty(localDirty)
        onDirtyChange(localDirty)
        return () => onDirtyChange(false)
    }, [storageKey, onDirtyChange])
    React.useEffect(() => {
        const resolved = command.resolution
        if (resolved?.path === `${inquiryPath(true, inquiry.id)}/feedback`) {
            // 恢复原命令后，只有当前输入与实际提交内容一致才算已保存。
            const unchanged =
                JSON.stringify(feedbackRequestData(form.state.values)) ===
                JSON.stringify(resolved.body.data)
            setDirty(!unchanged)
            onDirtyChange(!unchanged)
            setSaved(unchanged)
            setOwnCommandRevision((value) => value + 1)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [command.resolution])
    const values = useStore(form.store, (state) => state.values)
    const summary = React.useMemo(
        () => summarizeFeedback(values.lines, inquiry.draft.lines, values.fees),
        [values.lines, inquiry.draft.lines, values.fees],
    )
    const disabled =
        publishing ||
        conflict ||
        command.isPending ||
        !command.ready ||
        command.unresolved
    React.useEffect(() => {
        onStatusChange?.({
            dirty,
            saving: command.isPending,
            disabled,
            unresolved: command.unresolved,
            completeCount: summary.completeCount,
            totalCount: summary.totalCount,
            availableCount: summary.availableCount,
            partialCount: summary.partialCount,
            unavailableCount: summary.unavailableCount,
            pendingCount: summary.pendingCount,
            pricedCount: summary.pricedCount,
            goodsTotal: summary.goodsTotal,
            feeCount: summary.feeCount,
            feesTotal: summary.feesTotal,
            grandTotal: summary.grandTotal,
            saved,
        })
    }, [
        command.isPending,
        command.unresolved,
        dirty,
        disabled,
        onStatusChange,
        saved,
        summary,
    ])
    React.useEffect(() => {
        onLinesChange?.(values.lines)
    }, [onLinesChange, values.lines])
    React.useEffect(() => {
        window.dispatchEvent(
            new CustomEvent(INQUIRY_FEEDBACK_DRAFT_CHANGED, {
                detail: storageKey,
            }),
        )
    }, [storageKey, values, dirty])
    React.useImperativeHandle(ref, () => ({
        locateLine(lineId) {
            setLineLocation({ lineId })
        },
        save() {
            if (!disabled) void form.handleSubmit()
        },
        recover() {
            if (!command.unresolved || command.isPending) return
            void command
                .recover()
                .then(() => onSaved())
                .catch((failure) => setError(getErrorMessage(failure)))
        },
        openFees() {
            if (command.isPending || !command.ready) return
            feeFocus.current = null
            setFeesOpen(true)
        },
        focusFirstIncomplete() {
            setValidationRequest((value) => value + 1)
        },
        validateBeforePublish() {
            const incompleteLines = form.state.values.lines.filter((line) => {
                const demand = inquiry.draft.lines.find(
                    (item) => item.line_id === line.line_id,
                )
                return !demand || feedbackIssues(line, demand).length > 0
            })
            if (incompleteLines.length > 0) {
                setValidationRequest((value) => value + 1)
                setError(null)
                toast.add({
                    title: `还有 ${incompleteLines.length} 项商品待完善`,
                    description: "已为您定位到首处缺漏字段，补齐后即可发布。",
                    type: "warning",
                })
                return false
            }
            const feeIssues = feedbackFeeIssues(form.state.values.fees)
            if (feeIssues.length > 0) {
                const first = feeIssues[0]
                for (const issue of feeIssues) {
                    form.setFieldMeta(
                        `fees[${issue.index}].${issue.field}`,
                        (meta) => ({ ...meta, isTouched: true }),
                    )
                }
                const fee = form.state.values.fees[first.index]
                const suffix = {
                    name: "name",
                    amount_gross: "gross",
                    tax_rate: "tax",
                }[first.field]
                feeFocus.current = `procurement-inquiry-fee-${toAutomationIdSegment(fee.fee_id)}-${suffix}`
                setError(
                    `第 ${first.index + 1} 项额外成本未填写完整或格式有误，请补齐标记字段，或移除该费用。`,
                )
                setFeesOpen(true)
                setFeeValidationRequest((request) => request + 1)
                return false
            }
            return true
        },
        adoptQuote(quote) {
            if (disabled || publishSave.current) return false
            const demand = inquiry.draft.lines.find(
                (line) => line.line_id === quote.line_id,
            )
            if (
                !demand ||
                quote.requirement_version !== inquiry.requirement_version ||
                quoteExclusion(quote, demand, inquiry.feedback_round + 1)
            )
                return false
            const row = form.state.values.lines.find(
                (line) => line.line_id === quote.line_id,
            )
            if (!row || row.conclusion === "unavailable") return false
            const price = row.prices[0] ?? newFeedbackPrice()

            form.setFieldValue(
                "lines",
                form.state.values.lines.map((line) =>
                    line.line_id === quote.line_id
                        ? {
                              ...line,
                              prices: [
                                  {
                                      ...price,
                                      unit_price_gross: quoteReferencePrice(
                                          quote,
                                          demand.quantity,
                                      ),
                                      tax_rate: quote.terms.input_tax_rate,
                                  },
                              ],
                          }
                        : line,
                ),
            )
            return true
        },
        async saveForPublish() {
            if (disabled || publishSave.current) return null
            const first = form.state.values.lines.find((line) => {
                const demand = inquiry.draft.lines.find(
                    (item) => item.line_id === line.line_id,
                )
                return !demand || feedbackIssues(line, demand).length > 0
            })
            if (first) {
                setValidationRequest((value) => value + 1)
                setError("请先完善标出的商品反馈，再发布；发布时会自动保存。")
                return null
            }
            publishSave.current = true
            savedVersion.current = null
            try {
                await form.handleSubmit()
                if (savedVersion.current == null) return null
                // 保存会更新询价和任务版本，必须从服务端取得新版本再发布。
                const latest = await client.fetchQuery({
                    queryKey: inquiryKeys.detail(inquiry.id, true, task.id),
                    queryFn: () => fetchInquiry(inquiry.id, true, task.id),
                    staleTime: 0,
                })
                const latestTask =
                    "work_items" in latest
                        ? latest.work_items.find((item) => item.id === task.id)
                        : undefined
                if (!latestTask || latest.version !== savedVersion.current) {
                    throw new Error(
                        "询价或采购分工已发生变化，请核对最新内容后重新发布。",
                    )
                }
                return {
                    expected_version: savedVersion.current,
                    task_id: latestTask.id,
                    task_version: latestTask.version,
                }
            } catch (failure) {
                if (savedVersion.current == null) throw failure
                throw new Error(
                    `你的反馈已保存，尚未发起发布。${getErrorMessage(failure)}`,
                )
            } finally {
                publishSave.current = false
            }
        },
    }))
    const currentWork = inquiry.feedback_work.find(
        (work) => !work.frozen && work.actor_id === userId,
    )?.data
    return (
        <form
            id="procurement-inquiry-feedback-form"
            className="space-y-3"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <fieldset
                disabled={command.isPending || publishing}
                className="contents"
            >
                {conflict && (
                    <Alert variant="warning">
                        <AlertDescription className="space-y-3">
                            <p>
                                已保存的采购反馈发生变化，当前输入已保留。请核对以下最新内容，再决定是否用当前输入继续编辑。
                            </p>
                            {currentWork?.lines.map((line) => (
                                <div key={line.line_id}>
                                    {
                                        inquiry.draft.lines.find(
                                            (item) =>
                                                item.line_id === line.line_id,
                                        )?.product_name
                                    }
                                    ：
                                    {INQUIRY_CONCLUSION_LABEL[line.conclusion]}
                                    ，可供 {line.available_quantity}；
                                    {line.prices.map((price) => (
                                        <span key={price.price_id}>
                                            供给价{" "}
                                            <MoneyValue
                                                value={price.unit_price_gross}
                                            />
                                            ；
                                        </span>
                                    ))}
                                </div>
                            ))}
                            {currentWork?.fees.map((fee) => (
                                <div key={fee.fee_id}>
                                    {fee.name}：
                                    <MoneyValue value={fee.amount_gross} />
                                </div>
                            ))}
                            {!currentWork && <p>当前尚无已保存反馈。</p>}
                            {currentWork && (
                                <details>
                                    <summary
                                        id="procurement-inquiry-feedback-conflict-details"
                                        className="cursor-pointer"
                                    >
                                        查看完整已保存内容
                                    </summary>
                                    <InquiryFeedbackResults
                                        idPrefix="procurement-inquiry-feedback-conflict"
                                        inquiry={{
                                            ...inquiry,
                                            feedback_round:
                                                inquiry.feedback_round + 1,
                                            feedbacks: [
                                                {
                                                    round:
                                                        inquiry.feedback_round +
                                                        1,
                                                    requirement_version:
                                                        inquiry.requirement_version,
                                                    published_at:
                                                        inquiry.feedback_work.find(
                                                            (work) =>
                                                                !work.frozen &&
                                                                work.actor_id ===
                                                                    userId,
                                                        )?.saved_at ?? 0,
                                                    published_by: userId,
                                                    data: currentWork,
                                                },
                                            ],
                                        }}
                                    />
                                </details>
                            )}
                            <Button
                                id="procurement-inquiry-feedback-rebase"
                                type="button"
                                variant="outline"
                                disabled={
                                    command.isPending ||
                                    command.unresolved ||
                                    (baseline.current?.scope != null &&
                                        baseline.current.scope !==
                                            feedbackBaseline(
                                                inquiry,
                                                task,
                                                userId,
                                            ).scope)
                                }
                                onClick={() => {
                                    const current = feedbackBaseline(
                                        inquiry,
                                        task,
                                        userId,
                                    )
                                    try {
                                        writeInquiryLocalValue(storageKey, {
                                            data: form.state.values,
                                            version: inquiry.version,
                                            task_version: task.version,
                                            baseline: current,
                                        })
                                        baseline.current = current
                                        version.current = inquiry.version
                                        taskVersion.current = task.version
                                        setConflict(false)
                                        setError(null)
                                        setDirty(true)
                                        onDirtyChange(true)
                                    } catch {
                                        setError(
                                            "无法暂存当前填写内容，请清理浏览器缓存或关闭无痕模式后重试。",
                                        )
                                    }
                                }}
                            >
                                已核对，保留当前输入继续编辑
                            </Button>
                        </AlertDescription>
                    </Alert>
                )}
                {error && (
                    <Alert variant="destructive">
                        <AlertDescription>{error}</AlertDescription>
                    </Alert>
                )}
                {command.unresolved && (
                    <Alert variant="warning" className="border-l-2">
                        <AlertDescription>
                            上次提交结果待核对。
                            <Button
                                id="procurement-inquiry-feedback-recover"
                                type="button"
                                variant="outline"
                                size="sm"
                                className="ml-2"
                                onClick={() =>
                                    void command
                                        .recover()
                                        .then(() => onSaved())
                                        .catch((failure) =>
                                            setError(getErrorMessage(failure)),
                                        )
                                }
                            >
                                核对提交结果
                            </Button>
                        </AlertDescription>
                    </Alert>
                )}
                <form.Field
                    name="lines"
                    mode="array"
                    children={(field) => (
                        <InquiryFeedbackTable
                            workbench={workbench}
                            lineLocation={lineLocation}
                            inquiryId={inquiry.id}
                            taskId={task.id}
                            feedbackRound={inquiry.feedback_round + 1}
                            lines={field.state.value}
                            demand={inquiry.draft.lines}
                            onChange={field.handleChange}
                            validationRequest={validationRequest}
                            onInternalQuote={onInternalQuote}
                            disabled={disabled}
                            toolbarActions={
                                onInternalQuote ? (
                                    <InquiryRatioPricing
                                        inquiryId={inquiry.id}
                                        taskId={task.id}
                                        round={inquiry.feedback_round + 1}
                                        demand={inquiry.draft.lines}
                                        lines={values.lines}
                                        disabled={disabled}
                                        onChange={(lines) =>
                                            form.setFieldValue("lines", lines)
                                        }
                                    />
                                ) : undefined
                            }
                            quoteWorkbookActions={quoteWorkbookActions}
                            extraCostsFooter={
                                <div className="space-y-2 border-b border-border/70 py-3">
                                    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                                        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
                                            <span className="inline-flex items-center gap-1.5 text-body-compact font-medium text-foreground">
                                                <ReceiptTextIcon
                                                    aria-hidden="true"
                                                    className="size-3.5 text-muted-foreground"
                                                />
                                                额外成本
                                            </span>
                                            {values.fees.length === 0 ? (
                                                <span className="text-xs text-muted-foreground">
                                                    暂无运费、包装、打样等整单费用
                                                </span>
                                            ) : (
                                                <div className="flex flex-wrap items-center gap-1.5">
                                                    {values.fees.map((fee) => {
                                                        const hasFeeIssue =
                                                            feedbackFeeIssues([
                                                                fee,
                                                            ]).length > 0
                                                        return (
                                                            <button
                                                                key={fee.fee_id}
                                                                id={`procurement-feedback-fee-chip-${toAutomationIdSegment(fee.fee_id)}`}
                                                                type="button"
                                                                disabled={
                                                                    command.isPending ||
                                                                    !command.ready
                                                                }
                                                                className={cn(
                                                                    "inline-flex h-6 cursor-pointer items-center gap-1.5 rounded-md border px-2 text-xs transition-colors",
                                                                    hasFeeIssue
                                                                        ? "border-warning-border bg-warning-soft text-warning-soft-foreground hover:bg-warning-soft/80"
                                                                        : "border-border/70 bg-muted/50 text-foreground/85 hover:border-primary/35 hover:bg-sidebar-accent/70 hover:text-sidebar-accent-foreground",
                                                                )}
                                                                onClick={() => {
                                                                    focusFee(
                                                                        fee,
                                                                    )
                                                                    setFeesOpen(
                                                                        true,
                                                                    )
                                                                }}
                                                            >
                                                                <span className="max-w-36 truncate font-medium">
                                                                    {fee.name.trim() ||
                                                                        "未命名费用"}
                                                                </span>
                                                                {fee.amount_gross.trim() ? (
                                                                    <span className="text-muted-foreground">
                                                                        <MoneyValue
                                                                            value={fee.amount_gross.trim()}
                                                                            display="exact"
                                                                            showHint={
                                                                                false
                                                                            }
                                                                            inheritColor
                                                                        />
                                                                    </span>
                                                                ) : (
                                                                    <span className="text-warning-soft-foreground">
                                                                        待填金额
                                                                    </span>
                                                                )}
                                                            </button>
                                                        )
                                                    })}
                                                </div>
                                            )}
                                            <Button
                                                id="procurement-feedback-fees-open"
                                                ref={feesTrigger}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                className={cn(
                                                    detailWorkspaceStyles.inlineLinkButton,
                                                    "h-6 px-1.5 text-xs",
                                                )}
                                                disabled={
                                                    command.isPending ||
                                                    !command.ready
                                                }
                                                onClick={() => {
                                                    feeFocus.current = null
                                                    setFeesOpen(true)
                                                }}
                                            >
                                                <PlusIcon
                                                    aria-hidden="true"
                                                    className="size-3"
                                                />
                                                {values.fees.length > 0
                                                    ? `管理费用 (${values.fees.length})`
                                                    : "登记额外成本"}
                                            </Button>
                                        </div>

                                        <div className="flex shrink-0 items-baseline gap-6 text-right">
                                            <div className="flex items-baseline gap-2">
                                                <span className="text-xs text-muted-foreground">
                                                    额外成本合计
                                                </span>
                                                <span className="num min-w-24 font-medium text-foreground">
                                                    {summary.feesTotal !==
                                                    null ? (
                                                        <MoneyValue
                                                            value={
                                                                summary.feesTotal
                                                            }
                                                            display="exact"
                                                            showHint={false}
                                                        />
                                                    ) : (
                                                        <span className="font-normal text-muted-foreground/60">
                                                            —
                                                        </span>
                                                    )}
                                                </span>
                                            </div>
                                            {summary.feesTotal !== null &&
                                            summary.grandTotal !== null ? (
                                                <div className="flex items-baseline gap-2 border-l border-border/60 pl-5">
                                                    <span className="text-xs text-muted-foreground">
                                                        含税总计
                                                    </span>
                                                    <span className="num min-w-24 font-semibold text-foreground">
                                                        <MoneyValue
                                                            value={
                                                                summary.grandTotal
                                                            }
                                                            display="exact"
                                                            showHint={false}
                                                        />
                                                    </span>
                                                </div>
                                            ) : null}
                                        </div>
                                    </div>
                                </div>
                            }
                        />
                    )}
                />
                <InquiryExtraCostsDialog
                    form={form}
                    open={feesOpen}
                    onOpenChange={setFeesOpen}
                    triggerRef={feesTrigger}
                    feeFocusRef={feeFocus}
                    summary={summary}
                    error={error}
                    conflict={conflict}
                    commandUnresolved={command.unresolved}
                    disabled={disabled}
                    saving={command.isPending}
                    onSave={() => void form.handleSubmit()}
                    onClearFeeError={() => setError(null)}
                />
            </fieldset>
        </form>
    )
}
