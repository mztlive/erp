"use client"

import * as React from "react"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
    CheckIcon,
    CircleCheckIcon,
    CircleDashedIcon,
    FileCheck2Icon,
    ListChecksIcon,
    LockIcon,
    TriangleAlertIcon,
    UsersRoundIcon,
    type LucideIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogMedia,
    AlertDialogTitle,
    AlertDialogTrigger,
} from "@/components/ui/alert-dialog"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { getErrorMessage } from "@/lib/api/errors"
import { cn } from "@/lib/utils"

export type ControllableDialogProps = {
    open?: boolean
    defaultOpen?: boolean
    onOpenChange?: (open: boolean) => void
}

export function useControllableDialog({
    open,
    defaultOpen = false,
    onOpenChange,
}: ControllableDialogProps) {
    const [internalOpen, setInternalOpen] = React.useState(defaultOpen)
    const resolvedOpen = open ?? internalOpen

    const setOpen = React.useCallback(
        (nextOpen: boolean) => {
            if (open === undefined) {
                setInternalOpen(nextOpen)
            }
            onOpenChange?.(nextOpen)
        },
        [onOpenChange, open],
    )

    return [resolvedOpen, setOpen] as const
}

export type WorkflowStatus =
    | string
    | Readonly<{
          label: string
          tone?: StatusTone
          icon?: LucideIcon
      }>

function normalizeStatus(
    status: WorkflowStatus,
    fallbackTone: StatusTone,
    fallbackIcon: LucideIcon,
) {
    if (typeof status === "string") {
        return {
            label: status,
            tone: fallbackTone,
            icon: fallbackIcon,
        }
    }

    return {
        label: status.label,
        tone: status.tone ?? fallbackTone,
        icon: status.icon ?? fallbackIcon,
    }
}

/** 把提交异常转成可读消息；未知异常给通用下一步指引。 */
function messageFromError(error: unknown): string {
    return getErrorMessage(error, "操作未完成，请稍后重试。")
}

type WorkflowDetailListProps = {
    title: string
    icon: LucideIcon
    items: readonly React.ReactNode[]
    tone?: "default" | "destructive"
}

function StatusChangeStrip({
    sourceStatus,
    targetStatus,
    compact = false,
}: {
    sourceStatus: ReturnType<typeof normalizeStatus>
    targetStatus: ReturnType<typeof normalizeStatus>
    compact?: boolean
}) {
    return (
        <section
            aria-label="状态变化"
            className={cn(
                "flex flex-wrap items-center gap-2",
                !compact &&
                    "justify-center gap-3 rounded-xl border border-border bg-muted p-4 sm:justify-start",
            )}
        >
            <StatusBadge {...sourceStatus} />
            <ArrowRightIcon
                aria-label="变更为"
                className="size-4 text-muted-foreground"
            />
            <StatusBadge {...targetStatus} />
        </section>
    )
}

function WorkflowDetailList({
    title,
    icon: Icon,
    items,
    tone = "default",
}: WorkflowDetailListProps) {
    if (items.length === 0) {
        return null
    }

    return (
        <section
            className={cn(
                "rounded-xl border p-4",
                tone === "destructive"
                    ? "border-destructive-border bg-destructive-soft"
                    : "border-border bg-muted",
            )}
        >
            <h3
                className={cn(
                    "flex items-center gap-2 text-body-compact font-medium",
                    tone === "destructive"
                        ? "text-destructive-soft-foreground"
                        : "text-foreground",
                )}
            >
                <Icon aria-hidden="true" className="size-4" />
                {title}
            </h3>
            <ul className="mt-3 space-y-2 text-body-compact">
                {items.map((item, index) => (
                    <li
                        key={index}
                        className={cn(
                            "flex items-start gap-2",
                            tone === "destructive"
                                ? "text-destructive-soft-foreground"
                                : "text-muted-foreground",
                        )}
                    >
                        {tone === "destructive" ? (
                            <TriangleAlertIcon
                                aria-hidden="true"
                                className="mt-0.5 size-3.5 shrink-0"
                            />
                        ) : (
                            <CheckIcon
                                aria-hidden="true"
                                className="mt-0.5 size-4 shrink-0"
                            />
                        )}
                        <span className="min-w-0">{item}</span>
                    </li>
                ))}
            </ul>
        </section>
    )
}

