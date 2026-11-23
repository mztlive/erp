"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"

import {
    FormalActionConfirmDialog,
    type WorkflowStatus,
} from "@/components/business/workflow-actions"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { getErrorMessage } from "@/lib/api/errors"
import {
    FormalCommandKeyLedger,
    isCommandOutcomeUnknown,
} from "@/lib/formal-command"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    bpmReasonSchema,
    utf8ByteLength,
} from "@/lib/validation/reason-length"

import { approvalConflictMessage, isApprovalConflict } from "../api"

/** 驳回后撤回修改原单的标准预填文案。 */
export const REVISED_REJECTED_WITHDRAW_REASON = "按驳回意见修改原单后重新提交"

export type ApprovalWithdrawSubmitInput = Readonly<{
    reason: string
    idempotencyKey: string
    retryingUnknown: boolean
}>

export type ApprovalWithdrawDialogProps = Readonly<{
    open: boolean
    onOpenChange: (open: boolean) => void
    /** 稳定 ID 前缀，派生 `-cancel` / `-confirm` / `-reason` 等自动化 ID。 */
    idPrefix: string
    /** 可选显式覆盖原因输入框 ID，兼容既有自动化定位。 */
    reasonId?: string
    title: string
    description?: React.ReactNode
    actionLabel?: string
    confirmLabel?: string
    unknownConfirmLabel?: string
    cancelLabel?: string
    unknownCancelLabel?: string
    fromStatus?: WorkflowStatus
    toStatus?: WorkflowStatus
    showStatusChange?: boolean
    summary?: readonly React.ReactNode[]
    summaryTitle?: string
    effects?: readonly React.ReactNode[]
    irreversibleEffects?: readonly React.ReactNode[]
    reasonLabel?: string
    reasonPlaceholder?: string
    /** 驳回后撤回修改原单时自动预填标准文案。 */
    reviseRejected?: boolean
    /** 显式初始原因（如从顶部横幅恢复未知结果态时沿用上次输入）。 */
    initialReason?: string
    /** 外部 mutation 进行中状态。 */
    pending?: boolean
    /** 外部受控未知结果态（用于允许暂时关闭并从页面顶部恢复的场景）。 */
    unknown?: boolean
    onUnknownChange?: (unknown: boolean) => void
    /** 未知结果态是否允许点击取消暂时关闭弹窗（默认 false，锁定弹窗避免重复发起）。 */
    allowCloseWhenUnknown?: boolean
    /** 通知宿主当前弹窗因请求在途或未知结果态而禁止关闭（如内嵌详情弹窗）。 */
    onCloseBlockedChange?: (blocked: boolean) => void
    /** 幂等键台账命名空间（未传 `createIdempotencyKey` 时用于生成幂等键）。 */
    idempotencyScope?: string
    /** 自定义幂等键生成器，适配各业务域既定前缀格式。 */
    createIdempotencyKey?: () => string
    /**
     * 未知结果重试前核对服务端状态（例如检查单据是否已回到草稿）。
     * 返回 `true` 表示服务端已生效，直接按成功结束并不再重复提交。
     */
    checkUnknownResolved?: () => Promise<boolean>
    /** 执行撤回/终止请求；失败需抛出错误，由统一弹窗归类冲突与未知结果。 */
    onSubmitWithdraw: (input: ApprovalWithdrawSubmitInput) => Promise<void>
    /** 可选在发生错误时回调给业务层（如写入页面级反馈条）。 */
    onSubmitError?: (error: unknown, isUnknown: boolean) => void
    /** 可选自定义错误文案映射。 */
    formatError?: (error: unknown, isUnknown: boolean) => string
    /** 未知结果态的补充指引。 */
    unknownHint?: React.ReactNode
}>

function firstFieldError(errors: readonly unknown[]): string | undefined {
    for (const item of errors) {
        if (typeof item === "string" && item.trim()) return item
        if (
            item &&
            typeof item === "object" &&
            "message" in item &&
            typeof (item as { message?: unknown }).message === "string" &&
            (item as { message: string }).message.trim()
        ) {
            return (item as { message: string }).message
        }
    }
    return undefined
}

function isUnknownWithdrawError(error: unknown): boolean {
    return isCommandOutcomeUnknown(error)
}

/**
 * 全站统一审批撤回 / 受阻终止确认弹窗。
 *
 * 统一收敛 `AlertDialog`（`FormalActionConfirmDialog layout="compact"`）、强制 `destructive` 主按钮、
 * `useAppForm` + BPM 512 UTF-8 字节实时计数、`Ctrl/Cmd+Enter` 提交、幂等键台账与未知结果核对恢复。
 */
