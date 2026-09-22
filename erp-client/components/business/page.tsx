"use client"

import * as React from "react"
import type { LucideIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

function MetricDetailTooltip({
    content,
    children,
}: {
    content: React.ReactNode
    children: React.ReactNode
}) {
    return (
        <TooltipProvider>
            <Tooltip>
                <TooltipTrigger
                    render={
                        <span className="inline-flex cursor-help rounded-sm underline decoration-dotted decoration-muted-foreground/60 underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring" />
                    }
                >
                    {children}
                </TooltipTrigger>
                <TooltipContent className="max-w-xs text-xs">
                    {content}
                </TooltipContent>
            </Tooltip>
        </TooltipProvider>
    )
}

type ButtonProps = React.ComponentProps<typeof Button>
type StatusBadgeProps = React.ComponentProps<typeof StatusBadge>

export type SemanticStatus = Pick<StatusBadgeProps, "label" | "tone" | "icon">

export type PageAction = Omit<ButtonProps, "children" | "size"> & {
    actionKey: React.Key
    label: React.ReactNode
    icon?: LucideIcon
    iconPosition?: "start" | "end"
}

type LabeledButtonSize = Extract<
    NonNullable<ButtonProps["size"]>,
    "xs" | "sm" | "default" | "lg"
>

export type PageActionsProps = Omit<React.ComponentProps<"div">, "children"> & {
    actions: readonly PageAction[]
    size?: LabeledButtonSize
    ariaLabel?: string
    id?: string
    idPrefix?: string
}

function PageActions({
    actions,
    size = "sm",
    ariaLabel = "页面操作",
    id,
    idPrefix,
    className,
    ...props
}: PageActionsProps) {
    const baseId = idPrefix ?? id ?? "page-actions"
    return (
        <div
            role="group"
            aria-label={ariaLabel}
            data-slot="page-actions"
            id={baseId}
            className={cn("flex flex-wrap items-center gap-2", className)}
            {...props}
        >
            {actions.map((action) => {
                const {
                    actionKey,
                    label,
                    icon: Icon,
                    iconPosition = "start",
                    className: actionClassName,
                    id: actionId,
                    ...buttonProps
                } = action

                const derivedId =
                    actionId ??
                    `${baseId}-action-${toAutomationIdSegment(String(actionKey))}`

                return (
                    <Button
                        key={actionKey}
                        id={derivedId}
                        {...buttonProps}
                        size={size}
                        className={actionClassName}
                    >
                        {Icon && iconPosition === "start" ? (
                            <Icon data-icon="inline-start" aria-hidden="true" />
                        ) : null}
                        {label}
                        {Icon && iconPosition === "end" ? (
                            <Icon data-icon="inline-end" aria-hidden="true" />
                        ) : null}
                    </Button>
                )
            })}
        </div>
    )
}

/**
 * inline：明细直接占位（列表筛选指标、业务风险信号）。
 * tooltip：明细仅悬停可见（口径/实现旁白，避免拉高首屏）。
 * none：忽略 detail（调用方仍可传，便于开关）。
 */
export type MetricDetailMode = "inline" | "tooltip" | "none"

export type MetricDensity = "default" | "compact"

export type MetricItemProps = Omit<
    React.ComponentProps<"div">,
    "children" | "title"
> & {
    label: React.ReactNode
    value: React.ReactNode
    detail?: React.ReactNode
    status?: SemanticStatus
    /**
     * 明细展示策略。默认 inline；M4 对象中心建议业务风险用 inline，口径说明用 tooltip/none。
     */
    detailMode?: MetricDetailMode
    density?: MetricDensity
}

function MetricItem({
    label,
    value,
    detail,
    status,
    detailMode = "inline",
    density = "compact",
    className,
    ...props
}: MetricItemProps) {
    const compact = density === "compact"
    const showInlineDetail = detail != null && detailMode === "inline"
    const showTooltipDetail = detail != null && detailMode === "tooltip"
    const labelNode = (
        <span
            className={cn("block font-medium text-muted-foreground", "text-xs")}
        >
            {label}
        </span>
    )

    return (
        <div
            data-slot="metric-item"
            data-density={density}
            data-detail-mode={detailMode}
            className={cn("relative min-w-0", className)}
            {...props}
        >
            <div className="flex items-center justify-between gap-2">
                {showTooltipDetail ? (
                    <MetricDetailTooltip content={detail}>
                        {labelNode}
                    </MetricDetailTooltip>
                ) : (
                    labelNode
                )}
                {status ? <StatusBadge {...status} /> : null}
            </div>
            <div
                className={cn(
                    "flex min-w-0 flex-col items-start gap-1",
                    compact ? "mt-1" : "mt-1.5",
                )}
            >
                <div
                    className={cn(
                        "num max-w-full wrap-anywhere font-semibold tracking-tight text-foreground",
                        compact ? "text-xl" : "text-2xl",
                    )}
                >
                    {value}
                </div>
                {showInlineDetail ? (
                    <span className="text-xs leading-relaxed text-muted-foreground">
                        {detail}
                    </span>
                ) : null}
            </div>
        </div>
    )
}

export type MetricStripColumns = 1 | 2 | 3 | 4 | 5 | 6

const metricColumnClasses: Record<MetricStripColumns, string> = {
    1: "grid-cols-1",
    2: "sm:grid-cols-2",
    3: "sm:grid-cols-2 lg:grid-cols-3",
    4: "sm:grid-cols-2 lg:grid-cols-4",
    5: "sm:grid-cols-2 lg:grid-cols-5",
    6: "sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6",
}

export type MetricStripProps = Omit<React.ComponentProps<"div">, "title"> & {
    columns?: MetricStripColumns
    density?: MetricDensity
    /** 顶部统计默认采用浅底摘要；嵌入明细分区时可使用 plain。 */
    variant?: "summary" | "plain"
}

function MetricStrip({
    columns = 4,
    density = "default",
    variant = "summary",
    className,
    ...props
}: MetricStripProps) {
    return (
        <div
            data-slot="metric-strip"
            data-density={density}
            data-variant={variant}
            className={cn(
                "grid gap-x-6 gap-y-4",
                metricColumnClasses[columns],
                variant === "summary" && [
                    "gap-x-4 gap-y-4 rounded-xl bg-muted/35 px-4 py-3 lg:gap-x-6",
                    columns === 1 ? "grid-cols-1" : "grid-cols-2",
                ],
                className,
            )}
            {...props}
        />
    )
}

export type DataFreshnessState =
    | "fresh"
    | "stale"
    | "syncing"
    | "failed"
    | "unknown"

const dataFreshnessState: Record<
    DataFreshnessState,
    { label: string; tone: StatusTone }
> = {
    fresh: { label: "数据已更新", tone: "success" },
    stale: { label: "数据可能过期", tone: "warning" },
    syncing: { label: "正在同步", tone: "info" },
    failed: { label: "同步失败", tone: "destructive" },
    unknown: { label: "更新时间未知", tone: "neutral" },
}

export type DataFreshnessProps = Omit<
    React.ComponentProps<"div">,
    "children"
> & {
    updatedAt: React.ReactNode
    dateTime?: string
    label?: React.ReactNode
    state?: DataFreshnessState
    statusLabel?: string
}

function DataFreshness({
    updatedAt,
    dateTime,
    label = "数据更新于",
    state = "fresh",
    statusLabel,
    className,
    ...props
}: DataFreshnessProps) {
    const status = dataFreshnessState[state]
    const time = dateTime ? (
        <time className="num font-medium text-foreground" dateTime={dateTime}>
            {updatedAt}
        </time>
    ) : (
        <span className="num font-medium text-foreground">{updatedAt}</span>
    )

    return (
        <div
            data-slot="data-freshness"
            className={cn(
                "flex flex-wrap items-center gap-2 text-xs text-muted-foreground",
                className,
            )}
            {...props}
        >
            <span>{label}</span>
            {time}
            <StatusBadge
                tone={status.tone}
                label={statusLabel ?? status.label}
            />
        </div>
    )
}

/**
 * 工作面内容区统一脚手架：max-width、页边距、区块间距。
 * 标题贴画布；主工作面用 1～2 张浮起表面（见 surfacePanelClassName）。
 */
export type PageScaffoldDensity = "default" | "compact"

export type PageScaffoldProps = React.ComponentProps<"div"> & {
    density?: PageScaffoldDensity
}

function PageScaffold({
    density = "default",
    className,
    ...props
}: PageScaffoldProps) {
    return (
        <div
            data-slot="page-scaffold"
            data-density={density}
            className={cn(
                "mx-auto flex w-full max-w-none flex-1 flex-col bg-card",
                density === "compact"
                    ? "gap-4 p-5 md:px-8 md:py-6"
                    : "gap-6 p-5 md:px-8 md:py-6",
                "[&>[data-slot=page-header]]:pb-2",
                "[&:has(>[data-slot=document-header])>[data-slot=page-header]_h1]:text-sm",
                "[&:has(>[data-slot=document-header])>[data-slot=page-header]_h1]:font-normal",
                "[&:has(>[data-slot=document-header])>[data-slot=page-header]_h1]:text-muted-foreground",
                className,
            )}
            {...props}
        />
    )
}

/** 主工作面：白底平面，分区组件负责绘制边界。 */
const surfacePanelClassName = "erp-raised-surface min-w-0 bg-card"

/** 嵌入已有表面时去掉卡片壳，只保留分区线。 */
const surfaceFlatClassName =
    "rounded-none border-0 border-b border-grid bg-transparent shadow-none ring-0 last:border-b-0"

function surfaceClassName(flat?: boolean) {
    return flat ? surfaceFlatClassName : surfacePanelClassName
}

/** 作业面内容左右内边距；分割线画在区块外沿，与容器同宽。 */
const workspaceTaskSurfacePadClassName = "px-5"

/** 工作台内嵌页面脚手架：贴齐作业面，不再套一层页边距和卡片间隙。 */
const workspaceEmbeddedScaffoldClassName = cn(
    "h-full min-h-0 max-w-none gap-0 p-0 md:p-0",
    "[&>[data-slot=page-header]]:static",
    "[&>[data-slot=page-header]]:mx-0",
    "[&>[data-slot=page-header]]:mt-0",
    "[&>[data-slot=page-header]]:mb-0",
    "[&>[data-slot=page-header]]:bg-transparent",
    "[&>[data-slot=page-header]]:px-5",
    "[&>[data-slot=page-header]]:pt-5",
    "[&>[data-slot=page-header]]:pb-5",
)

/**
 * 工作台作业面：把嵌套浮起表面压成扁平分区。
 * 审批详情已经按分区线铺开；嵌入的领域页不得再套一层卡片。
 * 区块自己带左右内边距，分割线与容器同宽。
 */
const workspaceTaskSurfaceClassName = cn(
    "[&_.erp-raised-surface]:rounded-none",
    "[&_.erp-raised-surface]:border-x-0",
    "[&_.erp-raised-surface]:border-t-0",
    "[&_.erp-raised-surface]:shadow-none",
    "[&_.erp-raised-surface]:bg-transparent",
    "[&_.erp-raised-surface]:ring-0",
    "[&_[data-slot=card]]:rounded-none",
    "[&_[data-slot=card]]:border-x-0",
    "[&_[data-slot=card]]:border-t-0",
    "[&_[data-slot=card]]:border-b",
    "[&_[data-slot=card]]:border-grid",
    "[&_[data-slot=card]]:bg-transparent",
    "[&_[data-slot=card]]:shadow-none",
    "[&_[data-slot=card]]:px-5",
    "[&_[data-slot=card]]:py-5",
    "[&_[data-slot=card]]:gap-3",
    "[&_[data-slot=card]]:[--card-spacing:0px]",
    "[&_[data-slot=card-header]]:rounded-none",
    "[&_[data-slot=card-header]]:-mx-5",
    "[&_[data-slot=card-header]]:px-5",
    "[&_[data-slot=card-footer]]:rounded-none",
    "[&_[data-slot=card-footer]]:-mx-5",
    "[&_[data-slot=card-footer]]:px-5",
    "[&_[data-slot=sequential-process-bar]]:rounded-none",
    "[&_[data-slot=sequential-process-bar]]:border-x-0",
    "[&_[data-slot=sequential-process-bar]]:border-t-0",
    "[&_[data-slot=sequential-process-bar]]:bg-transparent",
    "[&_[data-slot=sequential-process-bar]]:px-5",
    "[&_[data-slot=sequential-process-bar]]:shadow-none",
    "[&_[data-slot=document-header]]:rounded-none",
    "[&_[data-slot=document-header]]:border-x-0",
    "[&_[data-slot=document-header]]:border-t-0",
    "[&_[data-slot=document-header]]:shadow-none",
    "[&_[data-slot=document-header]]:bg-transparent",
    "[&_[data-slot=document-header]]:px-5",
    "[&_[data-slot=page-header]]:px-5",
    "[&_[data-slot=document-section]]:px-5",
)

/** 主卡内轻提示/工具条：无描边，仅浅底区分。 */
const surfaceInsetClassName = "rounded-md bg-muted/40"

export {
    DataFreshness,
    MetricItem,
    MetricStrip,
    PageActions,
    PageScaffold,
    surfaceClassName,
    surfaceFlatClassName,
    surfaceInsetClassName,
    surfacePanelClassName,
    workspaceEmbeddedScaffoldClassName,
    workspaceTaskSurfaceClassName,
    workspaceTaskSurfacePadClassName,
}
