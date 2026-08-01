"use client"

import * as React from "react"
import type { LucideIcon } from "lucide-react"

import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb"
import { Button } from "@/components/ui/button"
import {
  StatusBadge,
  type StatusTone,
} from "@/components/ui/status-badge"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
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
        <TooltipContent className="max-w-xs text-xs">{content}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}

type ButtonProps = React.ComponentProps<typeof Button>
type StatusBadgeProps = React.ComponentProps<typeof StatusBadge>

export type SemanticStatus = Pick<
  StatusBadgeProps,
  "label" | "tone" | "icon"
>

export type PageBreadcrumbItem =
  | {
      id: string
      label: React.ReactNode
      current: true
      href?: never
    }
  | {
      id: string
      label: React.ReactNode
      current?: false
      href: string
    }

export type PageHeaderDensity = "default" | "compact"

/**
 * page：工作台/列表/治理等「页面级」标题头。
 * object-chrome：M4 对象中心壳层，只保留面包屑与轻动作；对象身份交给 DocumentHeader，
 * 禁止再叠一层与 DocumentHeader 重复的大标题。
 */
export type PageHeaderVariant = "page" | "object-chrome"

export type PageHeaderProps = Omit<
  React.ComponentProps<"header">,
  "children" | "title"
> & {
  /**
   * page 变体必填；object-chrome 时可选（通常省略，避免与 DocumentHeader 双标题）。
   */
  title?: React.ReactNode
  description?: React.ReactNode
  breadcrumbs?: readonly PageBreadcrumbItem[]
  breadcrumbLabel?: string
  status?: SemanticStatus
  metadata?: React.ReactNode
  actions?: React.ReactNode
  /**
   * compact（默认）：标题、状态与 metadata 同排，面包屑压到 xs，供高频作业页压缩首屏。
   * default：标题 text-2xl、metadata 独占一行，用于需要展示语气的落地页。
   * object-chrome 变体始终按 compact 节奏渲染，本 prop 仅影响 page 变体。
   */
  density?: PageHeaderDensity
  /**
   * page（默认）：列表/工作台页头。
   * object-chrome：对象中心导航条（面包屑 + 可选 metadata/返回），不渲染 h1 工作面名。
   */
  variant?: PageHeaderVariant
}

