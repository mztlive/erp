"use client"

import { useEffect, useRef, useState } from "react"
import { z } from "zod"

import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import type { CompleteImportConfirmationInput } from "@/features/import-opening/api/legacy-import"
import { useImportConfirmationOperations } from "@/features/import-opening/hooks/queries"
import {
    CONFIRMATION_SCOPE_LABEL,
    type ConfirmationScope,
    type ImportBatchView,
    type ImportConfirmationView,
} from "@/features/import-opening/types"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { getErrorMessage } from "@/lib/api/errors"
import {
    FormalCommandKeyLedger,
    type FormalCommandIdentity,
} from "@/lib/formal-command"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"

import { RETURN_REASON_OPTIONS, returnForFixSchema } from "./confirm-section"

type Payload = Omit<CompleteImportConfirmationInput, "idempotencyKey">
type Decision = Payload["action"]
type Failure = "unknown" | "conflict" | "rejected"

const commandSlot = "import-confirmation"
const returnCommentMaxChars = 1024
const RETURN_QUICK_PHRASES = [
    "预检数据与业务台账存在差异",
    "导入口径需按最新规则修正",
    "请补充缺失核对依据后重新预检",
] as const
const embeddedReturnSchema = returnForFixSchema.extend({
    comment: z
        .string()
        .refine(
            (comment) =>
                Array.from(comment.trim()).length <= returnCommentMaxChars,
            `修复说明不能超过 ${returnCommentMaxChars} 个字`,
        )
        .pipe(returnForFixSchema.shape.comment),
})

