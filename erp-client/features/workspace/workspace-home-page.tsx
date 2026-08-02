"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ArrowRightIcon,
  ChevronDownIcon,
  Clock3Icon,
  RefreshCwIcon,
  TriangleAlertIcon,
} from "lucide-react"

import {
  AsyncSectionState,
  BusinessEmptyState,
  BusinessFailureState,
  DataFreshness,
  MetricFilterItem,
  MetricStrip,
  PageActions,
  PageHeader,
  WorkTaskItem,
} from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Skeleton } from "@/components/ui/skeleton"
import {
  buildProcessHref,
  buildViewHref,
  buildWarningHref,
} from "@/features/workspace/destination"
import {
  deriveProjectionFreshness,
  deriveWorkItemsFreshness,
  greetingForNow,
} from "@/features/workspace/freshness"
import { useWorkspaceDashboardQuery } from "@/features/workspace/queries"
import {
  buildGroupAllHref,
  buildTaskQueueHref,
  buildWorkspaceSearchParams,
  FILTER_SUMMARY,
  metricKeyFromUrlState,
  parseWorkspaceSearchParams,
  toTodayWorkspaceQuery,
  urlStateFromMetricKey,
  type WorkspaceUrlState,
} from "@/features/workspace/url-state"
import type {
  WorkspaceMetricKey,
  WorkspaceTaskGroup,
  WorkspaceWorkItem,
} from "@/mock/workspace"

const VIEWER_TIMEZONE = "Asia/Shanghai"

function responsiblePartyLabel(item: WorkspaceWorkItem): string {
  if (item.ownerUserLabel) {
    return `${item.ownerRoleLabel} · ${item.ownerUserLabel}`
  }
  return `${item.ownerRoleLabel} · 待领取`
}

function processBlocker(item: WorkspaceWorkItem): string | undefined {
  return item.actionBlockers.find((b) => b.action === "PROCESS")?.message
}

function canProcess(item: WorkspaceWorkItem): boolean {
  return (
    item.allowedActions.includes("PROCESS") &&
    !item.actionBlockers.some((b) => b.action === "PROCESS")
  )
}

function canView(item: WorkspaceWorkItem): boolean {
  return item.allowedActions.includes("VIEW")
}

function WorkspaceHomeSkeleton() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="space-y-2">
        <Skeleton className="h-8 w-64" />
        <Skeleton className="h-4 w-96 max-w-full" />
      </div>
      <div className="grid gap-px overflow-hidden rounded-lg border sm:grid-cols-2 lg:grid-cols-4">
        {Array.from({ length: 4 }).map((_, index) => (
          <Skeleton key={index} className="h-24 rounded-none" />
        ))}
      </div>
      <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,3fr)_minmax(18rem,2fr)]">
        <Skeleton className="min-h-80 w-full" />
        <div className="space-y-4">
          <Skeleton className="h-40 w-full" />
          <Skeleton className="h-32 w-full" />
        </div>
      </div>
    </div>
  )
}

