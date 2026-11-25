"use client"

import * as React from "react"
import {
    CheckIcon,
    CircleCheckIcon,
    CircleDashedIcon,
    CircleXIcon,
    CopyIcon,
    EyeIcon,
    EyeOffIcon,
    LoaderCircleIcon,
    LockKeyholeIcon,
    SaveIcon,
    TriangleAlertIcon,
    type LucideIcon,
} from "lucide-react"

import { toAutomationIdSegment } from "@/lib/automation-id"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import {
    Progress,
    ProgressLabel,
    ProgressValue,
} from "@/components/ui/progress"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
import { resultText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"
import {
    AsyncSectionState,
    BusinessEmptyState,
    BusinessFailureState,
    GuardedBusinessAction,
} from "@/components/business/feedback-states"
import type {
    AsyncSectionStateProps,
    AsyncSectionStatus,
    BusinessEmptyStateKind,
    BusinessEmptyStateProps,
    BusinessFailureKind,
    BusinessFailureStateProps,
    GuardedBusinessActionProps,
} from "@/components/business/feedback-states"

type DraftSaveState = "idle" | "dirty" | "saving" | "saved" | "failed"

type DraftSaveIndicatorProps = {
    state: DraftSaveState
    savedAt?: Date | string
    message?: string
    onRetry?: () => void
    className?: string
    id?: string
    idPrefix?: string
}

const draftTimeFormatter = new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
})

function formatDraftTime(value: Date | string) {
    return typeof value === "string" ? value : draftTimeFormatter.format(value)
}

const draftStatePresentation: Record<
    DraftSaveState,
    { label: string; tone: StatusTone; icon: LucideIcon }
> = {
    idle: { label: "尚无更改", tone: "neutral", icon: CircleDashedIcon },
    dirty: { label: "有未保存更改", tone: "warning", icon: SaveIcon },
    saving: { label: "正在保存", tone: "info", icon: LoaderCircleIcon },
    saved: { label: "已保存", tone: "success", icon: CircleCheckIcon },
    failed: { label: "保存失败", tone: "destructive", icon: CircleXIcon },
}

function DraftSaveIndicator({
    state,
    savedAt,
    message,
    onRetry,
    className,
    id,
    idPrefix,
}: DraftSaveIndicatorProps) {
    const baseId = idPrefix ?? id
    const presentation = draftStatePresentation[state]
    const savedSuffix =
        state === "saved" && savedAt ? ` ${formatDraftTime(savedAt)}` : ""

    return (
        <div
            id={baseId}
            data-slot="draft-save-indicator"
            data-state={state}
            role="status"
            aria-live="polite"
            className={cn(
                "flex flex-wrap items-center gap-2 text-body-compact",
                className,
            )}
        >
            <StatusBadge
                tone={presentation.tone}
                icon={presentation.icon}
                label={`${presentation.label}${savedSuffix}`}
            />
            {message ? (
                <span className="text-muted-foreground">{message}</span>
            ) : null}
            {state === "failed" && onRetry ? (
                <Button
                    id={baseId ? `${baseId}-retry` : undefined}
                    type="button"
                    variant="link"
                    size="xs"
                    onClick={onRetry}
                >
                    重试保存
                </Button>
            ) : null}
        </div>
    )
}

type ValidationIssue = {
    id: string
    label: string
    message: string
    targetId?: string
}

type ValidationSummaryProps = {
    issues: readonly ValidationIssue[]
    title?: string
    compact?: boolean
    onLocate?: (issue: ValidationIssue) => void
    className?: string
    id?: string
    idPrefix?: string
}

function locateValidationIssue(issue: ValidationIssue) {
    if (!issue.targetId) return

    const target = document.getElementById(issue.targetId)
    target?.scrollIntoView({ block: "center" })

    if (target instanceof HTMLElement) {
        target.focus({ preventScroll: true })
    }
}