export type FormalActionConfirmDialogProps = ControllableDialogProps & {
    trigger?: React.ReactElement
    title?: string
    description?: React.ReactNode
    actionLabel: string
    confirmLabel?: string
    cancelLabel?: string
    fromStatus: WorkflowStatus
    toStatus: WorkflowStatus
    /** 创建关联草稿等不改变原单状态的动作可隐藏状态流转。 */
    showStatusChange?: boolean
    /** 提交前需要核对的业务对象、金额等摘要。 */
    summary?: readonly React.ReactNode[]
    summaryTitle?: string
    /** 显式指定动作风险；未传时兼容既有调用。 */
    actionVariant?: "default" | "destructive"
    lockedFields?: readonly React.ReactNode[]
    effects?: readonly React.ReactNode[]
    /** 需要随正式动作一并提交的显式业务字段。 */
    formContent?: React.ReactNode
    /** 覆盖确认层样式；默认宽度由 layout 决定（lg / xl）。 */
    contentClassName?: string
    /** 横版加宽确认层；紧凑版省略装饰图标，将状态放到标题旁。 */
    layout?: "stack" | "landscape" | "compact"
    nextDepartment?: React.ReactNode
    irreversibleEffects?: readonly React.ReactNode[]
    pending?: boolean
    confirmDisabled?: boolean
    onConfirm: () => void | Promise<void>
    onCancel?: () => void
    onConfirmError?: (error: unknown) => void
    id?: string
    idPrefix?: string
}

/**
 * 正式业务动作的影响确认层。
 *
 * 只负责呈现状态变化与影响，并调用业务层回调；不发请求，也不管理审批表单值。
 */