function PageHeader({
  title,
  description,
  breadcrumbs = [],
  breadcrumbLabel = "面包屑导航",
  status,
  metadata,
  actions,
  density = "compact",
  variant = "page",
  className,
  ...props
}: PageHeaderProps) {
  const objectChrome = variant === "object-chrome"
  const compact = objectChrome || density === "compact"
  const showTitleBlock =
    !objectChrome &&
    (title != null ||
      status != null ||
      description != null ||
      metadata != null)

  return (
    <header
      data-slot="page-header"
      data-density={objectChrome ? "compact" : density}
      data-variant={variant}
      className={cn(
        "flex flex-col",
        objectChrome ? "gap-1.5" : compact ? "gap-2" : "gap-3",
        className
      )}
      {...props}
    >
      {breadcrumbs.length > 0 || (objectChrome && (metadata || actions)) ? (
        <div
          className={cn(
            "flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between",
            objectChrome && "min-h-0"
          )}
        >
          <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
            {breadcrumbs.length > 0 ? (
              <Breadcrumb
                aria-label={breadcrumbLabel}
                className={compact ? "text-xs" : undefined}
              >
                <BreadcrumbList
                  className={compact ? "gap-1 sm:gap-1.5" : undefined}
                >
                  {breadcrumbs.map((item, index) => (
                    <React.Fragment key={item.id}>
                      {index > 0 ? <BreadcrumbSeparator /> : null}
                      <BreadcrumbItem>
                        {item.current ? (
                          <BreadcrumbPage>{item.label}</BreadcrumbPage>
                        ) : (
                          <BreadcrumbLink href={item.href}>
                            {item.label}
                          </BreadcrumbLink>
                        )}
                      </BreadcrumbItem>
                    </React.Fragment>
                  ))}
                </BreadcrumbList>
              </Breadcrumb>
            ) : null}
            {objectChrome && metadata ? (
              <div className="min-w-0 text-xs text-muted-foreground">
                {metadata}
              </div>
            ) : null}
          </div>
          {objectChrome && actions ? (
            <div className="shrink-0 sm:ml-auto">{actions}</div>
          ) : null}
        </div>
      ) : null}

      {showTitleBlock ? (
        <div
          className={cn(
            "flex flex-col gap-3 lg:flex-row",
            compact ? "lg:items-center" : "lg:items-start"
          )}
        >
          <div className="min-w-0 flex-1">
            <div
              className={cn(
                "flex flex-wrap items-center",
                compact ? "gap-x-3 gap-y-1" : "gap-2"
              )}
            >
              {title != null ? (
                <h1
                  className={cn(
                    "font-semibold tracking-tight text-foreground",
                    compact ? "text-lg" : "text-2xl"
                  )}
                >
                  {title}
                </h1>
              ) : null}
              {status ? <StatusBadge {...status} /> : null}
              {compact && metadata ? (
                <div className="min-w-0 text-sm text-muted-foreground">
                  {metadata}
                </div>
              ) : null}
            </div>
            {description ? (
              <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
                {description}
              </p>
            ) : null}
            {!compact && metadata ? (
              <div className="mt-2 text-sm text-muted-foreground">
                {metadata}
              </div>
            ) : null}
          </div>
          {actions ? (
            <div className="shrink-0 lg:ml-auto">{actions}</div>
          ) : null}
        </div>
      ) : !objectChrome && actions ? (
        <div className="flex justify-end">{actions}</div>
      ) : null}
    </header>
  )
}

export type PageAction = Omit<ButtonProps, "children" | "size"> & {
  actionKey: React.Key
  label: React.ReactNode
  icon?: LucideIcon
  iconPosition?: "start" | "end"
  /** 手机端只保留安全的查看/刷新入口，写操作和全量导出应隐藏。 */
  mobileVisibility?: "show" | "hide"
}

type LabeledButtonSize = Extract<
  NonNullable<ButtonProps["size"]>,
  "xs" | "sm" | "default" | "lg"
>

export type PageActionsProps = Omit<
  React.ComponentProps<"div">,
  "children"
> & {
  actions: readonly PageAction[]
  size?: LabeledButtonSize
  ariaLabel?: string
}

