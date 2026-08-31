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
    LoaderCircleIcon,
    LockIcon,
    ShieldAlertIcon,
    ShieldCheckIcon,
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
import { Button } from "@/components/ui/button"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { getErrorMessage } from "@/lib/api/errors"
import { responsibilityText } from "@/lib/ui-text"
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
                    "flex items-center gap-2 text-sm font-medium",
                    tone === "destructive"
                        ? "text-destructive-soft-foreground"
                        : "text-foreground",
                )}
            >
                <Icon aria-hidden="true" className="size-4" />
                {title}
            </h3>
            <ul className="mt-3 space-y-2 text-sm">
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
                        <CheckIcon
                            aria-hidden="true"
                            className="mt-0.5 size-4 shrink-0"
                        />
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
    lockedFields?: readonly React.ReactNode[]
    effects?: readonly React.ReactNode[]
    /** 需要随正式动作一并提交的显式业务字段。 */
    formContent?: React.ReactNode
    /** 覆盖确认层宽度等布局，默认 `sm:max-w-xl`。 */
    contentClassName?: string
    /** 横版把状态变化放到标题行右侧，并加宽确认层。 */
    layout?: "stack" | "landscape"
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
    confirmLabel = `确认${actionLabel}`,
    cancelLabel = "返回修改",
    fromStatus,
    toStatus,
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
    const [internalPending, setInternalPending] = React.useState(false)
    const [confirmError, setConfirmError] = React.useState<string | null>(null)
    const isPending = pending || internalPending
    const sourceStatus = normalizeStatus(
        fromStatus,
        "neutral",
        CircleDashedIcon,
    )
    const targetStatus = normalizeStatus(toStatus, "info", CircleCheckIcon)

    const handleConfirm = React.useCallback(async () => {
        setInternalPending(true)
        setConfirmError(null)
        try {
            await onConfirm()
            setOpen(false)
        } catch (error) {
            setConfirmError(messageFromError(error))
            onConfirmError?.(error)
        } finally {
            setInternalPending(false)
        }
    }, [onConfirm, onConfirmError, setOpen])

    const hasDetails =
        lockedFields.length > 0 ||
        effects.length > 0 ||
        formContent != null ||
        nextDepartment != null ||
        irreversibleEffects.length > 0

    const header = (
        <AlertDialogHeader className="place-items-start justify-items-start text-left has-data-[slot=alert-dialog-media]:grid-cols-[auto_minmax(0,1fr)]">
            <AlertDialogMedia className="row-span-2 mb-0 self-start text-primary">
                <FileCheck2Icon aria-hidden="true" />
            </AlertDialogMedia>
            <AlertDialogTitle>{title ?? `确认${actionLabel}`}</AlertDialogTitle>
            <AlertDialogDescription
                render={<div />}
                className="col-start-2 text-left"
            >
                {description ?? "请核对状态变化和业务影响后再继续。"}
            </AlertDialogDescription>
            {layout === "stack" ? (
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
        <AlertDialog open={resolvedOpen} onOpenChange={setOpen}>
            {trigger ? <AlertDialogTrigger render={trigger} /> : null}
            <AlertDialogContent
                className={cn(
                    layout === "landscape"
                        ? "sm:max-w-4xl data-[size=default]:sm:max-w-4xl"
                        : "sm:max-w-xl data-[size=default]:sm:max-w-xl",
                    contentClassName,
                )}
            >
                {layout === "landscape" ? (
                    <div className="grid items-center gap-4 sm:grid-cols-[minmax(0,1fr)_auto]">
                        {header}
                        <StatusChangeStrip
                            sourceStatus={sourceStatus}
                            targetStatus={targetStatus}
                            compact
                        />
                    </div>
                ) : (
                    header
                )}

                {hasDetails ? (
                    <div className="flex flex-col gap-4">
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
                        <AlertDescription>{confirmError}</AlertDescription>
                    </Alert>
                ) : null}

                <AlertDialogFooter>
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
                        variant={
                            irreversibleEffects.length > 0
                                ? "destructive"
                                : "default"
                        }
                        disabled={isPending || confirmDisabled}
                        onClick={() => void handleConfirm()}
                    >
                        {isPending ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                                className="animate-spin"
                            />
                        ) : (
                            <CheckIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        {isPending ? `正在${actionLabel}` : confirmLabel}
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

const responsibilityStatusMeta = {
    assigned_to_me: {
        label: responsibilityText.assignedToMe,
        tone: "success",
        icon: ShieldCheckIcon,
    },
    assigned_to_other: {
        label: responsibilityText.assignedToOther,
        tone: "neutral",
        icon: UsersRoundIcon,
    },
    blocked: {
        label: responsibilityText.blocked,
        tone: "warning",
        icon: ShieldAlertIcon,
    },
    completed: {
        label: responsibilityText.completed,
        tone: "success",
        icon: CircleCheckIcon,
    },
    closed: {
        label: responsibilityText.closed,
        tone: "neutral",
        icon: CircleDashedIcon,
    },
} satisfies Record<
    ResponsibilityStatus,
    { label: string; tone: StatusTone; icon: LucideIcon }
>

export type SequentialProcessBarProps = Omit<
    React.ComponentProps<"section">,
    "children"
> & {
    id?: string
    idPrefix?: string
    current: number
    total: number
    responsibilityStatus: ResponsibilityStatus
    responsibilityStatusLabel?: string
    processLabel?: string
    processIcon?: LucideIcon
    processNextLabel?: string
    pending?: boolean
    processDisabled?: boolean
    /**
     * 与 `processDisabled` 独立：只读翻页场景也可继续浏览。
     * 默认跟随 `processDisabled`。
     */
    processNextDisabled?: boolean
    /**
     * 返回按钮文案。行为由 `onBack` 决定；跳转目标不是队列页时（如回工作台），
     * 必须传与行为一致的文案（按钮说动作，不说机制）。
     */
    backLabel?: string
    /** 已嵌入对象详情时可隐藏返回按钮；默认保留。 */
    showBack?: boolean
    /** 主动作会离开当前页面（如跳转专用处理器）时置 false，避免两个同义按钮。 */
    showProcessNext?: boolean
    /**
     * 只读角色（如销售/财务查看进度）置 false：
     * 不渲染业务主动作。
     */
    showProcess?: boolean
    /**
     * 插入状态徽章区（位置/责任之后），例如先款条件结果徽章。
     * 不改变默认布局；各工作面按需传入。
     */
    statusExtras?: React.ReactNode
    onBack: () => void
    onProcess: () => void
    onProcessNext: () => void
}

/** 连续审核/确认页的队列位置、当前责任和处理动作。 */
function SequentialProcessBar({
    id,
    idPrefix,
    current,
    total,
    responsibilityStatus,
    responsibilityStatusLabel,
    processLabel = "处理当前任务",
    processIcon: ProcessIcon = CheckIcon,
    processNextLabel = "处理并打开下一条",
    pending = false,
    processDisabled = false,
    processNextDisabled,
    backLabel = "返回队列",
    showBack = true,
    showProcessNext = true,
    showProcess = true,
    statusExtras,
    onBack,
    onProcess,
    onProcessNext,
    className,
    ...props
}: SequentialProcessBarProps) {
    const baseId = idPrefix ?? id ?? "sequential-process-bar"
    const responsibility = responsibilityStatusMeta[responsibilityStatus]
    const canProcess =
        responsibilityStatus === "assigned_to_me" &&
        !pending &&
        !processDisabled
    const canProcessNext =
        processNextDisabled !== undefined
            ? !pending && !processNextDisabled
            : canProcess
    const bar = (
        <section
            data-slot="sequential-process-bar"
            aria-label="连续处理操作"
            className={cn(
                "erp-raised-surface flex flex-col gap-4 rounded-2xl border border-border bg-card p-4 lg:flex-row lg:items-center lg:justify-between",
                className,
            )}
            {...props}
        >
            <div
                className="flex min-w-0 flex-wrap items-center gap-3"
                aria-live="polite"
            >
                <StatusBadge
                    tone="info"
                    icon={ListChecksIcon}
                    label={`第 ${current.toLocaleString("zh-CN")} / ${total.toLocaleString("zh-CN")} 条`}
                />
                <StatusBadge
                    tone={responsibility.tone}
                    icon={responsibility.icon}
                    label={responsibilityStatusLabel ?? responsibility.label}
                />
                {statusExtras}
                {responsibilityStatus === "assigned_to_other" ? (
                    <span className="text-sm text-destructive">
                        {responsibilityText.changed}
                    </span>
                ) : null}
            </div>

            <div className="flex flex-wrap items-center gap-2">
                {showBack ? (
                    <Button
                        id={`${baseId}-back`}
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={onBack}
                    >
                        <ArrowLeftIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {backLabel}
                    </Button>
                ) : null}

                {showProcess ? (
                    <Button
                        id={`${baseId}-process`}
                        type="button"
                        /* 隐藏「并打开下一条」时，本按钮就是唯一主动作 */
                        variant={showProcessNext ? "secondary" : "default"}
                        disabled={!canProcess}
                        onClick={onProcess}
                    >
                        {pending ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                                className="animate-spin"
                            />
                        ) : (
                            <ProcessIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        {pending ? "正在处理" : processLabel}
                    </Button>
                ) : null}
                {showProcess && showProcessNext ? (
                    <Button
                        id={`${baseId}-process-next`}
                        type="button"
                        disabled={!canProcessNext}
                        onClick={onProcessNext}
                    >
                        {pending ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                                className="animate-spin"
                            />
                        ) : (
                            <ArrowRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        )}
                        {pending ? "正在处理" : processNextLabel}
                    </Button>
                ) : null}
            </div>
        </section>
    )
    if (!showProcess) return bar
    return <WorkspaceTaskFooter fallback={bar}>{bar}</WorkspaceTaskFooter>
}

export { FormalActionConfirmDialog, SequentialProcessBar }
