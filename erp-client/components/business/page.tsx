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
import { cn } from "@/lib/utils"

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

export type PageHeaderProps = Omit<
  React.ComponentProps<"header">,
  "children" | "title"
> & {
  title: React.ReactNode
  description?: React.ReactNode
  breadcrumbs?: readonly PageBreadcrumbItem[]
  breadcrumbLabel?: string
  status?: SemanticStatus
  metadata?: React.ReactNode
  actions?: React.ReactNode
  /** compact：标题、状态与 metadata 同排，供高频作业页压缩首屏。 */
  density?: PageHeaderDensity
}

function PageHeader({
  title,
  description,
  breadcrumbs = [],
  breadcrumbLabel = "面包屑导航",
  status,
  metadata,
  actions,
  density = "default",
  className,
  ...props
}: PageHeaderProps) {
  const compact = density === "compact"

  return (
    <header
      data-slot="page-header"
      data-density={density}
      className={cn("flex flex-col", compact ? "gap-2" : "gap-3", className)}
      {...props}
    >
      {breadcrumbs.length > 0 ? (
        <Breadcrumb aria-label={breadcrumbLabel}>
          <BreadcrumbList>
            {breadcrumbs.map((item, index) => (
              <React.Fragment key={item.id}>
                {index > 0 ? <BreadcrumbSeparator /> : null}
                <BreadcrumbItem>
                  {item.current ? (
                    <BreadcrumbPage>{item.label}</BreadcrumbPage>
                  ) : (
                    <BreadcrumbLink href={item.href}>{item.label}</BreadcrumbLink>
                  )}
                </BreadcrumbItem>
              </React.Fragment>
            ))}
          </BreadcrumbList>
        </Breadcrumb>
      ) : null}

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
            <h1
              className={cn(
                "font-semibold tracking-tight text-foreground",
                compact ? "text-lg" : "text-2xl"
              )}
            >
              {title}
            </h1>
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
            <div className="mt-2 text-sm text-muted-foreground">{metadata}</div>
          ) : null}
        </div>
        {actions ? <div className="shrink-0 lg:ml-auto">{actions}</div> : null}
      </div>
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

export type MetricItemProps = Omit<
  React.ComponentProps<"div">,
  "children" | "title"
> & {
  label: React.ReactNode
  value: React.ReactNode
  detail?: React.ReactNode
  status?: SemanticStatus
}

function MetricItem({
  label,
  value,
  detail,
  status,
  className,
  ...props
}: MetricItemProps) {
  return (
    <div
      data-slot="metric-item"
      className={cn("min-w-0 bg-card p-2.5 sm:p-3", className)}
      {...props}
    >
      <span className="block text-sm text-muted-foreground">{label}</span>
      <div className="mt-1">
        <div className="num text-xl font-semibold tracking-tight text-foreground">
          {value}
        </div>
        {detail || status ? (
          <div className="mt-1.5 flex flex-wrap items-center gap-2">
            {status ? <StatusBadge {...status} /> : null}
            {detail ? (
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
}

/** 可作为列表/待办过滤器的指标项，提供按钮语义与明确选中态。 */
function MetricFilterItem({
  label,
  value,
  detail,
  active = false,
  className,
  ...props
}: MetricFilterItemProps) {
  return (
    <div className="min-w-0 bg-card">
      <button
        type="button"
        aria-pressed={active}
        className={cn(
          "h-full w-full border-l-2 border-transparent p-2.5 text-left transition-colors hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset sm:p-3 xl:flex xl:items-baseline xl:gap-2",
          active && "border-l-primary bg-accent text-accent-foreground",
          className
        )}
        {...props}
      >
        <span className="block text-sm text-muted-foreground xl:shrink-0">{label}</span>
        <span className="num mt-1 block text-xl font-semibold tracking-tight text-foreground xl:mt-0">
          {value}
        </span>
        {detail ? (
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
}

function MetricStrip({
  columns = 4,
  className,
  ...props
}: MetricStripProps) {
  return (
    <div
      data-slot="metric-strip"
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