function TaskGroupSection({
  group,
  focusWorkItemId,
  onOpenTask,
  groupAllHref,
}: {
  group: WorkspaceTaskGroup
  focusWorkItemId?: string
  onOpenTask: (item: WorkspaceWorkItem) => void
  groupAllHref: string
}) {
  const [open, setOpen] = React.useState(group.defaultExpanded)
  const headingId = `task-group-${group.family}`
  const previewLimit =
    group.pagePreviewLimit ??
    // TEMPORARY design fallback only — not an acceptance contract.
    5
  const previewItems = group.items.slice(0, previewLimit)
  const hasMore = group.total > previewItems.length

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <div className="rounded-lg border">
        <CollapsibleTrigger
          className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-sm font-medium hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          aria-expanded={open}
          aria-controls={`${headingId}-panel`}
          id={headingId}
        >
          <span>
            {group.label}
            <span className="ml-2 text-muted-foreground num">
              {group.total}
            </span>
          </span>
          <ChevronDownIcon
            aria-hidden="true"
            className={
              open ? "size-4 shrink-0 rotate-180 transition" : "size-4 shrink-0 transition"
            }
          />
        </CollapsibleTrigger>
        <CollapsibleContent
          id={`${headingId}-panel`}
          role="region"
          aria-labelledby={headingId}
        >
          <div className="space-y-2 border-t p-3">
            {previewItems.map((item) => {
              const processOk = canProcess(item)
              const viewOk = canView(item)
              const blocker = processBlocker(item)
              const processHref = buildProcessHref(item)
              const viewHref = buildViewHref(item)
              const isFocused = focusWorkItemId === item.workItemId

              return (
                <div
                  key={item.workItemId}
                  id={`work-item-${item.workItemId}`}
                  data-work-item-id={item.workItemId}
                  tabIndex={isFocused ? -1 : undefined}
                  className={
                    isFocused
                      ? "rounded-lg ring-2 ring-ring ring-offset-2"
                      : undefined
                  }
                >
                  <WorkTaskItem
                    taskType={item.workItemTypeLabel}
                    businessObject={item.objectTitle}
                    counterparty={item.counterpartyName}
                    enteredAt={item.enteredAtLabel}
                    enteredDateTime={item.createdAt}
                    dueAt={item.dueAtLabel}
                    dueDateTime={item.dueAt}
                    responsibleParty={responsiblePartyLabel(item)}
                    reason={item.reasonLabel}
                    impact={item.impactSummary}
                    status={{
                      label: item.statusLabel,
                      tone: item.statusTone,
                    }}
                    nextAction={
                      <div className="flex flex-col items-end gap-1">
                        {processOk ? (
                          <Button
                            size="sm"
                            variant={
                              item.statusTone === "destructive"
                                ? "default"
                                : "outline"
                            }
                            render={
                              <Link
                                href={processHref}
                                onClick={() => onOpenTask(item)}
                              />
                            }
                          >
                            处理
                            <ArrowRightIcon
                              data-icon="inline-end"
                              aria-hidden="true"
                            />
                          </Button>
                        ) : (
                          <Button
                            size="sm"
                            variant="outline"
                            disabled
                            title={blocker}
                            aria-disabled="true"
                          >
                            处理
                          </Button>
                        )}
                        {!processOk && viewOk ? (
                          <Button
                            size="xs"
                            variant="ghost"
                            render={
                              <Link
                                href={viewHref}
                                onClick={() => onOpenTask(item)}
                              />
                            }
                          >
                            查看
                          </Button>
                        ) : null}
                        {blocker ? (
                          <span className="max-w-40 text-right text-xs text-muted-foreground">
                            {blocker}
                          </span>
                        ) : null}
                      </div>
                    }
                  />
                </div>
              )
            })}
            {hasMore ? (
              <div className="flex justify-end pt-1">
                <Button
                  size="sm"
                  variant="ghost"
                  render={<Link href={groupAllHref} />}
                >
                  查看该组全部 {group.total} 条
                  <ArrowRightIcon
                    data-icon="inline-end"
                    aria-hidden="true"
                  />
                </Button>
              </div>
            ) : null}
          </div>
        </CollapsibleContent>
      </div>
    </Collapsible>
  )
}

