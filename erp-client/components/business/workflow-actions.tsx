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
    RefreshCwIcon,
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
import { getErrorMessage } from "@/lib/api/errors"
import { leaseText, sequentialText } from "@/lib/ui-text"
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
            <ul className="mt-3 space-y-2 text-sm" role="list">
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
    nextDepartment?: React.ReactNode
    irreversibleEffects?: readonly React.ReactNode[]
    pending?: boolean
    onConfirm: () => void | Promise<void>
    onCancel?: () => void
    onConfirmError?: (error: unknown) => void
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
    nextDepartment,
    irreversibleEffects = [],
    pending = false,
    onConfirm,
    onCancel,
    onConfirmError,
    open,
    defaultOpen,
    onOpenChange,
}: FormalActionConfirmDialogProps) {
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

    return (
        <AlertDialog open={resolvedOpen} onOpenChange={setOpen}>
            {trigger ? <AlertDialogTrigger render={trigger} /> : null}
            <AlertDialogContent className="sm:max-w-xl">
                <AlertDialogHeader>
                    <AlertDialogMedia className="text-primary">
                        <FileCheck2Icon aria-hidden="true" />
                    </AlertDialogMedia>
                    <AlertDialogTitle>
                        {title ?? `确认${actionLabel}`}
                    </AlertDialogTitle>
                    <AlertDialogDescription>
                        {description ?? "请核对状态变化和业务影响后再继续。"}
                    </AlertDialogDescription>
                </AlertDialogHeader>

                <div className="space-y-4">
                    <section aria-label="状态变化">
                        <div className="flex flex-wrap items-center justify-center gap-3 rounded-xl border border-border bg-muted p-4 sm:justify-start">
                            <StatusBadge {...sourceStatus} />
                            <ArrowRightIcon
                                aria-label="变更为"
                                className="size-4 text-muted-foreground"
                            />
                            <StatusBadge {...targetStatus} />
                        </div>
                    </section>

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

                {confirmError != null && onConfirmError == null ? (
                    <Alert variant="destructive">
                        <TriangleAlertIcon aria-hidden="true" />
                        <AlertTitle>操作未完成</AlertTitle>
                        <AlertDescription>{confirmError}</AlertDescription>
                    </Alert>
                ) : null}

                <AlertDialogFooter>
                    <AlertDialogCancel disabled={isPending} onClick={onCancel}>
                        <ArrowLeftIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {cancelLabel}
                    </AlertDialogCancel>
                    <AlertDialogAction
                        variant={
                            irreversibleEffects.length > 0
                                ? "destructive"
                                : "default"
                        }
                        disabled={isPending}
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

export type SequentialLeaseStatus =
    | "unclaimed"
    | "active"
    | "renewing"
    | "lost"
    | "released"

const sequentialLeaseStatus = {
    unclaimed: {
        label: leaseText.unclaimed,
        tone: "neutral",
        icon: CircleDashedIcon,
    },
    active: {
        label: leaseText.active,
        tone: "success",
        icon: ShieldCheckIcon,
    },
    renewing: {
        label: leaseText.renewing,
        tone: "info",
        icon: RefreshCwIcon,
    },
    lost: {
        label: leaseText.lost,
        tone: "destructive",
        icon: ShieldAlertIcon,
    },
    released: {
        label: leaseText.released,
        tone: "neutral",
        icon: CircleDashedIcon,
    },
} satisfies Record<
    SequentialLeaseStatus,
    { label: string; tone: StatusTone; icon: LucideIcon }
>

export type SequentialProcessBarProps = Omit<
    React.ComponentProps<"section">,
    "children"
> & {
    current: number
    total: number
    leaseStatus: SequentialLeaseStatus
    leaseStatusLabel?: string
    processLabel?: string
    processNextLabel?: string
    pending?: boolean
    processDisabled?: boolean
    /**
     * 与 `processDisabled` 独立：只读翻页场景（未领取也能看下一条）传 `false`。
     * 默认跟随 `processDisabled`，保持既有「需要领取才能处理」行为。
     */
    processNextDisabled?: boolean
    /**
     * 返回按钮文案。行为由 `onBack` 决定；跳转目标不是队列页时（如回工作台），
     * 必须传与行为一致的文案（按钮说动作，不说机制）。
     */
    backLabel?: string
    /** 首次领取按钮文案（从未领取过）；默认「领取任务」。 */
    claimLabel?: string
    /** 处理权丢失后的重新领取按钮文案；默认「重新领取」。 */
    reclaimLabel?: string
    /** 主动作会离开当前页面（如跳转专用处理器）时置 false，避免两个同义按钮。 */
    showProcessNext?: boolean
    /**
     * 只读角色（如销售/财务查看进度）置 false：
     * 不渲染主动作与「重新领取」，避免展示一排点不动的按钮。
     */
    showProcess?: boolean
    /**
     * 插入状态徽章区（位置/租约之后），例如先款条件结果徽章。
     * 不改变默认布局；各工作面按需传入。
     */
    statusExtras?: React.ReactNode
    onBack: () => void
    onProcess: () => void
    onProcessNext: () => void
    onReclaim: () => void
}

/** 连续审核/确认页的队列位置、租约状态和处理动作。 */
function SequentialProcessBar({
    current,
    total,
    leaseStatus,
    leaseStatusLabel,
    processLabel = "处理当前任务",
    processNextLabel = "处理并打开下一条",
    pending = false,
    processDisabled = false,
    processNextDisabled,
    backLabel = "返回队列",
    claimLabel = sequentialText.claim,
    reclaimLabel = sequentialText.reclaim,
    showProcessNext = true,
    showProcess = true,
    statusExtras,
    onBack,
    onProcess,
    onProcessNext,
    onReclaim,
    className,
    ...props
}: SequentialProcessBarProps) {
    const lease = sequentialLeaseStatus[leaseStatus]
    const canProcess = leaseStatus === "active" && !pending && !processDisabled
    const canProcessNext =
        processNextDisabled !== undefined
            ? !pending && !processNextDisabled
            : canProcess
    const canReclaim =
        showProcess && (leaseStatus === "unclaimed" || leaseStatus === "lost")
    const isFirstClaim = leaseStatus === "unclaimed"
    const claimButtonLabel = isFirstClaim ? claimLabel : reclaimLabel
    const claimPendingLabel = isFirstClaim
        ? sequentialText.claiming
        : sequentialText.reclaiming

    return (
        <section
            data-slot="sequential-process-bar"
            aria-label="连续处理操作"
            className={cn(
                "flex flex-col gap-4 rounded-2xl border border-border bg-card p-4 lg:flex-row lg:items-center lg:justify-between",
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
                    tone={lease.tone}
                    icon={lease.icon}
                    label={leaseStatusLabel ?? lease.label}
                />
                {statusExtras}
                {leaseStatus === "lost" ? (
                    <span className="text-sm text-destructive">
                        当前输入会保留，重新领取后才能提交。
                    </span>
                ) : null}
            </div>

            <div className="flex flex-wrap items-center gap-2">
                <Button
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

                {canReclaim ? (
                    <Button
                        type="button"
                        variant="secondary"
                        disabled={pending}
                        onClick={onReclaim}
                    >
                        {pending ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                                className="animate-spin"
                            />
                        ) : (
                            <RefreshCwIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        {pending ? claimPendingLabel : claimButtonLabel}
                    </Button>
                ) : null}

                {showProcess ? (
                    <Button
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
                            <CheckIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        {pending ? "正在处理" : processLabel}
                    </Button>
                ) : null}
                {showProcess && showProcessNext ? (
                    <Button
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
}

export {
    FormalActionConfirmDialog,
    SequentialProcessBar,
}