export function ApprovalWithdrawDialog({
    open,
    onOpenChange,
    idPrefix,
    reasonId,
    title,
    description,
    actionLabel = "撤回审批",
    confirmLabel,
    unknownConfirmLabel = "核对撤回结果",
    cancelLabel = "取消",
    unknownCancelLabel,
    fromStatus = { label: "审批中", tone: "info" },
    toStatus = { label: "未提交", tone: "neutral" },
    showStatusChange = true,
    summary = [],
    summaryTitle = "核对信息",
    effects = [],
    irreversibleEffects = [],
    reasonLabel = "撤回原因",
    reasonPlaceholder = "请输入撤回原因",
    reviseRejected = false,
    initialReason,
    pending = false,
    unknown: controlledUnknown,
    onUnknownChange,
    allowCloseWhenUnknown = false,
    onCloseBlockedChange,
    idempotencyScope,
    createIdempotencyKey,
    checkUnknownResolved,
    onSubmitWithdraw,
    onSubmitError,
    formatError,
    unknownHint,
}: ApprovalWithdrawDialogProps) {
    const ledgerRef = React.useRef(new FormalCommandKeyLedger())
    const frozenInputRef = React.useRef<{
        reason: string
        idempotencyKey: string
    } | null>(null)
    const inFlightRef = React.useRef(false)
    const [localPending, setLocalPending] = React.useState(false)
    const [internalUnknown, setInternalUnknown] = React.useState(false)
    const [submitError, setSubmitError] = React.useState<string | null>(null)

    const isUnknown = controlledUnknown ?? internalUnknown
    const isBusy = pending || localPending

    const setUnknownState = React.useCallback(
        (next: boolean) => {
            if (controlledUnknown === undefined) {
                setInternalUnknown(next)
            }
            onUnknownChange?.(next)
        },
        [controlledUnknown, onUnknownChange],
    )

    const reasonFieldSchema = React.useMemo(
        () => bpmReasonSchema({ required: true, label: reasonLabel }),
        [reasonLabel],
    )
    const formSchema = React.useMemo(
        () => z.object({ reason: reasonFieldSchema }),
        [reasonFieldSchema],
    )

    const defaultReason =
        initialReason !== undefined
            ? initialReason
            : reviseRejected
              ? REVISED_REJECTED_WITHDRAW_REASON
              : ""

    const form = useAppForm({
        defaultValues: { reason: defaultReason },
        validators: {
            onChange: formSchema,
            onSubmit: formSchema,
        },
        onSubmit: async () => undefined,
    })

    const currentReason = useStore(form.store, (state) => state.values.reason)
    const trimmedReason = currentReason.trim()
    const reasonValid =
        trimmedReason.length > 0 &&
        utf8ByteLength(trimmedReason) <= BPM_REASON_MAX_UTF8_BYTES

    React.useEffect(() => {
        if (!open) return
        setSubmitError(null)
        if (!isUnknown && !ledgerRef.current.peek("withdraw")) {
            frozenInputRef.current = null
            if (controlledUnknown === undefined) {
                setInternalUnknown(false)
            }
            form.reset({ reason: defaultReason })
        }
    }, [controlledUnknown, defaultReason, form, isUnknown, open])

    React.useEffect(() => {
        const blocked =
            open && (isBusy || (isUnknown && !allowCloseWhenUnknown))
        onCloseBlockedChange?.(blocked)
        return () => onCloseBlockedChange?.(false)
    }, [allowCloseWhenUnknown, isBusy, isUnknown, onCloseBlockedChange, open])

    const executeWithdraw = React.useCallback(async () => {
        if (inFlightRef.current || pending) return
        inFlightRef.current = true
        setLocalPending(true)
        setSubmitError(null)

        const wasUnknown = isUnknown
        let checkingUnknown = false

        try {
            let input = frozenInputRef.current
            if (!input) {
                const rawReason = form.state.values.reason
                const parsed = reasonFieldSchema.safeParse(rawReason)
                if (!parsed.success) {
                    await form.validateAllFields("submit")
                    const issueMessage =
                        parsed.error.issues[0]?.message ??
                        `请填写${reasonLabel}`
                    throw new Error(issueMessage)
                }
                const trimmed = parsed.data.trim()
                if (createIdempotencyKey) {
                    input = {
                        reason: trimmed,
                        idempotencyKey: createIdempotencyKey(),
                    }
                } else {
                    const acquired = ledgerRef.current.acquire(
                        "withdraw",
                        idempotencyScope ?? `${idPrefix}:withdraw`,
                        { reason: trimmed },
                    )
                    input = {
                        reason: trimmed,
                        idempotencyKey: acquired.idempotencyKey,
                    }
                }
                frozenInputRef.current = input
            }

            if (wasUnknown && checkUnknownResolved) {
                checkingUnknown = true
                const resolved = await checkUnknownResolved()
                if (resolved) {
                    ledgerRef.current.settle("withdraw", "succeeded")
                    frozenInputRef.current = null
                    setUnknownState(false)
                    setSubmitError(null)
                    return
                }
                checkingUnknown = false
            }

            await onSubmitWithdraw({
                reason: input.reason,
                idempotencyKey: input.idempotencyKey,
                retryingUnknown: wasUnknown,
            })

            ledgerRef.current.settle("withdraw", "succeeded")
            frozenInputRef.current = null
            setUnknownState(false)
            setSubmitError(null)
        } catch (error) {
            const unknown = checkingUnknown || isUnknownWithdrawError(error)
            ledgerRef.current.settle("withdraw", unknown ? "unknown" : "failed")
            setUnknownState(unknown)
            if (!unknown) {
                frozenInputRef.current = null
            }
            const message = formatError
                ? formatError(error, unknown)
                : unknown
                  ? `处理结果待确认，请点击「${unknownConfirmLabel}」重试；确认前不要修改输入或重复撤回。`
                  : isApprovalConflict(error)
                    ? approvalConflictMessage(error)
                    : getErrorMessage(error, "撤回未完成，请刷新后重试。")
            setSubmitError(message)
            onSubmitError?.(error, unknown)
            throw error
        } finally {
            inFlightRef.current = false
            setLocalPending(false)
        }
    }, [
        checkUnknownResolved,
        createIdempotencyKey,
        form,
        formatError,
        idPrefix,
        idempotencyScope,
        isUnknown,
        onSubmitError,
        onSubmitWithdraw,
        pending,
        reasonFieldSchema,
        reasonLabel,
        setUnknownState,
        unknownConfirmLabel,
    ])

    const handleCtrlEnterSubmit = React.useCallback(() => {
        if (isBusy) return
        if (!isUnknown && !reasonValid) {
            void form.validateAllFields("submit")
            return
        }
        void executeWithdraw()
            .then(() => {
                onOpenChange(false)
            })
            .catch(() => {
                // 错误已在弹窗内呈现
            })
    }, [executeWithdraw, form, isBusy, isUnknown, onOpenChange, reasonValid])

    const resolvedConfirmLabel = isUnknown
        ? unknownConfirmLabel
        : (confirmLabel ?? (reviseRejected ? "撤回并修改原单" : "确认撤回"))
    const resolvedCancelLabel =
        isUnknown && unknownCancelLabel ? unknownCancelLabel : cancelLabel

    return (
        <FormalActionConfirmDialog
            idPrefix={idPrefix}
            layout="compact"
            open={open}
            onOpenChange={(next) => {
                if (!next && (isBusy || (isUnknown && !allowCloseWhenUnknown)))
                    return
                onOpenChange(next)
            }}
            pending={isBusy}
            title={title}
            description={description}
            actionLabel={actionLabel}
            confirmLabel={resolvedConfirmLabel}
            cancelLabel={resolvedCancelLabel}
            actionVariant="destructive"
            fromStatus={fromStatus}
            toStatus={toStatus}
            showStatusChange={showStatusChange}
            summary={summary}
            summaryTitle={summaryTitle}
            effects={effects}
            irreversibleEffects={irreversibleEffects}
            confirmDisabled={!isUnknown && !reasonValid}
            onConfirm={executeWithdraw}
            onConfirmError={() => undefined}
            formContent={
                <div className="space-y-3">
                    <form.AppField name="reason">
                        {(field) => {
                            const showError =
                                field.state.meta.isTouched ||
                                field.state.meta.errors.length > 0
                            const errorMessage = showError
                                ? firstFieldError(field.state.meta.errors)
                                : undefined
                            return (
                                <ReasonTextareaField
                                    id={reasonId ?? `${idPrefix}-reason`}
                                    name={field.name}
                                    mode="utf8-bytes"
                                    max={BPM_REASON_MAX_UTF8_BYTES}
                                    label={reasonLabel}
                                    placeholder={reasonPlaceholder}
                                    rows={3}
                                    required
                                    value={field.state.value}
                                    onChange={field.handleChange}
                                    onBlur={field.handleBlur}
                                    error={errorMessage}
                                    invalid={Boolean(errorMessage)}
                                    disabled={isBusy || isUnknown}
                                    onCtrlEnterSubmit={handleCtrlEnterSubmit}
                                />
                            )
                        }}
                    </form.AppField>
                    {isUnknown && unknownHint ? (
                        <Alert variant="warning">
                            <AlertDescription>{unknownHint}</AlertDescription>
                        </Alert>
                    ) : null}
                    {submitError ? (
                        <Alert variant="destructive">
                            <AlertDescription>{submitError}</AlertDescription>
                        </Alert>
                    ) : null}
                </div>
            }
        />
    )
}