function ValidationSummary({
    issues,
    title,
    compact = false,
    onLocate,
    className,
    id,
    idPrefix,
}: ValidationSummaryProps) {
    if (issues.length === 0) return null
    const baseId = idPrefix ?? id

    return (
        <Alert
            id={baseId}
            data-slot="validation-summary"
            variant="warning"
            className={cn(compact && "rounded-lg px-3 py-2", className)}
        >
            <TriangleAlertIcon aria-hidden="true" />
            <AlertTitle className={compact ? "sr-only" : undefined}>
                {title ?? `发现 ${issues.length} 项待处理`}
            </AlertTitle>
            <AlertDescription>
                <ol
                    className={
                        compact
                            ? "flex list-none flex-wrap gap-x-4 gap-y-1"
                            : "flex list-decimal flex-col gap-1 pl-5"
                    }
                >
                    {issues.map((issue) => (
                        <li key={issue.id}>
                            <Button
                                id={
                                    baseId
                                        ? `${baseId}-issue-${toAutomationIdSegment(issue.id)}`
                                        : undefined
                                }
                                type="button"
                                variant="link"
                                size="xs"
                                aria-controls={issue.targetId}
                                className={cn(
                                    "h-auto justify-start p-0 text-left whitespace-normal",
                                    compact && "text-xs",
                                )}
                                onClick={() => {
                                    if (onLocate) {
                                        onLocate(issue)
                                        return
                                    }

                                    locateValidationIssue(issue)
                                }}
                            >
                                <span className="font-medium">
                                    {issue.label}：
                                </span>
                                <span>{issue.message}</span>
                            </Button>
                        </li>
                    ))}
                </ol>
            </AlertDescription>
        </Alert>
    )
}

/**
 * 页面通用「表单动作结果」核心结构。
 * 统一以 `pendingIdempotencyKey` 承接未知结果的幂等核对键；
 * 各业务域如需扩展额外元数据，可通过第二个泛型参数 `TExtra` 声明。
 */
type ResultStateBase<TOutcome = unknown> = {
    status:
        | "succeeded"
        | "failed"
        | "rejected"
        | "blocked"
        | "unknown"
        | "processing"
    title: string
    description: string
    reference?: string
    facts?: Array<{ label: string; value: React.ReactNode }>
    pendingIdempotencyKey?: string
    outcome?: TOutcome
    stayOnItem?: boolean
    returnTo?: string
}

/**
 * 历史页面私有/重名字段兼容层（供存量 `features/*` 过渡期编译兼容，新页面请通过 `ResultState<TOutcome, TExtra>` 自行扩展）。
 * 已清理零引用死字段 `pendingRequestId` / `terminal` / `w29Href` / `stayUnknown`。
 */
type LegacyResultStateExtra = {
    /** @deprecated 请改用 `pendingIdempotencyKey`（存量 `customer-receivables` 迁移后移除）。 */
    pendingKey?: string
    /** @deprecated 业务私有字段（存量 `supplier-api-connections` 迁移后移除）。 */
    jobId?: string
    /** @deprecated 业务私有字段（存量 `supplier-api-connections` 迁移后移除）。 */
    jobNo?: string
    /** @deprecated 业务私有字段（存量 `supplier-settlements` 迁移后移除）。 */
    payableNo?: string
    /** @deprecated 业务私有字段（存量 `supplier-settlements` 迁移后移除）。 */
    w12Href?: string
}

type ResultState<
    TOutcome = unknown,
    TExtra extends Record<string, unknown> = LegacyResultStateExtra,
> = (ResultStateBase<TOutcome> & TExtra) | null

type FormalActionResultStatus =
    | "succeeded"
    | "rejected"
    | "blocked"
    | "processing"
    | "unknown"

type FormalActionFact = {
    label: string
    value: React.ReactNode
}

type FormalActionResultProps = {
    status: FormalActionResultStatus
    title?: string
    description?: React.ReactNode
    reference?: string
    /** 结果编号的标签；内部任务号/证据 ID 场景可改为「原任务号」等业务说法。 */
    referenceLabel?: string
    facts?: readonly FormalActionFact[]
    actions?: React.ReactNode
    className?: string
}

type FormalResultPreset = {
    title: string
    description: string
    label: string
    icon: LucideIcon
    tone: StatusTone
}

const formalResultPresets: Record<
    FormalActionResultStatus,
    FormalResultPreset
