"use client"

import * as React from "react"
import Link from "next/link"
import {
  ArrowRightIcon,
  Clock3Icon,
  RefreshCwIcon,
  TriangleAlertIcon,
} from "lucide-react"

import {
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
  RECENT_WORK,
  WORKSPACE_ALERTS,
  WORKSPACE_TASKS,
  type WorkspaceTaskFilter,
} from "@/mock/workspace"

const FILTER_LABEL: Record<WorkspaceTaskFilter, string> = {
  all: "待我处理",
  today: "今日到期",
  overdue: "已超期",
  sync: "同步异常",
}

export function WorkspaceHomePage() {
  const [filter, setFilter] = React.useState<WorkspaceTaskFilter>("all")
  const [refreshing, setRefreshing] = React.useState(false)
  const [updatedAt, setUpdatedAt] = React.useState("09:36")

  const visibleTasks = React.useMemo(() => {
    const tasks =
      filter === "all"
        ? WORKSPACE_TASKS
        : WORKSPACE_TASKS.filter((task) => task.filterTags.includes(filter))
    return [...tasks].sort((left, right) =>
      left.dueDateTime.localeCompare(right.dueDateTime)
    )
  }, [filter])

  const countFor = React.useCallback(
    (target: WorkspaceTaskFilter) =>
      target === "all"
        ? WORKSPACE_TASKS.length
        : WORKSPACE_TASKS.filter((task) => task.filterTags.includes(target))
            .length,
    []
  )

  const refresh = React.useCallback(() => {
    setRefreshing(true)
    window.setTimeout(() => {
      setUpdatedAt(
        new Intl.DateTimeFormat("zh-CN", {
          hour: "2-digit",
          minute: "2-digit",
          hour12: false,
        }).format(new Date())
      )
      setRefreshing(false)
    }, 450)
  }, [])

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="早上好，王敏"
        description="销售与采购协同工作台 · 先处理超期项，再完成今日到期任务。"
        metadata={
          <DataFreshness
            updatedAt={refreshing ? "正在刷新" : `今天 ${updatedAt}`}
            dateTime="2026-08-01T09:36:00+08:00"
            state={refreshing ? "syncing" : "fresh"}
            label="工作台数据"
          />
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

      <MetricStrip columns={4} aria-label="待办筛选">
        {(["all", "today", "overdue", "sync"] as const).map((item) => (
          <MetricFilterItem
            key={item}
            label={FILTER_LABEL[item]}
            value={countFor(item)}
            detail={
              item === "all"
                ? "全部责任范围"
                : item === "today"
                  ? "今天 18:00 前"
                  : item === "overdue"
                    ? "需要优先处理"
                    : "影响数据水位"
            }
            active={filter === item}
            onClick={() => setFilter(item)}
          />
        ))}
      </MetricStrip>

      <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,3fr)_minmax(18rem,2fr)]">
        <Card size="sm" className="min-w-0">
          <CardHeader className="border-b">
            <CardTitle>{FILTER_LABEL[filter]}</CardTitle>
            <CardDescription aria-live="polite">
              当前筛选共 {visibleTasks.length} 项，按截止时间优先展示。
            </CardDescription>
            {filter !== "all" ? (
              <CardAction>
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={() => setFilter("all")}
                >
                  清除筛选
                </Button>
              </CardAction>
            ) : null}
          </CardHeader>
          <CardContent>
            <div className="space-y-2">
              {visibleTasks.map((task) => (
                <WorkTaskItem
                  key={task.id}
                  taskType={task.taskType}
                  businessObject={task.businessObject}
                  counterparty={task.counterparty}
                  enteredAt={task.enteredAt}
                  enteredDateTime={task.enteredDateTime}
                  dueAt={task.dueAt}
                  dueDateTime={task.dueDateTime}
                  responsibleParty={task.responsibleParty}
                  reason={task.reason}
                  impact={task.impact}
                  status={task.status}
                  nextAction={
                    <Button
                      size="sm"
                      variant={task.status.tone === "destructive" ? "default" : "outline"}
                      render={<Link href={task.href} />}
                    >
                      {task.actionLabel}
                      <ArrowRightIcon data-icon="inline-end" aria-hidden="true" />
                    </Button>
                  }
                />
              ))}
            </div>
          </CardContent>
        </Card>

        <div className="space-y-4">
          <Card size="sm">
            <CardHeader className="border-b">
              <CardTitle>预警与数据水位</CardTitle>
              <CardDescription>只显示需要你关注的异常。</CardDescription>
            </CardHeader>
            <CardContent className="space-y-2">
              {WORKSPACE_ALERTS.map((alert) => (
                <Alert key={alert.id} variant={alert.tone}>
                  {alert.tone === "destructive" ? (
                    <TriangleAlertIcon aria-hidden="true" />
                  ) : (
                    <Clock3Icon aria-hidden="true" />
                  )}
                  <AlertTitle>{alert.title}</AlertTitle>
                  <AlertDescription>{alert.description}</AlertDescription>
                </Alert>
              ))}
            </CardContent>
          </Card>

          <Card size="sm">
            <CardHeader className="border-b">
              <CardTitle>最近打开</CardTitle>
              <CardDescription>继续上次的核对与处理上下文。</CardDescription>
            </CardHeader>
            <CardContent>
              <nav aria-label="最近打开的任务" className="space-y-1">
                {RECENT_WORK.map((item) => (
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
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  )
}