function FormalActionConfirmDialog({
    trigger,
    title,
    description,
    actionLabel,
    confirmLabel,
    cancelLabel = "返回修改",
    fromStatus,
    toStatus,
    showStatusChange = true,
    summary = [],
    summaryTitle = "核对信息",
    actionVariant,
    lockedFields = [],
    effects = [],
    formContent,
    contentClassName,
    layout = "stack",
    nextDepartment,
    irreversibleEffects = [],
    pending = false,
    confirmDisabled = false,
    onConfirm,
    onCancel,
    onConfirmError,
    id,
    idPrefix,
    open,
    defaultOpen,
    onOpenChange,
}: FormalActionConfirmDialogProps) {
    const baseId = idPrefix ?? id ?? "workflow-formal-action"
    const [resolvedOpen, setOpen] = useControllableDialog({
        open,
        defaultOpen,
        onOpenChange,
    })
    const confirmRunning = React.useRef(false)
    const [internalPending, setInternalPending] = React.useState(false)
    const [confirmError, setConfirmError] = React.useState<string | null>(null)
    const isPending = pending || internalPending
    React.useEffect(() => {
        if (!resolvedOpen) {
            setConfirmError(null)
        }
    }, [resolvedOpen])
    const isDestructive =
        (actionVariant ??
            (irreversibleEffects.length > 0 ? "destructive" : "default")) ===
        "destructive"
    const normalizedAction = actionLabel.replace(/^确认/, "") || actionLabel
    const resolvedTitle =
        title ??
        (actionLabel.startsWith("确认") ? actionLabel : `确认${actionLabel}`)
    const resolvedConfirmLabel =
        confirmLabel ??
        (actionLabel.startsWith("确认") ? actionLabel : `确认${actionLabel}`)
    const resolvedDescription =
        description === undefined
            ? showStatusChange
                ? "请核对状态变化和业务影响后再继续。"
                : "请核对本次操作的业务影响后再继续。"
            : description
    const sourceStatus = normalizeStatus(
        fromStatus,
        "neutral",
        CircleDashedIcon,
    )
    const targetStatus = normalizeStatus(toStatus, "info", CircleCheckIcon)

    const handleConfirm = React.useCallback(async () => {
        if (confirmRunning.current || pending) return
        confirmRunning.current = true
        setInternalPending(true)
        setConfirmError(null)
        try {
            await onConfirm()
            setOpen(false)
        } catch (error) {
            setConfirmError(messageFromError(error))
            onConfirmError?.(error)
        } finally {
            confirmRunning.current = false
            setInternalPending(false)
        }
    }, [onConfirm, onConfirmError, setOpen, pending])

    const hasDetails =
        summary.length > 0 ||
        lockedFields.length > 0 ||
        effects.length > 0 ||
        formContent != null ||
        nextDepartment != null ||
        irreversibleEffects.length > 0

    const header =
        layout === "compact" ? (
            <AlertDialogHeader className="block shrink-0 space-y-2 text-left">
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <AlertDialogTitle>{resolvedTitle}</AlertDialogTitle>
                    {showStatusChange ? (
                        <StatusChangeStrip
                            sourceStatus={sourceStatus}
                            targetStatus={targetStatus}
                            compact
                        />
                    ) : null}
                </div>
                <AlertDialogDescription render={<div />}>
                    {resolvedDescription}
                </AlertDialogDescription>
            </AlertDialogHeader>
        ) : (
            <AlertDialogHeader
                className={cn(
                    "place-items-start justify-items-start text-left has-data-[slot=alert-dialog-media]:grid-cols-[auto_minmax(0,1fr)]",
                    layout === "stack" && "shrink-0",
                )}
            >
                <AlertDialogMedia
                    className={cn(
                        "row-span-2 mb-0 self-start",
                        isDestructive
                            ? "bg-destructive-soft text-destructive-soft-foreground"
                            : "bg-info-soft text-info-soft-foreground",
                    )}
                >
                    {isDestructive ? (
                        <TriangleAlertIcon aria-hidden="true" />
                    ) : (
                        <FileCheck2Icon aria-hidden="true" />
                    )}
                </AlertDialogMedia>
                <AlertDialogTitle>{resolvedTitle}</AlertDialogTitle>
                <AlertDialogDescription
                    render={<div />}
                    className="col-start-2 text-left"
                >
                    {resolvedDescription}
                </AlertDialogDescription>
                {layout === "stack" && showStatusChange ? (
                    <div className="col-start-2 mt-1">
                        <StatusChangeStrip
                            sourceStatus={sourceStatus}
                            targetStatus={targetStatus}
                            compact
                        />
                    </div>
                ) : null}
            </AlertDialogHeader>
        )

    return (
        <AlertDialog
            open={resolvedOpen}
            onOpenChange={(next) => {
                if (!isPending) setOpen(next)
            }}
        >
            {trigger ? <AlertDialogTrigger render={trigger} /> : null}
            <AlertDialogContent
                size={layout === "landscape" ? "xl" : "lg"}
                className={cn(
                    "flex max-h-[calc(100dvh-2rem)] flex-col overflow-hidden",
                    contentClassName,
                )}
            >
                {layout === "landscape" ? (
                    <div className="grid shrink-0 items-center gap-4 sm:grid-cols-[minmax(0,1fr)_auto]">
                        {header}
                        {showStatusChange ? (
                            <StatusChangeStrip
                                sourceStatus={sourceStatus}
                                targetStatus={targetStatus}
                                compact
                            />
                        ) : null}
                    </div>
                ) : (
                    header
                )}

                {hasDetails ||
                (confirmError != null && onConfirmError == null) ? (
                    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-0.5">
                        {hasDetails ? (
                            <div className="flex flex-col gap-4">
                                <WorkflowDetailList
                                    title={summaryTitle}
                                    icon={ListChecksIcon}
                                    items={summary}
                                />
                                <WorkflowDetailList
                                    title="提交后锁定字段"
                                    icon={LockIcon}
                                    items={lockedFields}
                                />
                                <WorkflowDetailList
                                    title="本次动作产生的影响"
                                    icon={ListChecksIcon}
                                    items={effects}
                                />

                                {formContent}

                                {nextDepartment != null ? (
                                    <Alert>
                                        <UsersRoundIcon aria-hidden="true" />
                                        <AlertTitle>下一责任部门</AlertTitle>
                                        <AlertDescription>
                                            {nextDepartment}
                                        </AlertDescription>
                                    </Alert>
                                ) : null}

                                <WorkflowDetailList
                                    title="无法自动撤回的影响"
                                    icon={TriangleAlertIcon}
                                    items={irreversibleEffects}
                                    tone="destructive"
                                />
                            </div>
                        ) : null}

                        {confirmError != null && onConfirmError == null ? (
                            <Alert variant="destructive">
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertTitle>操作未完成</AlertTitle>
                                <AlertDescription>
                                    {confirmError}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </div>
                ) : null}

                <AlertDialogFooter className="shrink-0">
                    <AlertDialogCancel
                        id={`${baseId}-cancel`}
                        disabled={isPending}
                        onClick={onCancel}
                    >
                        <ArrowLeftIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {cancelLabel}
                    </AlertDialogCancel>
                    <AlertDialogAction
                        id={`${baseId}-confirm`}
                        variant={isDestructive ? "destructive" : "default"}
                        disabled={isPending || confirmDisabled}
                        loading={isPending}
                        onClick={() => void handleConfirm()}
                    >
                        {!isPending ? (
                            <CheckIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        ) : null}
                        {isPending
                            ? `正在${normalizedAction}…`
                            : resolvedConfirmLabel}
                    </AlertDialogAction>
                </AlertDialogFooter>
            </AlertDialogContent>
        </AlertDialog>
    )
}

export type ResponsibilityStatus =
    | "assigned_to_me"
    | "assigned_to_other"
    | "blocked"
    | "completed"
    | "closed"

export { FormalActionConfirmDialog }
