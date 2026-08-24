"use client"

import * as React from "react"
import {
    CircleCheckIcon,
    ClipboardCheckIcon,
    CloudOffIcon,
    DatabaseIcon,
    FileWarningIcon,
    GitCompareArrowsIcon,
    RefreshCwIcon,
    RotateCcwIcon,
    SearchXIcon,
    ServerCrashIcon,
    ShieldAlertIcon,
    ShieldXIcon,
    TriangleAlertIcon,
    type LucideIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Empty,
    EmptyContent,
    EmptyDescription,
    EmptyHeader,
    EmptyMedia,
    EmptyTitle,
} from "@/components/ui/empty"
import { Kbd } from "@/components/ui/kbd"
import { Skeleton } from "@/components/ui/skeleton"
import { Spinner } from "@/components/ui/spinner"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { getErrorPresentation } from "@/lib/api/errors"
import { freshnessText, versionText, workspaceLabel } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

type GuardedBusinessActionProps = Omit<
    React.ComponentProps<typeof Button>,
    "disabled"
> & {
    disabled?: boolean
    reason?: string
    nextResponsible?: string
    shortcut?: string
}

/**
 * 保留不可用动作的位置，并把禁用原因、下一责任人和快捷键放进可聚焦说明层。
 */
function GuardedBusinessAction({
    disabled = false,
    reason,
    nextResponsible,
    shortcut,
    children,
    ...buttonProps
}: GuardedBusinessActionProps) {
    const hasExplanation = Boolean(
        disabled || reason || nextResponsible || shortcut,
    )
    const explanation =
        reason ?? (disabled ? "当前状态不允许执行此操作" : undefined)

    if (!hasExplanation) {
        return <Button {...buttonProps}>{children}</Button>
    }

    const content = (
        <div className="flex flex-col items-start gap-1">
            {explanation ? <span>{explanation}</span> : null}
            {nextResponsible ? (
                <span className="text-background/80">
                    下一责任人：{nextResponsible}
                </span>
            ) : null}
            {shortcut ? (
                <span className="flex items-center gap-1.5 text-background/80">
                    快捷键 <Kbd>{shortcut}</Kbd>
                </span>
            ) : null}
        </div>
    )

    return (
        <TooltipProvider>
            <Tooltip>
                {disabled ? (
                    <TooltipTrigger
                        render={
                            <span
                                className="inline-flex cursor-not-allowed"
                                tabIndex={0}
                                role="button"
                                aria-disabled="true"
                                aria-label={`操作不可用：${explanation}`}
                            />
                        }
                    >
                        <Button disabled {...buttonProps}>
                            {children}
                        </Button>
                    </TooltipTrigger>
                ) : (
                    <TooltipTrigger render={<Button {...buttonProps} />}>
                        {children}
                    </TooltipTrigger>
                )}
                <TooltipContent>{content}</TooltipContent>
            </Tooltip>
        </TooltipProvider>
    )
}

type BusinessEmptyStateKind =
    | "no-data"
    | "filter"
    | "no-scope"
    | "not-synced"
    | "no-tasks"
    | "no-exceptions"

type EmptyStatePreset = {
    title: string
    description: string
    icon: LucideIcon
}

const emptyStatePresets: Record<BusinessEmptyStateKind, EmptyStatePreset> = {
    "no-data": {
        title: "尚无业务数据",
        description: "当前范围内还没有可展示的业务记录。",
        icon: DatabaseIcon,
    },
    filter: {
        title: "当前筛选无结果",
        description: "没有记录符合当前条件，可调整或清除筛选后重试。",
        icon: SearchXIcon,
    },
    "no-scope": {
        title: "当前角色无数据范围",
        description: "你可以进入此页面，但当前权限范围内没有可查看的数据。",
        icon: ShieldXIcon,
    },
    "not-synced": {
        title: "尚未完成同步",
        description: "来源系统尚未完成首次同步，可前往后台任务查看进度。",
        icon: CloudOffIcon,
    },
    "no-tasks": {
        title: "当前没有待处理事项",
        description: "当前工作范围内没有需要你处理的任务。",
        icon: ClipboardCheckIcon,
    },
    "no-exceptions": {
        title: "当前没有未解决异常",
        description: "当前筛选范围内的业务链路运行正常。",
        icon: CircleCheckIcon,
    },
}

type BusinessEmptyStateProps = {
    kind: BusinessEmptyStateKind
    title?: string
    description?: React.ReactNode
    icon?: LucideIcon
    action?: React.ReactNode
    className?: string
}

