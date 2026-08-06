"use client"

import * as React from "react"
import {
  CircleCheckIcon,
  CircleDashedIcon,
  CircleXIcon,
  ClipboardCheckIcon,
  CloudOffIcon,
  DatabaseIcon,
  EyeIcon,
  EyeOffIcon,
  FileWarningIcon,
  GitCompareArrowsIcon,
  LoaderCircleIcon,
  LockKeyholeIcon,
  RefreshCwIcon,
  RotateCcwIcon,
  SaveIcon,
  SearchXIcon,
  ServerCrashIcon,
  ShieldAlertIcon,
  ShieldXIcon,
  TriangleAlertIcon,
  type LucideIcon,
} from "lucide-react"

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
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
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Kbd } from "@/components/ui/kbd"
import {
  Progress,
  ProgressLabel,
  ProgressValue,
} from "@/components/ui/progress"
import { Skeleton } from "@/components/ui/skeleton"
import { Spinner } from "@/components/ui/spinner"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import {
  freshnessText,
  resultText,
  versionText,
  workspaceLabel,
} from "@/lib/ui-text"
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
    disabled || reason || nextResponsible || shortcut
  )
  const explanation = reason ?? (disabled ? "当前状态不允许执行此操作" : undefined)

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
                role="group"
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
        className
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
    title: "查询模型更新失败",
    description: freshnessText.lastSuccessKept,
    icon: RotateCcwIcon,
    variant: "warning",
    tone: "warning",
    label: "数据可能过期",
  },
}

type BusinessFailureStateProps = {
  kind: BusinessFailureKind
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
  const preset = failureStatePresets[kind]
  const Icon = preset.icon

  const resolvedAction =
    action ??
    (onRetry ? (
      <Button type="button" variant="outline" size="sm" onClick={onRetry}>
        {retryLabel}
      </Button>
    ) : null)

  return (
    <Alert
      data-slot="business-failure-state"
      data-kind={kind}
      variant={preset.variant}
      className={className}
    >
      <Icon aria-hidden="true" />
      <AlertTitle className="flex flex-wrap items-center gap-2">
        <span>{title ?? preset.title}</span>
        <StatusBadge tone={preset.tone} label={preset.label} />
      </AlertTitle>
      <AlertDescription>
        <div className="flex flex-col gap-3">
          <p>{description ?? preset.description}</p>
          {errorCode || nextResponsible ? (
            <dl className="grid gap-1 text-xs">
              {errorCode ? (
                <div className="flex flex-wrap gap-1">
                  <dt className="font-medium">错误编号：</dt>
                  <dd className="num font-mono">{errorCode}</dd>
                </div>
              ) : null}
              {nextResponsible ? (
                <div className="flex flex-wrap gap-1">
                  <dt className="font-medium">下一责任人：</dt>
                  <dd>{nextResponsible}</dd>
                </div>
              ) : null}
            </dl>
          ) : null}
          {details}
          {resolvedAction ? (
            <div className="flex flex-wrap gap-2">{resolvedAction}</div>
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
          <Spinner aria-label={status === "loading" ? loadingLabel : refreshingLabel} />
          <span>{status === "loading" ? loadingLabel : refreshingLabel}</span>
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
          aria-label={status === "loading" ? loadingLabel : refreshingLabel}
        >
          {loadingFallback ?? <DefaultSectionSkeleton />}
        </div>
      ) : null}
    </section>
  )
}

type DraftSaveState = "idle" | "dirty" | "saving" | "saved" | "failed"