function PageActions({
  actions,
  size = "sm",
  ariaLabel = "页面操作",
  className,
  ...props
}: PageActionsProps) {
  return (
    <div
      role="group"
      aria-label={ariaLabel}
      data-slot="page-actions"
      className={cn("flex flex-wrap items-center gap-2", className)}
      {...props}
    >
      {actions.map((action) => {
        const {
          actionKey,
          label,
          icon: Icon,
          iconPosition = "start",
          mobileVisibility = "show",
          className: actionClassName,
          ...buttonProps
        } = action

        return (
          <Button
            key={actionKey}
            {...buttonProps}
            size={size}
            className={cn(
              mobileVisibility === "hide" && "max-sm:hidden",
              actionClassName
            )}
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
  density = "default",
  className,
  ...props
}: MetricItemProps) {
  const compact = density === "compact"
  const showInlineDetail = detail != null && detailMode === "inline"
  const showTooltipDetail = detail != null && detailMode === "tooltip"
  const labelNode = (
    <span
      className={cn(
        "block text-muted-foreground",
        compact ? "text-xs" : "text-sm"
      )}
    >
      {label}
    </span>
  )

  return (
    <div
      data-slot="metric-item"
      data-density={density}
      data-detail-mode={detailMode}
      className={cn(
        "min-w-0 bg-card",
        compact ? "p-2 sm:p-2.5" : "p-2.5 sm:p-3",
        className
      )}
      {...props}
    >
      {showTooltipDetail ? (
        <MetricDetailTooltip content={detail}>{labelNode}</MetricDetailTooltip>
      ) : (
        labelNode
      )}
      <div className={compact ? "mt-0.5" : "mt-1"}>
        <div
          className={cn(
            "num font-semibold tracking-tight text-foreground",
            compact ? "text-lg" : "text-xl"
          )}
        >
          {value}
        </div>
        {status || showInlineDetail ? (
          <div
            className={cn(
              "flex flex-wrap items-center gap-2",
              compact ? "mt-1" : "mt-1.5"
            )}
          >
            {status ? <StatusBadge {...status} /> : null}
            {showInlineDetail ? (
              <span className="text-xs text-muted-foreground">{detail}</span>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  )
}

export type MetricStripColumns = 1 | 2 | 3 | 4 | 5 | 6

export type MetricFilterItemProps = Omit<
  React.ComponentProps<"button">,
  "children" | "value"
> & {
  label: React.ReactNode
  value: React.ReactNode
  detail?: React.ReactNode
  active?: boolean
  detailMode?: MetricDetailMode
  density?: MetricDensity
}

/** 可作为列表/待办过滤器的指标项，提供按钮语义与明确选中态。 */
function MetricFilterItem({
  label,
  value,
  detail,
  active = false,
  detailMode = "inline",
  density = "default",
  className,
  ...props
}: MetricFilterItemProps) {
  const compact = density === "compact"
  const showInlineDetail = detail != null && detailMode === "inline"
  return (
    <div className="min-w-0 bg-card">
      <button
        type="button"
        aria-pressed={active}
        data-density={density}
        className={cn(
          "h-full w-full border-l-2 border-transparent text-left transition-colors hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset xl:flex xl:items-baseline xl:gap-2",
          compact ? "p-2 sm:p-2.5" : "p-2.5 sm:p-3",
          active && "border-l-primary bg-accent text-accent-foreground",
          className
        )}
        {...props}
      >
        <span
          className={cn(
            "block text-muted-foreground xl:shrink-0",
            compact ? "text-xs" : "text-sm"
          )}
        >
          {label}
        </span>
        <span
          className={cn(
            "num mt-1 block font-semibold tracking-tight text-foreground xl:mt-0",
            compact ? "text-lg" : "text-xl"
          )}
        >
          {value}
        </span>
        {showInlineDetail ? (
          <span className="mt-1 block text-xs text-muted-foreground xl:ml-auto xl:mt-0 xl:truncate">
            {detail}
          </span>
        ) : null}
      </button>
    </div>
  )
}

const metricColumnClasses: Record<MetricStripColumns, string> = {
  1: "grid-cols-1",
  2: "sm:grid-cols-2",
  3: "sm:grid-cols-2 lg:grid-cols-3",
  4: "sm:grid-cols-2 lg:grid-cols-4",
  5: "sm:grid-cols-2 lg:grid-cols-5",
  6: "sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6",
}

export type MetricStripProps = Omit<
  React.ComponentProps<"div">,
  "title"
> & {
  columns?: MetricStripColumns
  density?: MetricDensity
}

function MetricStrip({
  columns = 4,
  density = "default",
  className,
  ...props
}: MetricStripProps) {
  return (
    <div
      data-slot="metric-strip"
      data-density={density}
      className={cn(
        "grid gap-px overflow-hidden rounded-lg border border-grid bg-grid",
        metricColumnClasses[columns],
        className
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
        className
      )}
      {...props}
    >
      <span>{label}</span>
      {time}
      <StatusBadge tone={status.tone} label={statusLabel ?? status.label} />
    </div>
  )
}

export {
  DataFreshness,
  MetricItem,
  MetricFilterItem,
  MetricStrip,
  PageActions,
  PageHeader,
}
