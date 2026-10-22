"use client"

import * as React from "react"
import { createPortal } from "react-dom"
import { CheckIcon, PlusIcon, ReceiptTextIcon, Trash2Icon } from "lucide-react"
import { useStore } from "@tanstack/react-form"
import { useQueryClient } from "@tanstack/react-query"
import { InquiryFeeSummary } from "./inquiry-fee-summary"
import { InquiryFeedbackTable } from "./inquiry-feedback-table"
import { quoteExclusion, type InternalQuote } from "../lib/internal-quotes"
import { newFeedbackPrice } from "../lib/feedback-editing"
import { feedbackIssues, feedbackRequestData } from "../lib/feedback-editing"
import {
    feedbackFeeIssues,
    validateFeedbackFees,
} from "../lib/feedback-fee-validation"
import { InquiryFeedbackResults } from "./inquiry-feedback-results"
import { MoneyValue } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fetchInquiry, inquiryPath } from "../api"
import { inquiryKeys } from "../hooks/queries"
import { inquiryDateInput } from "../lib/format"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryFeedbackData,
    type InquiryFee,
    type ProcurementInquiry,
} from "../types"

export type InquiryTask = {
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
    saveForPublish: () => Promise<InquiryPublishContext | null>
    adoptQuote: (quote: InternalQuote) => boolean
}
function newFee(): InquiryFee {
    return {
        fee_id: crypto.randomUUID(),
        name: "",
        amount_gross: "",
        tax_rate: "0",
        line_ids: [],
        valid_until: 0,
        public_note: "",
    }
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

export function InquiryFeedbackEditor({
    ref,
    publishing = false,
    inquiry,
    task,
    userId,
    onSaved,
    onDirtyChange,
    footerActions,
    onInternalQuote,
    footerContainer,
}: {
    ref?: React.Ref<InquiryFeedbackEditorHandle>
    publishing?: boolean
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    onSaved: () => void
    onDirtyChange: (dirty: boolean) => void
    footerActions?: React.ReactNode
    onInternalQuote?: (lineId: string) => void
    footerContainer?: HTMLElement | null
}) {
    const key = inquiryStorageKey(userId, `${inquiry.id}:${task.id}:feedback`)
    const [initial, setInitial] = React.useState<{
        data: InquiryFeedbackData
        version: number
        task_version: number
        baseline?: FeedbackBaseline
    } | null>(null)
    React.useEffect(() => {
        const existing =
            inquiry.feedback_work.find(
                (work) => !work.frozen && work.actor_id === userId,
            )?.data ?? inquiry.feedbacks.at(-1)?.data
        setInitial(
            readInquiryLocalValue(key) ?? {
                version: inquiry.version,
                baseline: feedbackBaseline(inquiry, task, userId),
                task_version: task.version,
                data: {
                    lines: inquiry.draft.lines.map(
                        (line) =>
                            existing?.lines.find(
                                (feedback) => feedback.line_id === line.line_id,
                            ) ?? {
                                line_id: line.line_id,
                                conclusion: "pending",
                                available_quantity: "0",
                                prices: [],
                                public_note: "",
                            },
                    ),
                    fees: existing?.fees ?? [],
                },
            },
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
            footerActions={footerActions}
            onInternalQuote={onInternalQuote}
            footerContainer={footerContainer}
        />
    ) : (
        <p role="status">正在恢复采购输入…</p>
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
    footerActions,
    onInternalQuote,
    footerContainer,
}: {
    ref?: React.Ref<InquiryFeedbackEditorHandle>
    publishing: boolean
    storageKey: string
    initial: {
        data: InquiryFeedbackData
        version: number
        task_version: number
        baseline?: FeedbackBaseline
    }
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    onSaved: () => void
    onDirtyChange: (dirty: boolean) => void
    footerActions?: React.ReactNode
    onInternalQuote?: (lineId: string) => void
    footerContainer?: HTMLElement | null
}) {
    const client = useQueryClient()
    const command = useInquiryCommand(userId, inquiry.id)
    const publishSave = React.useRef(false)
    const savedVersion = React.useRef<number | null>(null)
    const [error, setError] = React.useState<string | null>(null)
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
    const [editingFeeId, setEditingFeeId] = React.useState<string | null>(null)
    const feesTrigger = React.useRef<HTMLButtonElement>(null)
    const feeFocus = React.useRef<string | null>(null)
    const [feeValidationRequest, setFeeValidationRequest] = React.useState(0)
    const [validationRequest, setValidationRequest] = React.useState(0)
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
            setEditingFeeId(value.fees[first.index].fee_id)
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
                setFeesOpen(false)
                setEditingFeeId(null)
                onDirtyChange(false)
                if (!publishSave.current) onSaved()
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
    function editFee(fee: InquiryFee, field = "name") {
        setEditingFeeId(fee.fee_id)
        feeFocus.current = `procurement-inquiry-fee-${toAutomationIdSegment(fee.fee_id)}-${field}`
        setFeeValidationRequest((request) => request + 1)
    }
    function finishFee(index: number) {
        const fee = form.state.values.fees[index]
        const issues = feedbackFeeIssues([fee])
        if (issues.length) {
            for (const issue of issues) {
                form.setFieldMeta(`fees[${index}].${issue.field}`, (meta) => ({
                    ...meta,
                    isTouched: true,
                }))
            }
            void form.validate("change")
            setError(issues[0].message)
            editFee(
                fee,
                { name: "name", amount_gross: "gross", tax_rate: "tax" }[
                    issues[0].field
                ],
            )
            return
        }
        setError(null)
        setEditingFeeId(null)
        feeFocus.current = null
        requestAnimationFrame(() =>
            document
                .getElementById(
                    `procurement-inquiry-fee-${toAutomationIdSegment(fee.fee_id)}-edit`,
                )
                ?.focus(),
        )
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
        onDirtyChange(Boolean(readInquiryLocalValue(storageKey)))
        return () => onDirtyChange(false)
    }, [storageKey, onDirtyChange])
    React.useEffect(() => {
        const resolved = command.resolution
        if (resolved?.path === `${inquiryPath(true, inquiry.id)}/feedback`) {
            // 恢复原命令后，只有当前输入与实际提交内容一致才算已保存。
            const unchanged =
                JSON.stringify(feedbackRequestData(form.state.values)) ===
                JSON.stringify(resolved.body.data)
            onDirtyChange(!unchanged)
            setSaved(unchanged)
            setOwnCommandRevision((value) => value + 1)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [command.resolution])
    const values = useStore(form.store, (state) => state.values)
    const completeCount = values.lines.filter((line) => {
        const demand = inquiry.draft.lines.find(
            (item) => item.line_id === line.line_id,
        )
        return demand && feedbackIssues(line, demand).length === 0
    }).length
    const disabled =
        publishing ||
        conflict ||
        command.isPending ||
        !command.ready ||
        command.unresolved
    React.useImperativeHandle(ref, () => ({
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
            const expiry = quote.terms.valid_to
                ? Math.floor(
                      new Date(
                          `${quote.terms.valid_to}T00:00:00+08:00`,
                      ).getTime() / 1000,
                  ) - 1
                : price.valid_until
            form.setFieldValue(
                "lines",
                form.state.values.lines.map((line) =>
                    line.line_id === quote.line_id
                        ? {
                              ...line,
                              prices: [
                                  {
                                      ...price,
                                      unit_price_gross:
                                          quote.terms.unit_cost_gross,
                                      tax_rate: quote.terms.input_tax_rate,
                                      valid_until: expiry,
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
    const footer = (
        <div className="flex flex-wrap items-center justify-end gap-3 border-t bg-background py-4">
            <p className="mr-auto text-xs text-muted-foreground">
                已填写完整 {completeCount} / {values.lines.length} 项 ·{" "}
                {saved
                    ? "处理进度已保存，发布后销售可见"
                    : "尚未保存，发布时自动保存"}
            </p>
            <Button
                id="procurement-inquiry-save-feedback"
                type="submit"
                form="procurement-inquiry-feedback-form"
                variant="outline"
                disabled={disabled}
            >
                {command.isPending ? "正在保存…" : "保存处理进度"}
            </Button>
            {footerActions}
        </div>
    )
    return (
        <form
            id="procurement-inquiry-feedback-form"
            className="space-y-4"
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
                                            ，有效至{" "}
                                            {inquiryDateInput(
                                                price.valid_until,
                                            )}
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
                                        onDirtyChange(true)
                                    } catch {
                                        setError(
                                            "浏览器无法保留输入，请先恢复浏览器存储后重试。",
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
                    <Alert variant="warning">
                        <AlertDescription>
                            上次提交结果待核对。
                            <Button
                                id="procurement-inquiry-feedback-recover"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                    void command
                                        .recover()
                                        .then(() => onSaved())
                                        .catch((failure) =>
                                            setError(getErrorMessage(failure)),
                                        )
                                }
                            >
                                核对上次提交结果
                            </Button>
                        </AlertDescription>
                    </Alert>
                )}
                <form.Field
                    name="lines"
                    mode="array"
                    children={(field) => (
                        <InquiryFeedbackTable
                            inquiryId={inquiry.id}
                            taskId={task.id}
                            feedbackRound={inquiry.feedback_round + 1}
                            lines={field.state.value}
                            demand={inquiry.draft.lines}
                            onChange={field.handleChange}
                            validationRequest={validationRequest}
                            onInternalQuote={onInternalQuote}
                            disabled={disabled}
                            selectionActions={
                                <Button
                                    id="procurement-feedback-fees-open"
                                    ref={feesTrigger}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={
                                        command.isPending || !command.ready
                                    }
                                    onClick={() => {
                                        setEditingFeeId(null)
                                        feeFocus.current = null
                                        setFeesOpen(true)
                                    }}
                                >
                                    <ReceiptTextIcon aria-hidden="true" />
                                    额外成本（{values.fees.length}）
                                </Button>
                            }
                        />
                    )}
                />
                <Dialog open={feesOpen} onOpenChange={setFeesOpen}>
                    <DialogContent
                        id="procurement-feedback-fees-dialog"
                        closeButtonId="procurement-feedback-fees-close"
                        finalFocus={feesTrigger}
                        initialFocus={() =>
                            feeFocus.current
                                ? (document.getElementById(feeFocus.current) ??
                                  true)
                                : true
                        }
                        className="flex max-h-[85svh] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl"
                    >
                        <DialogHeader className="shrink-0 border-b px-6 py-5 pr-14">
                            <DialogTitle>登记额外成本</DialogTitle>
                            <DialogDescription>
                                填写商品价格之外的费用，备注可选。
                            </DialogDescription>
                        </DialogHeader>
                        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto bg-muted/20 px-6 py-5">
                            {error && (
                                <Alert variant="destructive">
                                    <AlertDescription>{error}</AlertDescription>
                                </Alert>
                            )}
                            {(conflict || command.unresolved) && (
                                <Alert variant="warning">
                                    <AlertDescription>
                                        请先关闭弹窗，在采购反馈中核对
                                        {conflict
                                            ? "已保存内容的变化"
                                            : "上次提交结果"}
                                        ，再继续保存。
                                    </AlertDescription>
                                </Alert>
                            )}
                            <fieldset
                                disabled={disabled}
                                className="min-w-0 space-y-4"
                            >
                                <div className="flex items-center justify-between">
                                    <p className="text-sm text-muted-foreground">
                                        共 {values.fees.length} 项费用
                                    </p>
                                    <Button
                                        id="procurement-inquiry-add-fee"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() => {
                                            const fee = newFee()
                                            form.pushFieldValue("fees", fee)
                                            editFee(fee)
                                        }}
                                    >
                                        <PlusIcon aria-hidden="true" />
                                        添加费用
                                    </Button>
                                </div>
                                {values.fees.length === 0 && (
                                    <p className="rounded-lg border border-dashed px-4 py-8 text-center text-sm text-muted-foreground">
                                        暂无额外成本，点击“添加费用”登记。
                                    </p>
                                )}
                                <form.Field
                                    name="fees"
                                    mode="array"
                                    children={(feesField) => (
                                        <div className="space-y-3">
                                            {feesField.state.value.map(
                                                (fee, index) => {
                                                    const prefix = `procurement-inquiry-fee-${toAutomationIdSegment(fee.fee_id)}`
                                                    if (
                                                        editingFeeId !==
                                                        fee.fee_id
                                                    )
                                                        return (
                                                            <InquiryFeeSummary
                                                                key={fee.fee_id}
                                                                fee={fee}
                                                                idPrefix={
                                                                    prefix
                                                                }
                                                                disabled={
                                                                    disabled
                                                                }
                                                                onEdit={() =>
                                                                    editFee(fee)
                                                                }
                                                                onRemove={() =>
                                                                    feesField.removeValue(
                                                                        index,
                                                                    )
                                                                }
                                                            />
                                                        )
                                                    return (
                                                        <div
                                                            key={fee.fee_id}
                                                            className="space-y-4 rounded-xl border bg-background p-4 sm:p-5"
                                                        >
                                                            <div className="flex items-center justify-between gap-3 border-b pb-3">
                                                                <p className="text-sm font-medium">
                                                                    费用{" "}
                                                                    {index + 1}
                                                                </p>
                                                                <Button
                                                                    id={`${prefix}-remove`}
                                                                    type="button"
                                                                    size="sm"
                                                                    variant="ghost"
                                                                    className="text-muted-foreground hover:text-destructive"
                                                                    onClick={() =>
                                                                        feesField.removeValue(
                                                                            index,
                                                                        )
                                                                    }
                                                                >
                                                                    <Trash2Icon aria-hidden="true" />
                                                                    移除
                                                                </Button>
                                                            </div>
                                                            <form.AppField
                                                                name={`fees[${index}].name`}
                                                                children={(
                                                                    field,
                                                                ) => (
                                                                    <field.TextField
                                                                        id={`${prefix}-name`}
                                                                        label="费用名称"
                                                                        placeholder="例如：运费、包装费、印刷费"
                                                                        required
                                                                    />
                                                                )}
                                                            />
                                                            <div className="grid gap-4 sm:grid-cols-2">
                                                                <form.AppField
                                                                    name={`fees[${index}].amount_gross`}
                                                                    children={(
                                                                        field,
                                                                    ) => (
                                                                        <field.TextField
                                                                            id={`${prefix}-gross`}
                                                                            label="含税金额（元）"
                                                                            placeholder="填写金额"
                                                                            required
                                                                            inputMode="decimal"
                                                                            money
                                                                        />
                                                                    )}
                                                                />
                                                                <form.AppField
                                                                    name={`fees[${index}].tax_rate`}
                                                                    children={(
                                                                        field,
                                                                    ) => (
                                                                        <field.TextField
                                                                            id={`${prefix}-tax`}
                                                                            label="税率"
                                                                            description="0.13 表示 13%，无税额填 0"
                                                                            required
                                                                            inputMode="decimal"
                                                                        />
                                                                    )}
                                                                />
                                                            </div>
                                                            <form.AppField
                                                                name={`fees[${index}].public_note`}
                                                                children={(
                                                                    field,
                                                                ) => (
                                                                    <field.TextareaField
                                                                        id={`${prefix}-note`}
                                                                        label="备注（选填）"
                                                                        placeholder="补充费用说明，销售可见"
                                                                        rows={2}
                                                                    />
                                                                )}
                                                            />
                                                            <div className="flex justify-end">
                                                                <Button
                                                                    id={`${prefix}-done`}
                                                                    type="button"
                                                                    size="sm"
                                                                    variant="outline"
                                                                    onClick={() =>
                                                                        finishFee(
                                                                            index,
                                                                        )
                                                                    }
                                                                >
                                                                    <CheckIcon
                                                                        data-icon="inline-start"
                                                                        aria-hidden="true"
                                                                    />
                                                                    完成编辑
                                                                </Button>
                                                            </div>
                                                        </div>
                                                    )
                                                },
                                            )}
                                        </div>
                                    )}
                                />
                            </fieldset>
                        </div>
                        <DialogFooter className="shrink-0 flex-wrap items-center gap-3 border-t px-6 py-4">
                            <p className="mr-auto text-xs text-muted-foreground">
                                随商品反馈一起保存；关闭保留输入。
                            </p>
                            <Button
                                id="procurement-feedback-fees-dismiss"
                                type="button"
                                variant="outline"
                                onClick={() => setFeesOpen(false)}
                            >
                                关闭
                            </Button>
                            <Button
                                id="procurement-feedback-fees-save"
                                type="button"
                                disabled={disabled}
                                onClick={() => void form.handleSubmit()}
                            >
                                {command.isPending
                                    ? "正在保存…"
                                    : "保存处理进度"}
                            </Button>
                        </DialogFooter>
                    </DialogContent>
                </Dialog>
                {footerContainer
                    ? createPortal(footer, footerContainer)
                    : footer}
            </fieldset>
        </form>
    )
}