> = {
    succeeded: {
        title: resultText.operationSucceeded,
        description: "操作已生效，可在关联单据或操作记录中查看。",
        label: "已完成",
        icon: CircleCheckIcon,
        tone: "success",
    },
    rejected: {
        title: resultText.operationRejected,
        description: "操作未通过，请按提示原因调整后重试。",
        label: "未通过",
        icon: CircleXIcon,
        tone: "destructive",
    },
    blocked: {
        title: resultText.operationBlocked,
        description: "当前条件暂不满足，操作未生效。",
        label: "暂不可操作",
        icon: TriangleAlertIcon,
        tone: "warning",
    },
    processing: {
        title: resultText.operationProcessing,
        description: "结果尚未确定，可安全离开并在后台任务中继续查看。",
        label: "处理中",
        icon: LoaderCircleIcon,
        tone: "info",
    },
    unknown: {
        title: resultText.unknown,
        description:
            "暂无法确认是否提交成功，请先刷新核对单据状态，切勿重复操作。",
        label: "结果未知",
        icon: TriangleAlertIcon,
        tone: "warning",
    },
}

function FormalActionResult({
    status,
    title,
    description,
    reference,
    referenceLabel = "结果编号",
    facts,
    actions,
    className,
}: FormalActionResultProps) {
    const preset = formalResultPresets[status]
    const Icon = preset.icon
    const role =
        status === "rejected" || status === "blocked" ? "alert" : "status"

    return (
        <section
            data-slot="formal-action-result"
            data-status={status}
            role={role}
            aria-live="polite"
            tabIndex={-1}
            className={cn(
                "erp-raised-surface rounded-lg border border-border bg-card px-4 py-3 text-body-compact text-card-foreground shadow-xs",
                className,
            )}
        >
            <div className="flex items-start gap-3">
                <Icon
                    className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                    aria-hidden="true"
                />
                <div className="min-w-0 flex-1 space-y-3">
                    <header className="flex flex-wrap items-center gap-2">
                        <h3 className="font-medium">{title ?? preset.title}</h3>
                        <StatusBadge tone={preset.tone} label={preset.label} />
                    </header>
                    <p className="text-muted-foreground">
                        {description ?? preset.description}
                    </p>
                    {reference ? (
                        <p className="text-xs text-muted-foreground">
                            {referenceLabel}：
                            <span className="num font-mono">{reference}</span>
                        </p>
                    ) : null}
                    {facts?.length ? (
                        <dl className="grid gap-2 sm:grid-cols-2">
                            {facts.map((fact) => (
                                <div
                                    key={fact.label}
                                    className="flex flex-col gap-0.5 rounded-md bg-muted/40 px-3 py-2"
                                >
                                    <dt className="text-xs font-medium text-muted-foreground">
                                        {fact.label}
                                    </dt>
                                    <dd className="text-foreground">
                                        {fact.value}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                    ) : null}
                    {actions ? (
                        <div className="flex flex-wrap gap-2">{actions}</div>
                    ) : null}
                </div>
            </div>
        </section>
    )
}

type BackgroundJobMode = "all-or-nothing" | "partialAllowed"
type BackgroundJobStatus =
    | "queued"
    | "running"
    | "succeeded"
    | "partial"
    | "failed"
    | "frozen"

type BackgroundJobProgressProps = {
    mode: BackgroundJobMode
    status: BackgroundJobStatus
    total: number
    completed?: number
    succeeded?: number
    skipped?: number
    failed?: number
    label?: string
    description?: React.ReactNode
    action?: React.ReactNode
    className?: string
}

const backgroundJobStatusPresentation: Record<
    BackgroundJobStatus,
    { label: string; tone: StatusTone }
> = {
    queued: { label: "等待执行", tone: "neutral" },
    running: { label: "执行中", tone: "info" },
    succeeded: { label: "已完成", tone: "success" },
    partial: { label: "部分成功", tone: "warning" },
    failed: { label: "执行失败", tone: "destructive" },
    frozen: { label: "已冻结", tone: "warning" },
}

function safeCount(value = 0) {
    return Number.isFinite(value) ? Math.max(0, value) : 0
}

function BackgroundJobProgress({
    mode,
    status,
    total,
    completed,
    succeeded,
    skipped,
    failed,
    label = "后台任务进度",
    description,
    action,
    className,
}: BackgroundJobProgressProps) {
    const safeTotal = safeCount(total)
    const successCount = safeCount(succeeded)
    const skippedCount = safeCount(skipped)
    const failedCount = safeCount(failed)
    const processedCount = Math.min(
        safeTotal,
        safeCount(completed ?? successCount + skippedCount + failedCount),
    )
    const percent = safeTotal === 0 ? 0 : (processedCount / safeTotal) * 100
    const statusPresentation = backgroundJobStatusPresentation[status]
    const modeDescription =
        mode === "all-or-nothing"
            ? "全部校验通过后一起生效；若任一条有误，整批均不生效。"
            : "允许部分生效；已成功处理的记录立即生效，失败项可单独修正后重试。"

    return (
        <section
            data-slot="background-job-progress"
            data-mode={mode}
            data-status={status}
            aria-live="polite"
            className={cn(
                "flex flex-col gap-4 rounded-lg border bg-card p-4 text-body-compact",
                className,
            )}
        >
            <div className="flex flex-wrap items-start justify-between gap-2">
                <div className="flex flex-col gap-1">
                    <h3 className="font-medium">{label}</h3>
                    <p className="text-muted-foreground">
                        {description ?? modeDescription}
                    </p>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                    <StatusBadge
                        tone={mode === "all-or-nothing" ? "neutral" : "info"}
                        label={
                            mode === "all-or-nothing"
                                ? "全部通过才生效"
                                : "允许部分成功"
                        }
                    />
                    <StatusBadge
                        tone={statusPresentation.tone}
                        label={statusPresentation.label}
                    />
                </div>
            </div>

            <Progress value={percent}>
                <ProgressLabel>{label}</ProgressLabel>
                <ProgressValue>
                    {() => (
                        <span className="num">
                            {processedCount} / {safeTotal}
                        </span>
                    )}
                </ProgressValue>
            </Progress>

            {mode === "partialAllowed" ? (
                <dl className="grid grid-cols-3 gap-2 text-center">
                    <div className="rounded-md bg-success-soft p-2 text-success-soft-foreground">
                        <dt className="text-xs">成功</dt>
                        <dd className="num font-medium">{successCount}</dd>
                    </div>
                    <div className="rounded-md bg-neutral-soft p-2 text-neutral-soft-foreground">
                        <dt className="text-xs">跳过</dt>
                        <dd className="num font-medium">{skippedCount}</dd>
                    </div>
                    <div className="rounded-md bg-destructive-soft p-2 text-destructive-soft-foreground">
                        <dt className="text-xs">失败</dt>
                        <dd className="num font-medium">{failedCount}</dd>
                    </div>
                </dl>
            ) : null}

            {action ? (
                <div className="flex flex-wrap gap-2">{action}</div>
            ) : null}
        </section>
    )
}

type SensitiveValueProps = {
    maskedValue: string
    label?: string
    onReveal?: () => Promise<string>
    /** 是否展示一键复制按钮（仅在存在 `onReveal` 权限回调时生效，避免复制脱敏占位文本）。默认 `true`。 */
    copyable?: boolean
    autoHideAfterMs?: number
    className?: string
    id?: string
    idPrefix?: string
}

const defaultSensitiveValueAutoHideMs = 15_000
const sensitiveValueCopiedResetMs = 2_000

/**
 * 敏感值默认只接收脱敏文本。明文只能在用户主动操作后异步取得，并按时清除本地状态；
 * 具备查看权限时同步提供一键复制（始终复制明文而非脱敏占位文本）。
 */
function SensitiveValue({
    maskedValue,
    label = "敏感信息",
    onReveal,
    copyable = true,
    autoHideAfterMs = defaultSensitiveValueAutoHideMs,
    className,
    id,
    idPrefix,
}: SensitiveValueProps) {
    const baseId = idPrefix ?? id
    const [revealedValue, setRevealedValue] = React.useState<string | null>(
        null,
    )
    const [isRevealing, setIsRevealing] = React.useState(false)
    const [isCopying, setIsCopying] = React.useState(false)
    const [copied, setCopied] = React.useState(false)
    const [revealFailed, setRevealFailed] = React.useState(false)
    const requestSequence = React.useRef(0)
    const copiedTimerRef = React.useRef<number | null>(null)

    const clearCopiedTimer = React.useCallback(() => {
        if (copiedTimerRef.current !== null) {
            window.clearTimeout(copiedTimerRef.current)
            copiedTimerRef.current = null
        }
    }, [])

    React.useEffect(() => {
        return () => {
            requestSequence.current += 1
            clearCopiedTimer()
        }
    }, [clearCopiedTimer])

    React.useEffect(() => {
        if (revealedValue === null || autoHideAfterMs <= 0) return

        const timer = window.setTimeout(() => {
            requestSequence.current += 1
            setRevealedValue(null)
        }, autoHideAfterMs)

        return () => window.clearTimeout(timer)
    }, [autoHideAfterMs, revealedValue])

    const hide = () => {
        requestSequence.current += 1
        clearCopiedTimer()
        setRevealedValue(null)
        setRevealFailed(false)
        setIsRevealing(false)
        setIsCopying(false)
        setCopied(false)
    }

    const reveal = async () => {
        if (!onReveal || isRevealing) return

        const request = requestSequence.current + 1
        requestSequence.current = request
        setIsRevealing(true)
        setRevealFailed(false)

        try {
            const value = await onReveal()
            if (requestSequence.current === request) {
                setRevealedValue(value)
            }
        } catch {
            if (requestSequence.current === request) {
                setRevealFailed(true)
            }
        } finally {
            if (requestSequence.current === request) {
                setIsRevealing(false)
            }
        }
    }

    const copy = async () => {
        if (!onReveal || isCopying || isRevealing) return

        const request = requestSequence.current + 1
        requestSequence.current = request
        setIsCopying(true)
        setRevealFailed(false)

        let plainText: string
        try {
            plainText = revealedValue ?? (await onReveal())
        } catch {
            if (requestSequence.current === request) {
                setRevealFailed(true)
                setIsCopying(false)
            }
            return
        }

        if (requestSequence.current !== request) return

        if (typeof navigator === "undefined" || !navigator.clipboard) {
            setRevealedValue(plainText)
            setIsCopying(false)
            toast.add({
                title: "当前环境不支持一键复制，已显示明文请手动复制",
                type: "warning",
            })
            return
        }

        try {
            await navigator.clipboard.writeText(plainText)
            if (requestSequence.current !== request) return
            setCopied(true)
            clearCopiedTimer()
            copiedTimerRef.current = window.setTimeout(() => {
                setCopied(false)
                copiedTimerRef.current = null
            }, sensitiveValueCopiedResetMs)
            toast.add({
                title: `已复制${label}`,
                type: "success",
            })
        } catch {
            if (requestSequence.current !== request) return
            setRevealedValue(plainText)
            toast.add({
                title: "复制失败，已显示明文请手动选中复制",
                type: "error",
            })
        } finally {
            if (requestSequence.current === request) {
                setIsCopying(false)
            }
        }
    }

    const isRevealed = revealedValue !== null
    const displayValue = isRevealed ? revealedValue : maskedValue

    return (
        <div
            id={baseId}
            data-slot="sensitive-value"
            className={cn(
                "inline-flex flex-wrap items-center gap-2",
                className,
            )}
        >
            <span className="sr-only">{label}：</span>
            <code
                className={cn(
                    "num rounded-md bg-muted px-2 py-1 font-mono text-body-compact",
                    isRevealed && "select-all",
                )}
            >
                {displayValue}
            </code>
            {onReveal ? (
                <>
                    <Button
                        id={baseId ? `${baseId}-toggle` : undefined}
                        type="button"
                        variant="ghost"
                        size="xs"
                        aria-pressed={isRevealed}
                        aria-label={
                            isRevealed ? `隐藏${label}` : `显示${label}`
                        }
                        disabled={isRevealing || isCopying}
                        onClick={() => {
                            if (isRevealed) {
                                hide()
                                return
                            }

                            void reveal()
                        }}
                    >
                        {isRevealed ? (
                            <EyeOffIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        ) : (
                            <EyeIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        {isRevealing ? "读取中" : isRevealed ? "隐藏" : "显示"}
                    </Button>
                    {copyable ? (
                        <Button
                            id={baseId ? `${baseId}-copy` : undefined}
                            type="button"
                            variant="ghost"
                            size="xs"
                            aria-label={`复制${label}`}
                            disabled={isRevealing || isCopying}
                            onClick={() => {
                                void copy()
                            }}
                        >
                            {copied ? (
                                <CheckIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                            ) : (
                                <CopyIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                            )}
                            {isCopying ? "复制中" : copied ? "已复制" : "复制"}
                        </Button>
                    ) : null}
                </>
            ) : (
                <LockKeyholeIcon
                    className="size-4 text-muted-foreground"
                    aria-hidden="true"
                />
            )}
            {isRevealed && autoHideAfterMs > 0 ? (
                <span className="text-xs text-muted-foreground">
                    将在{" "}
                    <span className="num">
                        {Math.ceil(autoHideAfterMs / 1000)}
                    </span>{" "}
                    秒后自动隐藏
                </span>
            ) : null}
            {revealFailed ? (
                <span
                    role="alert"
                    className="flex flex-wrap items-center gap-1.5 text-xs text-destructive"
                >
                    <span>暂时无法显示敏感信息</span>
                    <Button
                        id={baseId ? `${baseId}-retry` : undefined}
                        type="button"
                        variant="link"
                        size="xs"
                        className="h-auto p-0"
                        onClick={() => void reveal()}
                    >
                        重试
                    </Button>
                </span>
            ) : null}
        </div>
    )
}

type DiscardConfirmDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    onConfirm: () => void
    title?: string
    description?: string
    confirmLabel?: string
    cancelLabel?: string
    id?: string
    idPrefix?: string
}

/**
 * 离开前放弃未保存输入的确认层。不负责 dirty 判定，由调用方决定何时弹出。
 */
function DiscardConfirmDialog({
    open,
    onOpenChange,
    onConfirm,
    title = "放弃未保存的更改？",
    description = "本次输入尚未保存，离开后将丢失。",
    confirmLabel = "放弃更改",
    cancelLabel = "继续编辑",
    id,
    idPrefix,
}: DiscardConfirmDialogProps) {
    const baseId = idPrefix ?? id
    return (
        <AlertDialog open={open} onOpenChange={onOpenChange}>
            <AlertDialogContent>
                <AlertDialogHeader>
                    <AlertDialogTitle>{title}</AlertDialogTitle>
                    <AlertDialogDescription>
                        {description}
                    </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                    <AlertDialogCancel
                        id={baseId ? `${baseId}-cancel` : undefined}
                    >
                        {cancelLabel}
                    </AlertDialogCancel>
                    <AlertDialogAction
                        id={baseId ? `${baseId}-confirm` : undefined}
                        variant="destructive"
                        onClick={onConfirm}
                    >
                        {confirmLabel}
                    </AlertDialogAction>
                </AlertDialogFooter>
            </AlertDialogContent>
        </AlertDialog>
    )
}

export {
    AsyncSectionState,
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    DiscardConfirmDialog,
    DraftSaveIndicator,
    FormalActionResult,
    GuardedBusinessAction,
    SensitiveValue,
    ValidationSummary,
    type AsyncSectionStateProps,
    type AsyncSectionStatus,
    type BackgroundJobMode,
    type BackgroundJobProgressProps,
    type BackgroundJobStatus,
    type BusinessEmptyStateKind,
    type BusinessEmptyStateProps,
    type BusinessFailureKind,
    type BusinessFailureStateProps,
    type DraftSaveIndicatorProps,
    type DraftSaveState,
    type DiscardConfirmDialogProps,
    type FormalActionFact,
    type FormalActionResultProps,
    type FormalActionResultStatus,
    type GuardedBusinessActionProps,
    type LegacyResultStateExtra,
    type ResultState,
    type ResultStateBase,
    type SensitiveValueProps,
    type ValidationIssue,
    type ValidationSummaryProps,
}