function BusinessEmptyState({
    kind,
    title,
    description,
    icon,
    action,
    className,
}: BusinessEmptyStateProps) {
    const preset = emptyStatePresets[kind]
    const Icon = icon ?? preset.icon

    return (
        <Empty
            data-slot="business-empty-state"
            data-kind={kind}
            role="status"
            className={cn(
                // 默认轻浮起、弱分割；嵌在主卡内时由页面 className 去掉边框
                "border-0 bg-muted/20 ring-1 ring-foreground/[0.04]",
                className,
            )}
        >
            <EmptyHeader>
                <EmptyMedia variant="icon">
                    <Icon aria-hidden="true" />
                </EmptyMedia>
                <EmptyTitle>{title ?? preset.title}</EmptyTitle>
                <EmptyDescription>
                    {description ?? preset.description}
                </EmptyDescription>
            </EmptyHeader>
            {action ? <EmptyContent>{action}</EmptyContent> : null}
        </Empty>
    )
}

type BusinessFailureKind =
    | "validation"
    | "business"
    | "permission"
    | "conflict"
    | "system"
    | "sync"
    | "integration"
    | "projection"

type AlertVariant = React.ComponentProps<typeof Alert>["variant"]

type FailureStatePreset = {
    title: string
    description: string
    icon: LucideIcon
    variant: AlertVariant
    tone: StatusTone
    label: string
}

const failureStatePresets: Record<BusinessFailureKind, FailureStatePreset> = {
    validation: {
        title: "存在待修正字段",
        description: "请按提示修正输入后再次提交。",
        icon: FileWarningIcon,
        variant: "warning",
        tone: "warning",
        label: "校验未通过",
    },
    business: {
        title: "业务规则阻断",
        description: "当前业务状态或前置条件不允许继续处理。",
        icon: TriangleAlertIcon,
        variant: "warning",
        tone: "warning",
        label: "业务阻断",
    },
    permission: {
        title: "权限不足",
        description: "当前账号缺少所需的页面、动作或数据范围权限。",
        icon: ShieldAlertIcon,
        variant: "destructive",
        tone: "destructive",
        label: "权限限制",
    },
    conflict: {
        title: versionText.versionChanged,
        description: "数据已更新，请比较差异后重新处理。",
        icon: GitCompareArrowsIcon,
        variant: "warning",
        tone: "warning",
        label: "数据已更新",
    },
    system: {
        title: "系统暂时无法完成操作",
        description: "输入已保留，可根据错误编号重试或联系支持人员。",
        icon: ServerCrashIcon,
        variant: "destructive",
        tone: "destructive",
        label: "系统失败",
    },
    sync: {
        title: "来源同步失败",
        description: "来源数据与现有业务记录已保留，请进入同步任务处理。",
        icon: RefreshCwIcon,
        variant: "destructive",
        tone: "destructive",
        label: "同步失败",
    },
    integration: {
        title: "外部接口失败",
        description: `业务记录已保留，请进入${workspaceLabel("W29")}查看后续处理。`,
        icon: CloudOffIcon,
        variant: "destructive",
        tone: "destructive",
        label: "接口失败",
    },
    projection: {
        title: "数据更新失败",
        description: freshnessText.lastSuccessKept,
        icon: RotateCcwIcon,
        variant: "warning",
        tone: "warning",
        label: "数据可能过期",
    },
}

type BusinessFailureStateProps = {
    kind?: BusinessFailureKind
    /** 原始请求异常；提供时统一覆盖错误分类和说明。 */
    error?: unknown
    title?: React.ReactNode
    description?: React.ReactNode
    errorCode?: string
    nextResponsible?: string
    details?: React.ReactNode
    action?: React.ReactNode
    /** 快捷重试回调；渲染「重试」按钮（与 action 二选一，action 优先）。 */
    onRetry?: () => void
    retryLabel?: string
    className?: string
}

