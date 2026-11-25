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

import type { Alert } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
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
import { fieldErrorMessages } from "@/lib/api/field-errors"
import { getErrorPresentation, isApiError } from "@/lib/api/errors"
import { readProblemCopy, type ReadProblemContext } from "@/lib/system-feedback"
import { freshnessText, versionText, workspaceLabel } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

type GuardedBusinessActionProps = Omit<
    React.ComponentProps<typeof LoadingButton>,
    "disabled"
> & {
    disabled?: boolean
    reason?: string
    nextResponsible?: string
    shortcut?: string
    id?: string
    idPrefix?: string
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
    id,
    idPrefix,
    ...buttonProps
}: GuardedBusinessActionProps) {
    const baseId = idPrefix ?? id
    const hasExplanation = Boolean(
        disabled || reason || nextResponsible || shortcut,
    )
    const explanation =
        reason ?? (disabled ? "当前状态不允许执行此操作" : undefined)

    if (!hasExplanation) {
        return (
            <LoadingButton id={baseId} {...buttonProps}>
                {children}
            </LoadingButton>
        )
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
                            <LoadingButton
                                id={baseId}
                                {...buttonProps}
                                type="button"
                                aria-disabled="true"
                                data-disabled="true"
                                className={cn(
                                    "cursor-not-allowed opacity-50",
                                    buttonProps.className,
                                )}
                                onClick={(event) => {
                                    event.preventDefault()
                                }}
                                onKeyDown={(event) => {
                                    if (
                                        event.key === "Enter" ||
                                        event.key === " "
                                    ) {
                                        event.preventDefault()
                                    }
                                }}
                            />
                        }
                    >
                        {children}
                    </TooltipTrigger>
                ) : (
                    <TooltipTrigger
                        render={<LoadingButton id={baseId} {...buttonProps} />}
                    >
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
        title: "暂无可查看的数据",
        description:
            "您当前尚未被分配该模块的查看范围，如需查看请联系管理员开通。",
        icon: ShieldXIcon,
    },
    "not-synced": {
        title: "尚未完成同步",
        description: "暂未完成首次数据同步，可前往批量任务记录查看进度。",
        icon: CloudOffIcon,
    },
    "no-tasks": {
        title: "当前没有待处理事项",
        description: "当前工作范围内没有需要你处理的任务。",
        icon: ClipboardCheckIcon,
    },
    "no-exceptions": {
        title: "当前没有未解决异常",
        description: "当前一切正常，暂无待处理的异常事项。",
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
                // 空状态沿用所在工作面的背景，保留标题、说明与下一步操作。
                "rounded-none border-0 bg-transparent ring-0",
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

const failureSurfaceClasses: Record<
    NonNullable<AlertVariant>,
    { container: string; icon: string }
> = {
    default: {
        container: "border-border bg-card text-foreground",
        icon: "text-muted-foreground",
    },
    destructive: {
        container:
            "border-destructive-border bg-destructive-soft/75 text-foreground",
        icon: "text-destructive-soft-foreground",
    },
    warning: {
        container: "border-warning-border bg-warning-soft/75 text-foreground",
        icon: "text-warning-soft-foreground",
    },
    info: {
        container: "border-info-border bg-info-soft/75 text-foreground",
        icon: "text-info-soft-foreground",
    },
    success: {
        container: "border-success-border bg-success-soft/75 text-foreground",
        icon: "text-success-soft-foreground",
    },
}

const failureStatePresets: Record<BusinessFailureKind, FailureStatePreset> = {
    validation: {
        title: "填写内容有误",
        description: "请按提示修正输入后再次提交。",
        icon: FileWarningIcon,
        variant: "warning",
        tone: "warning",
        label: "校验未通过",
    },
    business: {
        title: "暂无法执行此操作",
        description: "当前单据状态或业务条件不满足要求，请核对后再试。",
        icon: TriangleAlertIcon,
        variant: "warning",
        tone: "warning",
        label: "暂不可操作",
    },
    permission: {
        title: "权限不足",
        description: "当前账号暂无此项操作或数据的查看权限，请联系管理员开通。",
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
        title: "外部平台通信失败",
        description: `业务记录已保留，请进入${workspaceLabel("W29")}查看后续处理。`,
        icon: CloudOffIcon,
        variant: "destructive",
        tone: "destructive",
        label: "平台通信异常",
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
    readContext?: ReadProblemContext
    errorCode?: string
    nextResponsible?: string
    details?: React.ReactNode
    /** 技术详情默认折叠；需要直接平铺展示时可显式传入 false。 */
    collapsibleDetails?: boolean
    action?: React.ReactNode
    /** 快捷重试回调；渲染「重试」按钮（与 action 二选一，action 优先）。 */
    onRetry?: () => void
    retryLabel?: string
    id?: string
    idPrefix?: string
    className?: string
}

function BusinessFailureState({
    kind,
    error,
    title,
    description,
    readContext,
    errorCode,
    nextResponsible,
    details,
    collapsibleDetails = true,
    action,
    onRetry,
    retryLabel = "重试",
    id,
    idPrefix,
    className,
}: BusinessFailureStateProps) {
    const presentation =
        error === undefined ? undefined : getErrorPresentation(error)
    const readCopy =
        presentation && readContext
            ? readProblemCopy(presentation, readContext)
            : undefined
    const resolvedKind = presentation?.kind ?? kind ?? "system"
    const preset = failureStatePresets[resolvedKind]
    const PresetIcon = preset.icon
    const surface = failureSurfaceClasses[preset.variant ?? "default"]
    const resolvedErrorCode = errorCode ?? presentation?.code
    const technicalId = idPrefix
        ? `${idPrefix}-technical`
        : id
          ? `${id}-technical`
          : undefined
    const hasTechnicalDetails = Boolean(
        resolvedErrorCode || presentation?.requestId || nextResponsible,
    )
    const technicalDetailsList = hasTechnicalDetails ? (
        <dl className="mt-2 grid gap-1 text-xs">
            {resolvedErrorCode ? (
                <div className="flex flex-wrap gap-1">
                    <dt className="font-medium">错误编号：</dt>
                    <dd className="num font-mono">{resolvedErrorCode}</dd>
                </div>
            ) : null}
            {presentation?.requestId ? (
                <div className="flex flex-wrap gap-1">
                    <dt className="font-medium">请求编号：</dt>
                    <dd className="num font-mono">{presentation.requestId}</dd>
                </div>
            ) : null}
            {nextResponsible ? (
                <div className="flex flex-wrap gap-1">
                    <dt className="font-medium">下一责任人：</dt>
                    <dd>{nextResponsible}</dd>
                </div>
            ) : null}
        </dl>
    ) : null

    const retryId = idPrefix
        ? `${idPrefix}-retry`
        : id
          ? `${id}-retry`
          : undefined
    const resolvedAction =
        action ??
        (onRetry && (presentation?.retryable ?? true) ? (
            <Button
                id={retryId}
                type="button"
                variant="outline"
                size="sm"
                onClick={onRetry}
            >
                {retryLabel}
            </Button>
        ) : null)

    return (
        <section
            role={preset.variant === "destructive" ? "alert" : "status"}
            data-slot="business-failure-state"
            data-kind={resolvedKind}
            data-variant={preset.variant}
            className={cn(
                "space-y-2 rounded-lg border p-4 text-body-compact",
                surface.container,
                className,
            )}
        >
            <div className="flex flex-wrap items-center gap-2 font-medium">
                <PresetIcon
                    className={cn("size-4 shrink-0", surface.icon)}
                    aria-hidden="true"
                />
                <span>
                    {title ??
                        readCopy?.title ??
                        presentation?.title ??
                        preset.title}
                </span>
                <StatusBadge tone={preset.tone} label={preset.label} />
            </div>
            <div className="text-muted-foreground">
                <div className="flex flex-col gap-3">
                    <p>
                        {readCopy?.description ??
                            (presentation?.kind === "permission"
                                ? "当前账号无权查看此内容。"
                                : presentation?.description) ??
                            description ??
                            preset.description}
                    </p>
                    {technicalDetailsList ? (
                        collapsibleDetails ? (
                            <details>
                                <summary
                                    id={technicalId}
                                    className="cursor-pointer text-xs text-muted-foreground hover:text-foreground"
                                >
                                    技术详情
                                </summary>
                                {technicalDetailsList}
                            </details>
                        ) : (
                            <div>
                                <p className="text-xs font-medium text-muted-foreground">
                                    技术详情
                                </p>
                                {technicalDetailsList}
                            </div>
                        )
                    ) : null}
                    {presentation?.fieldErrors ? (
                        <ul className="list-disc space-y-1 pl-5">
                            {fieldErrorMessages(presentation.fieldErrors).map(
                                (message, index) => (
                                    <li key={`${index}-${message}`}>
                                        {message}
                                    </li>
                                ),
                            )}
                        </ul>
                    ) : null}
                    {details}
                    {resolvedAction ? (
                        <div className="flex flex-wrap gap-2">
                            {resolvedAction}
                        </div>
                    ) : null}
                </div>
            </div>
        </section>
    )
}

type AsyncSectionStatus = "loading" | "refreshing" | "error" | "success"

type AsyncSectionStateProps = {
    status: AsyncSectionStatus
    children?: React.ReactNode
    loadingLabel?: string
    refreshingLabel?: string
    loadingFallback?: React.ReactNode
    error?: unknown
    errorKind?: BusinessFailureKind
    retryAction?: React.ReactNode
    id?: string
    idPrefix?: string
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
    refreshingLabel = "正在刷新最新数据…",
    loadingFallback,
    error,
    errorKind = "system",
    retryAction,
    id,
    idPrefix,
    className,
}: AsyncSectionStateProps) {
    const hasChildren = React.Children.count(children) > 0
    const showActivity = status === "loading" || status === "refreshing"
    const isStructuredError = error instanceof Error || isApiError(error)

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
                    className="flex items-center gap-2 rounded-lg bg-surface-sunken px-3 py-2 text-body-compact text-muted-foreground"
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
                    error={isStructuredError ? error : undefined}
                    description={
                        isStructuredError
                            ? undefined
                            : (error as React.ReactNode)
                    }
                    action={retryAction}
                    id={id}
                    idPrefix={idPrefix}
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