type DraftSaveIndicatorProps = {
  state: DraftSaveState
  savedAt?: Date | string
  message?: string
  onRetry?: () => void
  className?: string
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
}: DraftSaveIndicatorProps) {
  const presentation = draftStatePresentation[state]
  const savedSuffix = state === "saved" && savedAt ? ` ${formatDraftTime(savedAt)}` : ""

  return (
    <div
      data-slot="draft-save-indicator"
      data-state={state}
      role="status"
      aria-live="polite"
      className={cn("flex flex-wrap items-center gap-2 text-sm", className)}
    >
      <StatusBadge
        tone={presentation.tone}
        icon={presentation.icon}
        label={`${presentation.label}${savedSuffix}`}
      />
      {message ? <span className="text-muted-foreground">{message}</span> : null}
      {state === "failed" && onRetry ? (
        <Button type="button" variant="link" size="xs" onClick={onRetry}>
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
  onLocate?: (issue: ValidationIssue) => void
  className?: string
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
  onLocate,
  className,
}: ValidationSummaryProps) {
  if (issues.length === 0) return null

  return (
    <Alert
      data-slot="validation-summary"
      variant="warning"
      className={className}
    >
      <TriangleAlertIcon aria-hidden="true" />
      <AlertTitle>{title ?? `发现 ${issues.length} 项待处理`}</AlertTitle>
      <AlertDescription>
        <ol className="flex list-decimal flex-col gap-1 pl-5">
          {issues.map((issue) => (
            <li key={issue.id}>
              <Button
                type="button"
                variant="link"
                size="xs"
                aria-controls={issue.targetId}
                className="h-auto justify-start p-0 text-left whitespace-normal"
                onClick={() => {
                  if (onLocate) {
                    onLocate(issue)
                    return
                  }

                  locateValidationIssue(issue)
                }}
              >
                <span className="font-medium">{issue.label}：</span>
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
 * 页面通用「表单动作结果」状态，用 optional 吸收各页面历史变体：
 * - facts：部分页面 value 为 string，部分为 ReactNode
 * - outcome：随各业务域类型参数化（默认 unknown）
 * - 扩展字段（pendingIdempotencyKey / jobId / w12Href / returnTo 等）均为可选
 */
type ResultState<TOutcome = unknown> = {
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
  facts?: Array<{ label: string; value: string | React.ReactNode }>
  pendingIdempotencyKey?: string
  pendingRequestId?: string
  pendingKey?: string
  outcome?: TOutcome
  stayOnItem?: boolean
  terminal?: boolean
  jobId?: string
  jobNo?: string
  payableNo?: string
  w12Href?: string
  returnTo?: string
  w29Href?: string
  stayUnknown?: boolean
} | null

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
  variant: AlertVariant
  tone: StatusTone
}

const formalResultPresets: Record<
  FormalActionResultStatus,
  FormalResultPreset
> = {
  succeeded: {
    title: resultText.operationSucceeded,
    description: "结果已经形成，并可在关联单据与审计记录中追溯。",
    label: "已完成",
    icon: CircleCheckIcon,
    variant: "success",
    tone: "success",
  },
  rejected: {
    title: resultText.operationRejected,
    description: "本次操作未形成目标结果，请根据原因继续处理。",
    label: "未通过",
    icon: CircleXIcon,
    variant: "destructive",
    tone: "destructive",
  },
  blocked: {
    title: resultText.operationBlocked,
    description: "当前前置条件尚未满足，本次操作未形成处理结果。",
    label: "已阻断",
    icon: TriangleAlertIcon,
    variant: "warning",
    tone: "warning",
  },
  processing: {
    title: resultText.operationProcessing,
    description: "结果尚未确定，可安全离开并在后台任务中继续查看。",
    label: "处理中",
    icon: LoaderCircleIcon,
    variant: "info",
    tone: "info",
  },
  unknown: {
    title: resultText.unknown,
    description: "不得按成功处理，请等待核对或进入异常处理流程。",
    label: "结果未知",
    icon: TriangleAlertIcon,
    variant: "warning",
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
  const role = status === "rejected" || status === "blocked" ? "alert" : "status"

  return (
    <Alert
      data-slot="formal-action-result"
      data-status={status}
      variant={preset.variant}
      role={role}
      aria-live="polite"
      tabIndex={-1}
      className={className}
    >
      <Icon aria-hidden="true" />
      <AlertTitle className="flex flex-wrap items-center gap-2">
        <span>{title ?? preset.title}</span>
        <StatusBadge tone={preset.tone} label={preset.label} />
      </AlertTitle>
      <AlertDescription>
        <div className="flex flex-col gap-3">
          <p>{description ?? preset.description}</p>
          {reference ? (
            <p className="text-xs">
              {referenceLabel}：
              <span className="num font-mono">{reference}</span>
            </p>
          ) : null}
          {facts?.length ? (
            <dl className="grid gap-2 sm:grid-cols-2">
              {facts.map((fact) => (
                <div key={fact.label} className="flex flex-col gap-0.5">
                  <dt className="text-xs font-medium">{fact.label}</dt>
                  <dd className="text-foreground">{fact.value}</dd>
                </div>
              ))}
            </dl>
          ) : null}
          {actions ? <div className="flex flex-wrap gap-2">{actions}</div> : null}
        </div>
      </AlertDescription>
    </Alert>
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
    safeCount(completed ?? successCount + skippedCount + failedCount)
  )
  const percent = safeTotal === 0 ? 0 : (processedCount / safeTotal) * 100
  const statusPresentation = backgroundJobStatusPresentation[status]
  const modeDescription =
    mode === "all-or-nothing"
      ? "整批按原子方式执行；任一项失败时，整批结果不生效。"
      : "允许部分成功；已经形成的有效结果不会因同批其他失败而回退。"

  return (
    <section
      data-slot="background-job-progress"
      data-mode={mode}
      data-status={status}
      aria-live="polite"
      className={cn(
        "flex flex-col gap-4 rounded-2xl border bg-card p-4 text-sm",
        className
      )}
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="flex flex-col gap-1">
          <h3 className="font-medium">{label}</h3>
          <p className="text-muted-foreground">{description ?? modeDescription}</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <StatusBadge
            tone={mode === "all-or-nothing" ? "neutral" : "info"}
            label={mode === "all-or-nothing" ? "整批原子执行" : "允许部分成功"}
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
          <div className="rounded-2xl bg-success-soft p-2 text-success-soft-foreground">
            <dt className="text-xs">成功</dt>
            <dd className="num font-medium">{successCount}</dd>
          </div>
          <div className="rounded-2xl bg-neutral-soft p-2 text-neutral-soft-foreground">
            <dt className="text-xs">跳过</dt>
            <dd className="num font-medium">{skippedCount}</dd>
          </div>
          <div className="rounded-2xl bg-destructive-soft p-2 text-destructive-soft-foreground">
            <dt className="text-xs">失败</dt>
            <dd className="num font-medium">{failedCount}</dd>
          </div>
        </dl>
      ) : null}

      {action ? <div className="flex flex-wrap gap-2">{action}</div> : null}
    </section>
  )
}

type SensitiveValueProps = {
  maskedValue: string
  label?: string
  onReveal?: () => Promise<string>
  autoHideAfterMs?: number
  className?: string
}

const defaultSensitiveValueAutoHideMs = 15_000

/**
 * 敏感值默认只接收脱敏文本。明文只能在用户主动操作后异步取得，并按时清除本地状态。
 */
function SensitiveValue({
  maskedValue,
  label = "敏感信息",
  onReveal,
  autoHideAfterMs = defaultSensitiveValueAutoHideMs,
  className,
}: SensitiveValueProps) {
  const [revealedValue, setRevealedValue] = React.useState<string | null>(null)
  const [isRevealing, setIsRevealing] = React.useState(false)
  const [revealFailed, setRevealFailed] = React.useState(false)
  const requestSequence = React.useRef(0)

  React.useEffect(() => {
    return () => {
      requestSequence.current += 1
    }
  }, [])

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
    setRevealedValue(null)
    setRevealFailed(false)
    setIsRevealing(false)
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

  const isRevealed = revealedValue !== null
  const displayValue = isRevealed ? revealedValue : maskedValue

  return (
    <div
      data-slot="sensitive-value"
      className={cn("inline-flex flex-wrap items-center gap-2", className)}
    >
      <span className="sr-only">{label}：</span>
      <code className="num rounded-xl bg-muted px-2 py-1 font-mono text-sm">
        {displayValue}
      </code>
      {onReveal ? (
        <Button
          type="button"
          variant="ghost"
          size="xs"
          aria-pressed={isRevealed}
          aria-label={isRevealed ? `隐藏${label}` : `显示${label}`}
          disabled={isRevealing}
          onClick={() => {
            if (isRevealed) {
              hide()
              return
            }

            void reveal()
          }}
        >
          {isRevealed ? (
            <EyeOffIcon data-icon="inline-start" aria-hidden="true" />
          ) : (
            <EyeIcon data-icon="inline-start" aria-hidden="true" />
          )}
          {isRevealing ? "读取中" : isRevealed ? "隐藏" : "显示"}
        </Button>
      ) : (
        <LockKeyholeIcon
          className="size-4 text-muted-foreground"
          aria-hidden="true"
        />
      )}
      {isRevealed && autoHideAfterMs > 0 ? (
        <span className="text-xs text-muted-foreground">
          将在 {Math.ceil(autoHideAfterMs / 1000)} 秒后自动隐藏
        </span>
      ) : null}
      {revealFailed ? (
        <span
          role="alert"
          className="flex flex-wrap items-center gap-1.5 text-xs text-destructive"
        >
          <span>暂时无法显示敏感信息</span>
          <Button
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
}: DiscardConfirmDialogProps) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent className="sm:max-w-md">
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>
            {description}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{cancelLabel}</AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={onConfirm}>
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
  type ResultState,
  type SensitiveValueProps,
  type ValidationIssue,
  type ValidationSummaryProps,
}