function BusinessFailureState({
    kind,
    error,
    title,
    description,
    errorCode,
    nextResponsible,
    details,
    action,
    onRetry,
    retryLabel = "重试",
    className,
}: BusinessFailureStateProps) {
    const presentation =
        error === undefined ? undefined : getErrorPresentation(error)
    const resolvedKind = presentation?.kind ?? kind ?? "system"
    const preset = failureStatePresets[resolvedKind]
    const Icon = preset.icon
    const resolvedErrorCode = errorCode ?? presentation?.code

    const resolvedAction =
        action ??
        (onRetry && (presentation?.retryable ?? true) ? (
            <Button type="button" variant="outline" size="sm" onClick={onRetry}>
                {retryLabel}
            </Button>
        ) : null)

    return (
        <Alert
            data-slot="business-failure-state"
            data-kind={resolvedKind}
            variant={preset.variant}
            className={className}
        >
            <Icon aria-hidden="true" />
            <AlertTitle className="flex flex-wrap items-center gap-2">
                <span>{title ?? presentation?.title ?? preset.title}</span>
                <StatusBadge tone={preset.tone} label={preset.label} />
            </AlertTitle>
            <AlertDescription>
                <div className="flex flex-col gap-3">
                    <p>
                        {presentation?.description ??
                            description ??
                            preset.description}
                    </p>
                    {resolvedErrorCode ||
                    presentation?.requestId ||
                    nextResponsible ? (
                        <dl className="grid gap-1 text-xs">
                            {resolvedErrorCode ? (
                                <div className="flex flex-wrap gap-1">
                                    <dt className="font-medium">错误编号：</dt>
                                    <dd className="num font-mono">
                                        {resolvedErrorCode}
                                    </dd>
                                </div>
                            ) : null}
                            {presentation?.requestId ? (
                                <div className="flex flex-wrap gap-1">
                                    <dt className="font-medium">请求编号：</dt>
                                    <dd className="num font-mono">
                                        {presentation.requestId}
                                    </dd>
                                </div>
                            ) : null}
                            {nextResponsible ? (
                                <div className="flex flex-wrap gap-1">
                                    <dt className="font-medium">
                                        下一责任人：
                                    </dt>
                                    <dd>{nextResponsible}</dd>
                                </div>
                            ) : null}
                        </dl>
                    ) : null}
                    {details}
                    {resolvedAction ? (
                        <div className="flex flex-wrap gap-2">
                            {resolvedAction}
                        </div>
                    ) : null}
                </div>
            </AlertDescription>
        </Alert>
    )
}

type AsyncSectionStatus = "loading" | "refreshing" | "error" | "success"

type AsyncSectionStateProps = {
    status: AsyncSectionStatus
    children?: React.ReactNode
    loadingLabel?: string
    refreshingLabel?: string
    loadingFallback?: React.ReactNode
    error?: React.ReactNode
    errorKind?: BusinessFailureKind
    retryAction?: React.ReactNode
    className?: string
}

function DefaultSectionSkeleton() {
    return (
        <div className="flex flex-col gap-3" aria-hidden="true">
            <Skeleton className="h-5 w-40" />
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
        </div>
    )
}

/**
 * 区块级异步状态。刷新或失败时只追加反馈，不替换已经存在的旧内容。
 */
function AsyncSectionState({
    status,
    children,
    loadingLabel = "正在加载",
    refreshingLabel = "正在更新，当前仍显示上次成功内容",
    loadingFallback,
    error,
    errorKind = "system",
    retryAction,
    className,
}: AsyncSectionStateProps) {
    const hasChildren = React.Children.count(children) > 0
    const showActivity = status === "loading" || status === "refreshing"

    return (
        <section
            data-slot="async-section-state"
            data-status={status}
            aria-busy={showActivity}
            className={cn("flex min-w-0 flex-col gap-3", className)}
        >
            {showActivity && hasChildren ? (
                <div
                    role="status"
                    className="flex items-center gap-2 rounded-2xl bg-surface-sunken px-3 py-2 text-sm text-muted-foreground"
                >
                    <Spinner
                        aria-label={
                            status === "loading"
                                ? loadingLabel
                                : refreshingLabel
                        }
                    />
                    <span>
                        {status === "loading" ? loadingLabel : refreshingLabel}
                    </span>
                </div>
            ) : null}

            {status === "error" ? (
                <BusinessFailureState
                    kind={errorKind}
                    description={error}
                    action={retryAction}
                />
            ) : null}

            {hasChildren ? children : null}

            {showActivity && !hasChildren ? (
                <div
                    role="status"
                    aria-label={
                        status === "loading" ? loadingLabel : refreshingLabel
                    }
                >
                    {loadingFallback ?? <DefaultSectionSkeleton />}
                </div>
            ) : null}
        </section>
    )
}

export {
    AsyncSectionState,
    BusinessEmptyState,
    BusinessFailureState,
    GuardedBusinessAction,
    type AsyncSectionStateProps,
    type AsyncSectionStatus,
    type AlertVariant,
    type BusinessEmptyStateKind,
    type BusinessEmptyStateProps,
    type BusinessFailureKind,
    type BusinessFailureStateProps,
    type GuardedBusinessActionProps,
}