export function WorkspaceHomePage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const urlState = React.useMemo(
    () => parseWorkspaceSearchParams(searchParams),
    [searchParams]
  )

  const queryInput = React.useMemo(
    () => toTodayWorkspaceQuery(urlState, VIEWER_TIMEZONE),
    [urlState]
  )

  const dashboardQuery = useWorkspaceDashboardQuery(queryInput)
  const view = dashboardQuery.data
  const refreshing =
    dashboardQuery.isFetching && !dashboardQuery.isPending && !!view

  const activeMetric = metricKeyFromUrlState(urlState)
  const hasActiveFilter = Boolean(urlState.due || urlState.family)

  const pushUrl = React.useCallback(
    (next: WorkspaceUrlState) => {
      const qs = buildWorkspaceSearchParams(next)
      router.push(`${pathname}${qs}`, { scroll: false })
    },
    [pathname, router]
  )

  const onMetricClick = React.useCallback(
    (key: WorkspaceMetricKey) => {
      pushUrl(urlStateFromMetricKey(key, urlState))
    },
    [pushUrl, urlState]
  )

  const clearFilters = React.useCallback(() => {
    pushUrl({
      scope: urlState.scope,
      scenario: urlState.scenario,
    })
  }, [pushUrl, urlState.scope, urlState.scenario])

  const onOpenTask = React.useCallback(
    (item: WorkspaceWorkItem) => {
      // Persist focus in the current history entry so browser back restores it.
      const qs = buildWorkspaceSearchParams({
        ...urlState,
        focusWorkItemId: item.workItemId,
      })
      window.history.replaceState(null, "", `${pathname}${qs}`)
    },
    [pathname, urlState]
  )

  const refresh = React.useCallback(() => {
    void dashboardQuery.refetch()
  }, [dashboardQuery])

  // Restore task focus after return from a target page (or refresh with focus in URL).
  React.useEffect(() => {
    if (!view || !urlState.focusWorkItemId) return
    const el = document.getElementById(
      `work-item-${urlState.focusWorkItemId}`
    )
    if (!el) return
    el.scrollIntoView({ block: "nearest", behavior: "smooth" })
    if (el instanceof HTMLElement) {
      el.focus({ preventScroll: true })
    }
  }, [view, urlState.focusWorkItemId, urlState.due, urlState.family])

  if (dashboardQuery.isPending && !view) {
    return <WorkspaceHomeSkeleton />
  }

  if (dashboardQuery.isError && !view) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessFailureState
          kind="system"
          description="今日工作台暂时无法加载，请重试。业务记录未被修改。"
          action={
            <Button type="button" variant="outline" onClick={refresh}>
              重试
            </Button>
          }
        />
      </div>
    )
  }

  if (!view) {
    return <WorkspaceHomeSkeleton />
  }

  if (view.access === "forbidden") {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessFailureState
          kind="permission"
          title="无今日工作台权限"
          description="当前账号没有今日工作台模块权限。入口应已隐藏；若通过链接直接访问，请联系管理员开通权限。"
        />
      </div>
    )
  }

  if (view.access === "no_data_scope") {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title={greetingForNow(view.viewer.displayName)}
          description={`${view.viewer.activeRoleLabel}工作台`}
        />
        <BusinessEmptyState
          kind="no-scope"
          title="当前角色无数据范围"
          description="你可以进入此页面，但当前权限范围内没有可查看的任务与指标。系统不会展示虚假的 0 指标。"
        />
      </div>
    )
  }

  const projectionFreshness = deriveProjectionFreshness(view.freshness, {
    refreshing,
  })
  const workItemsFreshness = deriveWorkItemsFreshness(view.freshness, {
    refreshing,
  })

  const visibleCount = view.groups.reduce((sum, group) => sum + group.total, 0)
  const filterLabel = FILTER_SUMMARY[activeMetric]
  const taskQueueHref = buildTaskQueueHref(urlState)
  const metrics = view.metrics.filter((metric) => metric.visible)

  const asyncStatus = dashboardQuery.isError
    ? "error"
    : refreshing
      ? "refreshing"
      : "success"

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title={greetingForNow(view.viewer.displayName)}
        description={`${view.viewer.activeRoleLabel}工作台 · 先处理超期项，再完成今日到期任务。`}
        metadata={
          <div className="flex flex-col gap-1 sm:flex-row sm:flex-wrap sm:items-center sm:gap-x-4">
            <DataFreshness
              label="待办"
              updatedAt={workItemsFreshness.updatedAtLabel}
              dateTime={workItemsFreshness.dateTime}
              state={workItemsFreshness.state}
              statusLabel={workItemsFreshness.statusLabel}
            />
            <DataFreshness
              label="工作台汇总"
              updatedAt={projectionFreshness.updatedAtLabel}
              dateTime={projectionFreshness.dateTime}
              state={projectionFreshness.state}
              statusLabel={projectionFreshness.statusLabel}
            />
          </div>
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "refresh",
                label: refreshing ? "刷新中" : "刷新",
                icon: RefreshCwIcon,
                variant: "outline",
                disabled: refreshing,
                onClick: refresh,
              },
            ]}
          />
        }
      />

      {projectionFreshness.state === "stale" ||
      projectionFreshness.state === "failed" ? (
        <Alert
          variant={
            projectionFreshness.state === "failed" ? "destructive" : "warning"
          }
        >
          <TriangleAlertIcon aria-hidden="true" />
          <AlertTitle>
            {projectionFreshness.state === "failed"
              ? "工作台数据更新失败"
              : "工作台数据可能不是最新"}
          </AlertTitle>
          <AlertDescription>
            {projectionFreshness.state === "failed"
              ? "指标与预警不得视为实时；待办任务仍可按下方任务列表处理。"
              : "数据更新已超过 1 分钟，指标可能不是最新；待办任务仍可按下方列表处理。"}
          </AlertDescription>
        </Alert>
      ) : null}

      <MetricStrip columns={4} aria-label="待办筛选">
        {metrics.map((metric) => (
          <MetricFilterItem
            key={metric.key}
            label={metric.label}
            value={metric.count}
            detail={metric.detail}
            active={activeMetric === metric.key}
            onClick={() => onMetricClick(metric.key)}
          />
        ))}
      </MetricStrip>

      <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,3fr)_minmax(18rem,2fr)]">
        <Card size="sm" className="min-w-0">
          <CardHeader className="border-b">
            <CardTitle id="workspace-task-main-title">{filterLabel}</CardTitle>
            <CardDescription aria-live="polite" aria-atomic="true">
              {hasActiveFilter
                ? `当前筛选「${filterLabel}」共 ${visibleCount} 项`
                : `当前共 ${visibleCount} 项待办，按超期、优先级与截止时间展示`}
            </CardDescription>
            <CardAction className="flex flex-wrap gap-1">
              {hasActiveFilter ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={clearFilters}
                >
                  清除筛选
                </Button>
              ) : null}
              {view.canOpenTaskQueue ? (
                <Button
                  size="xs"
                  variant="outline"
                  render={<Link href={taskQueueHref} />}
                >
                  查看全部待办
                  <ArrowRightIcon
                    data-icon="inline-end"
                    aria-hidden="true"
                  />
                </Button>
              ) : null}
            </CardAction>
          </CardHeader>
          <CardContent>
            <AsyncSectionState
              status={asyncStatus}
              refreshingLabel="正在刷新，仍显示上次结果"
              error="工作台任务刷新失败，已保留上次成功内容。"
              errorKind={dashboardQuery.isError ? "projection" : "system"}
              retryAction={
                <Button type="button" variant="outline" size="sm" onClick={refresh}>
                  重试
                </Button>
              }
            >
              {visibleCount === 0 && !hasActiveFilter ? (
                <BusinessEmptyState
                  kind="no-tasks"
                  title="当前没有待处理事项"
                  description="当前没有待处理事项。可查看最近打开记录或进入其它业务模块。"
                />
              ) : null}

              {visibleCount === 0 && hasActiveFilter ? (
                <BusinessEmptyState
                  kind="filter"
                  title="当前筛选无结果"
                  description={`没有符合「${filterLabel}」的待办。可清除筛选查看全部任务。`}
                  action={
                    <Button type="button" variant="outline" onClick={clearFilters}>
                      清除筛选
                    </Button>
                  }
                />
              ) : null}

              {visibleCount > 0 ? (
                <div className="space-y-3" role="list" aria-labelledby="workspace-task-main-title">
                  {view.groups.map((group) => (
                    <div key={group.family} role="listitem">
                      <TaskGroupSection
                        group={group}
                        focusWorkItemId={urlState.focusWorkItemId}
                        onOpenTask={onOpenTask}
                        groupAllHref={buildGroupAllHref(urlState, group.family)}
                      />
                    </div>
                  ))}
                </div>
              ) : null}
            </AsyncSectionState>
          </CardContent>
        </Card>

        <div className="space-y-4">
          <Card size="sm">
            <CardHeader className="border-b">
              <CardTitle>预警与数据新鲜度</CardTitle>
              <CardDescription>
                只显示需要你关注的异常
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-2">
              {view.warnings.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  当前没有需要关注的预警。
                </p>
              ) : (
                view.warnings.map((warning) => (
                  <Alert
                    key={warning.warningId}
                    variant={
                      warning.severity === "destructive"
                        ? "destructive"
                        : warning.severity === "warning"
                          ? "warning"
                          : "default"
                    }
                  >
                    {warning.severity === "destructive" ? (
                      <TriangleAlertIcon aria-hidden="true" />
                    ) : (
                      <Clock3Icon aria-hidden="true" />
                    )}
                    <AlertTitle>{warning.title}</AlertTitle>
                    <AlertDescription className="flex flex-col gap-2">
                      <span>{warning.description}</span>
                      <Button
                        size="xs"
                        variant="outline"
                        className="w-fit"
                        render={
                          <Link href={buildWarningHref(warning)} />
                        }
                      >
                        打开处理面
                        <ArrowRightIcon
                          data-icon="inline-end"
                          aria-hidden="true"
                        />
                      </Button>
                    </AlertDescription>
                  </Alert>
                ))
              )}
            </CardContent>
          </Card>

          <Card size="sm">
            <CardHeader className="border-b">
              <CardTitle>最近打开</CardTitle>
              <CardDescription>
                快速回到上次处理的任务。
              </CardDescription>
            </CardHeader>
            <CardContent>
              {view.recent.length === 0 ? (
                <p className="text-sm text-muted-foreground">暂无最近记录。</p>
              ) : (
                <nav aria-label="最近打开的任务" className="space-y-1">
                  {view.recent.map((item) => (
                    <Button
                      key={item.id}
                      variant="ghost"
                      className="w-full justify-between"
                      render={<Link href={item.href} />}
                    >
                      <span className="truncate">{item.label}</span>
                      <ArrowRightIcon aria-hidden="true" />
                    </Button>
                  ))}
                </nav>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  )
}
