"use client"

import * as React from "react"
import {
  ArrowLeftIcon,
  ArrowRightIcon,
  CheckIcon,
  CircleCheckIcon,
  CircleDashedIcon,
  Clock3Icon,
  EyeIcon,
  FileCheck2Icon,
  FilterIcon,
  ListChecksIcon,
  LoaderCircleIcon,
  LockIcon,
  PencilIcon,
  RefreshCwIcon,
  RotateCcwIcon,
  SaveIcon,
  ShieldAlertIcon,
  ShieldCheckIcon,
  TriangleAlertIcon,
  UserRoundIcon,
  UsersRoundIcon,
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
  AlertDialogMedia,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import {
  StatusBadge,
  type StatusTone,
} from "@/components/ui/status-badge"
import { leaseText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

type ControllableDialogProps = {
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
}

function useControllableDialog({
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
    [onOpenChange, open]
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
  fallbackIcon: LucideIcon
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
          : "border-border bg-muted"
      )}
    >
      <h3
        className={cn(
          "flex items-center gap-2 text-sm font-medium",
          tone === "destructive"
            ? "text-destructive-soft-foreground"
            : "text-foreground"
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
                : "text-muted-foreground"
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
  const isPending = pending || internalPending
  const sourceStatus = normalizeStatus(
    fromStatus,
    "neutral",
    CircleDashedIcon
  )
  const targetStatus = normalizeStatus(toStatus, "info", CircleCheckIcon)

  const handleConfirm = React.useCallback(async () => {
    setInternalPending(true)
    try {
      await onConfirm()
      setOpen(false)
    } catch (error) {
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
          <AlertDialogTitle>{title ?? `确认${actionLabel}`}</AlertDialogTitle>
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
              <AlertDescription>{nextDepartment}</AlertDescription>
            </Alert>
          ) : null}

          <WorkflowDetailList
            title="无法自动撤回的影响"
            icon={TriangleAlertIcon}
            items={irreversibleEffects}
            tone="destructive"
          />
        </div>

        <AlertDialogFooter>
          <AlertDialogCancel
            disabled={isPending}
            onClick={onCancel}
          >
            <ArrowLeftIcon data-icon="inline-start" aria-hidden="true" />
            {cancelLabel}
          </AlertDialogCancel>
          <AlertDialogAction
            variant={
              irreversibleEffects.length > 0 ? "destructive" : "default"
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
              <CheckIcon data-icon="inline-start" aria-hidden="true" />
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
  const canProcess =
    leaseStatus === "active" && !pending && !processDisabled
  const canReclaim =
    showProcess && (leaseStatus === "unclaimed" || leaseStatus === "lost")

  return (
    <section
      data-slot="sequential-process-bar"
      aria-label="连续处理操作"
      className={cn(
        "flex flex-col gap-4 rounded-2xl border border-border bg-card p-4 lg:flex-row lg:items-center lg:justify-between",
        className
      )}
      {...props}
    >
      <div className="flex min-w-0 flex-wrap items-center gap-3" aria-live="polite">
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
          <ArrowLeftIcon data-icon="inline-start" aria-hidden="true" />
          返回队列
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
            {pending ? "正在重新领取" : "重新领取"}
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
              <CheckIcon data-icon="inline-start" aria-hidden="true" />
            )}
            {pending ? "正在处理" : processLabel}
          </Button>
        ) : null}
        {showProcess && showProcessNext ? (
          <Button
            type="button"
            disabled={!canProcess}
            onClick={onProcessNext}
          >
            {pending ? (
              <LoaderCircleIcon
                data-icon="inline-start"
                aria-hidden="true"
                className="animate-spin"
              />
            ) : (
              <ArrowRightIcon data-icon="inline-end" aria-hidden="true" />
            )}
            {pending ? "正在处理" : processNextLabel}
          </Button>
        ) : null}
      </div>
    </section>
  )
}

export type BatchImpactPreviewProps = Omit<
  React.ComponentProps<typeof Card>,
  "children" | "title"
> & {
  title?: React.ReactNode
  description?: React.ReactNode
  filterSummary: React.ReactNode
  selectionScope: React.ReactNode
  estimated: number
  processable: number
  skipped: number
  background: boolean
  sensitiveFields?: readonly string[]
  skippedReason?: React.ReactNode
}

/** 批量动作执行前的范围、数量、后台任务和敏感字段预览。 */
function BatchImpactPreview({
  title = "批量操作影响预览",
  description = "执行前请再次核对当前筛选、选择范围和预计处理结果。",
  filterSummary,
  selectionScope,
  estimated,
  processable,
  skipped,
  background,
  sensitiveFields = [],
  skippedReason,
  className,
  ...props
}: BatchImpactPreviewProps) {
  return (
    <Card
      data-slot="batch-impact-preview"
      className={className}
      {...props}
    >
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <CardTitle className="flex items-center gap-2">
              <ListChecksIcon aria-hidden="true" className="size-4" />
              {title}
            </CardTitle>
            <CardDescription className="mt-1">{description}</CardDescription>
          </div>
          <StatusBadge
            tone={background ? "info" : "neutral"}
            icon={background ? Clock3Icon : CircleCheckIcon}
            label={background ? "将创建后台任务" : "当前页面内执行"}
          />
        </div>
      </CardHeader>

      <CardContent className="space-y-4">
        <dl className="grid gap-3 sm:grid-cols-2">
          <div className="rounded-xl border border-border bg-muted p-4">
            <dt className="flex items-center gap-2 text-sm font-medium text-foreground">
              <FilterIcon aria-hidden="true" className="size-4" />
              当前筛选
            </dt>
            <dd className="mt-2 text-sm text-muted-foreground">
              {filterSummary}
            </dd>
          </div>
          <div className="rounded-xl border border-border bg-muted p-4">
            <dt className="flex items-center gap-2 text-sm font-medium text-foreground">
              <ShieldCheckIcon aria-hidden="true" className="size-4" />
              选择范围
            </dt>
            <dd className="mt-2 text-sm text-muted-foreground">
              {selectionScope}
            </dd>
          </div>
        </dl>

        <dl className="grid gap-px overflow-hidden rounded-xl border border-border bg-border sm:grid-cols-3">
          <div className="bg-card p-4">
            <dt className="flex items-center gap-2 text-sm text-muted-foreground">
              <ListChecksIcon aria-hidden="true" className="size-4" />
              预计处理
            </dt>
            <dd className="num mt-2 text-2xl font-semibold text-foreground">
              {estimated.toLocaleString("zh-CN")}
            </dd>
          </div>
          <div className="bg-card p-4">
            <dt className="flex items-center gap-2 text-sm text-muted-foreground">
              <CircleCheckIcon aria-hidden="true" className="size-4" />
              可处理
            </dt>
            <dd className="num mt-2 text-2xl font-semibold text-foreground">
              {processable.toLocaleString("zh-CN")}
            </dd>
          </div>
          <div className="bg-card p-4">
            <dt className="flex items-center gap-2 text-sm text-muted-foreground">
              <TriangleAlertIcon aria-hidden="true" className="size-4" />
              将跳过
            </dt>
            <dd className="num mt-2 text-2xl font-semibold text-foreground">
              {skipped.toLocaleString("zh-CN")}
            </dd>
            {skippedReason != null ? (
              <div className="mt-2 text-sm text-muted-foreground">
                {skippedReason}
              </div>
            ) : null}
          </div>
        </dl>

        {sensitiveFields.length > 0 ? (
          <Alert>
            <ShieldAlertIcon aria-hidden="true" />
            <AlertTitle>包含敏感字段</AlertTitle>
            <AlertDescription>
              <p>结果将继续执行当前用户的字段权限和遮罩规则。</p>
              <ul className="mt-2 flex flex-wrap gap-2" role="list">
                {sensitiveFields.map((field) => (
                  <li key={field}>
                    <StatusBadge
                      tone="warning"
                      icon={ShieldAlertIcon}
                      label={field}
                    />
                  </li>
                ))}
              </ul>
            </AlertDescription>
          </Alert>
        ) : null}
      </CardContent>
    </Card>
  )
}

export type ConflictAction = "reload" | "save-copy" | "compare"

export type ConflictResolutionDialogProps = ControllableDialogProps & {
  trigger?: React.ReactElement
  title?: string
  description?: React.ReactNode
  currentVersion: React.ReactNode
  localBaseline: React.ReactNode
  actor: React.ReactNode
  changedAt?: React.ReactNode
  diff: React.ReactNode
  pendingAction?: ConflictAction
  onReload: () => void
  onSaveCopy: () => void
  onCompare: () => void
}

/** 并发版本冲突的差异查看与安全处理入口。 */
function ConflictResolutionDialog({
  trigger,
  title = "数据已更新",
  description = "当前数据已经更新，你输入的内容不能直接覆盖。",
  currentVersion,
  localBaseline,
  actor,
  changedAt,
  diff,
  pendingAction,
  onReload,
  onSaveCopy,
  onCompare,
  open,
  defaultOpen,
  onOpenChange,
}: ConflictResolutionDialogProps) {
  const [resolvedOpen, setOpen] = useControllableDialog({
    open,
    defaultOpen,
    onOpenChange,
  })
  const isPending = pendingAction != null

  return (
    <Dialog open={resolvedOpen} onOpenChange={setOpen}>
      {trigger ? <DialogTrigger render={trigger} /> : null}
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <TriangleAlertIcon
              aria-hidden="true"
              className="size-4 text-destructive"
            />
            {title}
          </DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <dl className="grid gap-3 sm:grid-cols-2">
            <div className="rounded-xl border border-border bg-muted p-4">
              <dt className="text-sm font-medium text-muted-foreground">
                当前服务端版本
              </dt>
              <dd className="mt-2 flex flex-wrap items-center gap-2">
                <StatusBadge
                  tone="success"
                  icon={CircleCheckIcon}
                  label="当前有效"
                />
                <span className="num font-medium text-foreground">
                  {currentVersion}
                </span>
              </dd>
            </div>
            <div className="rounded-xl border border-border bg-muted p-4">
              <dt className="text-sm font-medium text-muted-foreground">
                你输入的内容版本
              </dt>
              <dd className="mt-2 flex flex-wrap items-center gap-2">
                <StatusBadge
                  tone="warning"
                  icon={Clock3Icon}
                  label="数据已过期"
                />
                <span className="num font-medium text-foreground">
                  {localBaseline}
                </span>
              </dd>
            </div>
          </dl>

          <Alert>
            <UserRoundIcon aria-hidden="true" />
            <AlertTitle>当前版本的最近变更</AlertTitle>
            <AlertDescription>
              <span className="font-medium text-foreground">{actor}</span>
              {changedAt != null ? (
                <span className="num ml-2">{changedAt}</span>
              ) : null}
            </AlertDescription>
          </Alert>

          <section aria-labelledby="conflict-diff-title">
            <h3
              id="conflict-diff-title"
              className="flex items-center gap-2 text-sm font-medium text-foreground"
            >
              <ListChecksIcon aria-hidden="true" className="size-4" />
              版本差异
            </h3>
            <div className="mt-2 rounded-xl border border-border bg-card p-4">
              {diff}
            </div>
          </section>
        </div>

        <DialogFooter>
          <DialogClose render={<Button variant="ghost" disabled={isPending} />}>
            取消
          </DialogClose>
          <Button
            type="button"
            variant="outline"
            disabled={isPending}
            onClick={onCompare}
          >
            {pendingAction === "compare" ? (
              <LoaderCircleIcon
                data-icon="inline-start"
                aria-hidden="true"
                className="animate-spin"
              />
            ) : (
              <EyeIcon data-icon="inline-start" aria-hidden="true" />
            )}
            查看差异
          </Button>
          <Button
            type="button"
            variant="secondary"
            disabled={isPending}
            onClick={onSaveCopy}
          >
            {pendingAction === "save-copy" ? (
              <LoaderCircleIcon
                data-icon="inline-start"
                aria-hidden="true"
                className="animate-spin"
              />
            ) : (
              <SaveIcon data-icon="inline-start" aria-hidden="true" />
            )}
            保留为新草稿
          </Button>
          <Button
            type="button"
            disabled={isPending}
            onClick={onReload}
          >
            {pendingAction === "reload" ? (
              <LoaderCircleIcon
                data-icon="inline-start"
                aria-hidden="true"
                className="animate-spin"
              />
            ) : (
              <RotateCcwIcon data-icon="inline-start" aria-hidden="true" />
            )}
            重新加载
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export type EditorPresenceUser = Readonly<{
  id: React.Key
  name: React.ReactNode
}>

export type EditorPresenceProps = Omit<
  React.ComponentProps<"aside">,
  "children"
> & {
  viewers?: readonly EditorPresenceUser[]
  editors?: readonly EditorPresenceUser[]
  reminder?: React.ReactNode
}

function PresenceNames({ users }: { users: readonly EditorPresenceUser[] }) {
  return (
    <>
      {users.map((user, index) => (
        <React.Fragment key={user.id}>
          {index > 0 ? "、" : null}
          <span>{user.name}</span>
        </React.Fragment>
      ))}
    </>
  )
}

/** 同一草稿的查看/编辑协作提醒；明确说明它不是业务锁。 */
function EditorPresence({
  viewers = [],
  editors = [],
  reminder = "这是协作提醒；提交仍以系统最新数据为准。",
  className,
  ...props
}: EditorPresenceProps) {
  const hasPresence = viewers.length > 0 || editors.length > 0

  return (
    <aside
      data-slot="editor-presence"
      aria-label="协作状态"
      aria-live="polite"
      className={cn(
        "rounded-2xl border border-border bg-card p-4",
        className
      )}
      {...props}
    >
      <div className="flex flex-wrap items-center gap-2">
        {editors.length > 0 ? (
          <StatusBadge
            tone="warning"
            icon={PencilIcon}
            label={`${editors.length.toLocaleString("zh-CN")} 人正在编辑`}
          />
        ) : null}
        {viewers.length > 0 ? (
          <StatusBadge
            tone="info"
            icon={EyeIcon}
            label={`${viewers.length.toLocaleString("zh-CN")} 人正在查看`}
          />
        ) : null}
        {!hasPresence ? (
          <StatusBadge
            tone="neutral"
            icon={UsersRoundIcon}
            label="当前无其他协作者"
          />
        ) : null}
      </div>

      {editors.length > 0 ? (
        <p className="mt-3 flex items-start gap-2 text-sm text-foreground">
          <PencilIcon
            aria-hidden="true"
            className="mt-0.5 size-4 shrink-0 text-muted-foreground"
          />
          <span>
            正在编辑：<PresenceNames users={editors} />
          </span>
        </p>
      ) : null}
      {viewers.length > 0 ? (
        <p className="mt-2 flex items-start gap-2 text-sm text-foreground">
          <EyeIcon
            aria-hidden="true"
            className="mt-0.5 size-4 shrink-0 text-muted-foreground"
          />
          <span>
            正在查看：<PresenceNames users={viewers} />
          </span>
        </p>
      ) : null}

      <div className="mt-3 flex items-start gap-2 text-sm text-muted-foreground">
        <UserRoundIcon aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
        <p>{reminder}</p>
      </div>
    </aside>
  )
}

export {
  BatchImpactPreview,
  ConflictResolutionDialog,
  EditorPresence,
  FormalActionConfirmDialog,
  SequentialProcessBar,
}