export function EmbeddedConfirmActions({
    batch,
    confirmation,
    scope,
    idPrefix,
    canComplete,
    blocked,
    anomalyCount,
    onLocateAnomaly,
    onRefresh,
    onTaskCompleted,
}: {
    batch: ImportBatchView
    confirmation?: ImportConfirmationView
    scope?: ConfirmationScope
    idPrefix: string
    canComplete: boolean
    blocked: boolean
    anomalyCount: number
    onLocateAnomaly: () => void
    onRefresh: () => Promise<void>
    onTaskCompleted?: (workItemId: string) => void
}) {
    const operations = useImportConfirmationOperations()
    const [ledger] = useState(() => new FormalCommandKeyLedger())
    const [returnOpen, setReturnOpen] = useState(false)
    const [completed, setCompleted] = useState<Decision>()
    const [failure, setFailure] = useState<{ kind: Failure; message: string }>()
    const [refreshing, setRefreshing] = useState(false)
    const submitting = useRef(false)
    const returnCommentRef = useRef<HTMLTextAreaElement>(null)
    const task = confirmation?.workItem
    const canDecide =
        canComplete &&
        !blocked &&
        !completed &&
        confirmation?.result === "PENDING" &&
        task?.status === "OPEN"
    const canConfirm =
        canDecide && task.allowedActions.includes("CONFIRM_SCOPE")
    const canReturn =
        canDecide && task.allowedActions.includes("RETURN_FOR_FIX")
    const unknown = failure?.kind === "unknown"
    const conflict = failure?.kind === "conflict"
    const pending = operations.isCompleting
    const returnVisible = returnOpen && canReturn && !unknown

    useEffect(() => {
        if (returnVisible) returnCommentRef.current?.focus()
    }, [returnVisible])

    useEffect(() => {
        if (!canReturn || unknown || conflict) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "reject") setReturnOpen(true)
        })
    }, [canReturn, conflict, unknown])

    useEffect(() => {
        if (!unknown) return
        if (confirmation && confirmation.result !== "PENDING") {
            ledger.settle(commandSlot, "succeeded")
            setFailure(undefined)
        }
    }, [confirmation, ledger, unknown])

    async function run(command: FormalCommandIdentity<Payload>) {
        if (submitting.current || !canComplete) return
        submitting.current = true
        try {
            await operations.completeConfirmation({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
        } catch (error) {
            const kind = commandFailureDisposition(error)
            ledger.settle(
                commandSlot,
                kind === "unknown" ? "unknown" : "failed",
            )
            setFailure({
                kind,
                message: getErrorMessage(error, "提交未完成，请重试。"),
            })
            return
        } finally {
            submitting.current = false
        }

        ledger.settle(commandSlot, "succeeded")
        setFailure(undefined)
        setCompleted(command.payload.action)
        setReturnOpen(false)
        form.reset()
        onTaskCompleted?.(command.payload.workItemId)
    }

    async function submit(
        action: Decision,
        reasonCode?: string,
        comment?: string,
    ) {
        if (
            !task ||
            !confirmation ||
            submitting.current ||
            ledger.peek(commandSlot) ||
            conflict ||
            (action === "CONFIRM_SCOPE" ? !canConfirm : !canReturn)
        ) {
            return
        }
        let command: FormalCommandIdentity<Payload>
        try {
            command = ledger.acquire<Payload>(commandSlot, commandSlot, {
                batchId: batch.batchId,
                batchVersion: batch.version,
                trialVersion: confirmation.trialVersion,
                confirmationScope: confirmation.scope,
                workItemId: task.workItemId,
                taskVersion: task.taskVersion,
                subjectVersion: task.subjectVersion,
                action,
                reasonCode,
                comment,
            })
        } catch (error) {
            setFailure({
                kind: "rejected",
                message: getErrorMessage(
                    error,
                    "当前环境无法提交，请刷新后重试。",
                ),
            })
            return
        }
        setFailure(undefined)
        await run(command)
    }

    async function refresh() {
        setRefreshing(true)
        try {
            await onRefresh()
            setFailure(undefined)
        } catch (error) {
            setFailure({
                kind: "conflict",
                message: getErrorMessage(error, "刷新失败，请重试。"),
            })
        } finally {
            setRefreshing(false)
        }
    }

    const form = useAppForm({
        defaultValues: { reasonCode: "DATA_MISMATCH", comment: "" },
        validators: {
            onChange: embeddedReturnSchema,
            onSubmit: embeddedReturnSchema,
        },
        onSubmit: async ({ value }) =>
            submit("RETURN_FOR_FIX", value.reasonCode, value.comment.trim()),
    })
    const readiness = pending
        ? "正在提交"
        : unknown
          ? "待核对提交结果"
          : conflict
            ? "待刷新"
            : completed === "CONFIRM_SCOPE" ||
                confirmation?.result === "CONFIRMED"
              ? "已确认"
              : completed === "RETURN_FOR_FIX" ||
                  confirmation?.result === "REJECTED"
                ? "已退回"
                : confirmation?.result === "INVALIDATED"
                  ? "已失效"
                  : canConfirm
                    ? "可确认"
                    : canReturn
                      ? "可退回"
                      : "暂不可提交"

    const summary = (
        <dl
            className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs"
            aria-live="polite"
        >
            <div className="flex min-w-0 flex-wrap items-baseline gap-1.5">
                <dt className="text-muted-foreground">确认范围</dt>
                <dd className="font-medium">
                    {scope ? CONFIRMATION_SCOPE_LABEL[scope] : "待核对"}
                </dd>
            </div>
            <div className="flex min-w-0 flex-wrap items-baseline gap-1.5">
                <dt className="text-muted-foreground">办理状态</dt>
                <dd className="font-medium">{readiness}</dd>
            </div>
        </dl>
    )
    const actions = (
        <div className="flex min-w-0 flex-1 flex-col items-end gap-2">
            {failure ? (
                <p
                    role="alert"
                    className="w-full border-l-2 border-l-warning bg-warning-soft px-2.5 py-1.5 text-xs text-warning-soft-foreground wrap-anywhere"
                >
                    {unknown
                        ? "提交结果暂未确认。请核对本次提交结果。"
                        : conflict
                          ? `数据已更新，请刷新后重新核对。${failure.message}`
                          : failure.message}
                </p>
            ) : null}
            {returnVisible ? (
                <form
                    id={`${idPrefix}-return-form`}
                    className="w-full min-w-0 space-y-2.5 border-b border-border/60 pb-2.5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="reasonCode">
                        {(field) => (
                            <field.SelectField
                                id={`${idPrefix}-return-reason`}
                                label="退回原因"
                                options={RETURN_REASON_OPTIONS}
                                allowClear={false}
                                required
                                disabled={pending || conflict}
                            />
                        )}
                    </form.AppField>
                    <form.Field name="comment">
                        {(field) => (
                            <ReasonTextareaField
                                id={`${idPrefix}-return-comment`}
                                name={field.name}
                                mode="chars"
                                max={returnCommentMaxChars}
                                textareaRef={returnCommentRef}
                                label="修复说明"
                                placeholder="说明需要修复的数据、口径或依据"
                                quickPhrases={RETURN_QUICK_PHRASES}
                                rows={2}
                                required
                                disabled={pending || conflict}
                                value={field.state.value}
                                onChange={field.handleChange}
                                onBlur={field.handleBlur}
                                error={
                                    field.state.meta.isTouched
                                        ? toFieldErrors(
                                              field.state.meta.errors,
                                          )[0]?.message
                                        : undefined
                                }
                                onCtrlEnterSubmit={() =>
                                    void form.handleSubmit()
                                }
                            />
                        )}
                    </form.Field>
                </form>
            ) : null}

            <div className="flex min-w-0 flex-wrap items-center justify-end gap-2">
                {anomalyCount > 0 ? (
                    <Button
                        id={`${idPrefix}-footer-locate`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="num"
                        onClick={onLocateAnomaly}
                    >
                        {anomalyCount} 项异常 · 定位
                    </Button>
                ) : null}
                {canComplete && unknown ? (
                    <Button
                        id={`${idPrefix}-verify`}
                        type="button"
                        variant="default"
                        size="sm"
                        disabled={pending}
                        onClick={() => {
                            const command = ledger.peek<Payload>(commandSlot)
                            if (command) void run(command)
                        }}
                    >
                        {pending ? "正在核对…" : "核对提交结果"}
                    </Button>
                ) : null}
                {canComplete && conflict ? (
                    <Button
                        id={`${idPrefix}-refresh`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={refreshing}
                        onClick={() => void refresh()}
                    >
                        {refreshing ? "正在刷新…" : "刷新最新数据"}
                    </Button>
                ) : null}
                {canReturn && !unknown && !conflict ? (
                    <Button
                        id={`${idPrefix}-return`}
                        type="button"
                        variant={returnVisible ? "ghost" : "outline"}
                        size="sm"
                        className={
                            returnVisible
                                ? undefined
                                : "border-destructive-border text-destructive-soft-foreground hover:bg-destructive-soft"
                        }
                        aria-expanded={returnVisible}
                        aria-controls={
                            returnVisible
                                ? `${idPrefix}-return-form`
                                : undefined
                        }
                        disabled={pending}
                        onClick={() => setReturnOpen(!returnOpen)}
                    >
                        {returnVisible ? "取消退回" : "退回修复"}
                    </Button>
                ) : null}
                {returnVisible && !conflict ? (
                    <form.Subscribe selector={(state) => state.values}>
                        {(values) => (
                            <Button
                                id={`${idPrefix}-return-submit`}
                                type="submit"
                                form={`${idPrefix}-return-form`}
                                variant="destructive"
                                size="sm"
                                disabled={
                                    pending ||
                                    !embeddedReturnSchema.safeParse(values)
                                        .success
                                }
                            >
                                {pending ? "正在提交…" : "确认退回修复"}
                            </Button>
                        )}
                    </form.Subscribe>
                ) : null}
                {canConfirm && !returnVisible && !unknown && !conflict ? (
                    <Button
                        id={`${idPrefix}-confirm`}
                        type="button"
                        variant="default"
                        size="sm"
                        disabled={pending}
                        onClick={() => void submit("CONFIRM_SCOPE")}
                    >
                        {pending ? "正在提交…" : "确认本范围业务数据"}
                    </Button>
                ) : null}
            </div>
        </div>
    )

    return (
        <WorkspaceTaskFooter
            summary={summary}
            fallback={
                <div className="flex w-full min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-2">
                    {summary}
                    {actions}
                </div>
            }
        >
            {actions}
        </WorkspaceTaskFooter>
    )
}
