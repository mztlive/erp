"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ArrowRightIcon,
  CircleCheckIcon,
  PauseIcon,
  XIcon,
} from "lucide-react"

import {
  BusinessEmptyState,
  BusinessStatusBadge,
  FormalActionConfirmDialog,
  FormalActionResult,
  PageHeader,
  SequentialProcessBar,
  WorkTaskItem,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { filterQueueTasks } from "@/features/workspace-kit/filter-queue-tasks"
import {
  buildQueueSearchParams,
  scopeLabelToSlug,
  scopeSlugToLabel,
} from "@/features/workspace-kit/queue-scope"
import {
  useCompleteQueueTaskMutation,
  useWorkspaceQueueQuery,
} from "@/features/workspace-kit/queries"
import type { WorkspacePageDef } from "@/features/workspace-kit/types"
import type { WorkspaceId } from "@/lib/workspace-registry"

export function QueueWorkspacePage({ def }: { def: WorkspacePageDef }) {
  if (def.shell.kind !== "queue") {
    throw new Error(`QueueWorkspacePage expects queue shell for ${def.id}`)
  }
  const { payload } = def.shell
  const workspaceId = def.id as WorkspaceId
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const scope = scopeSlugToLabel(searchParams.get("scope"), payload.scopeLabels)
  const workItemFromUrl = searchParams.get("currentWorkItemId")
  const queueContextId =
    searchParams.get("queueContextId") ??
    `queue:${workspaceId}:${scopeLabelToSlug(scope, payload.scopeLabels)}`

  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [lastResult, setLastResult] = React.useState<{
    status: "succeeded" | "blocked" | "rejected"
    title: string
    description: string
    reference: string
  } | null>(null)

  const queueQuery = useWorkspaceQueueQuery(workspaceId)
  const completeMutation = useCompleteQueueTaskMutation(workspaceId)
  const sourceTasks = React.useMemo(
    () => queueQuery.data ?? [],
    [queueQuery.data]
  )

  const tasks = React.useMemo(
    () =>
      filterQueueTasks(sourceTasks, {
        scope,
        scopeLabels: payload.scopeLabels,
      }),
    [payload.scopeLabels, scope, sourceTasks]
  )

  const urlIndex = workItemFromUrl
    ? tasks.findIndex((item) => item.id === workItemFromUrl)
    : -1
  const currentIndex = urlIndex >= 0 ? urlIndex : 0
  const task = tasks[currentIndex]
  const completed = tasks.length === 0

  const replaceQueueUrl = React.useCallback(
    (next: {
      scopeLabel: string
      currentWorkItemId?: string | null
      queueContextId?: string
    }) => {
      const qs = buildQueueSearchParams({
        scopeLabel: next.scopeLabel,
        scopeLabels: payload.scopeLabels,
        currentWorkItemId: next.currentWorkItemId,
        queueContextId: next.queueContextId ?? queueContextId,
      })
      router.replace(`${pathname}${qs}`, { scroll: false })
    },
    [pathname, payload.scopeLabels, queueContextId, router]
  )

  // Ensure shareable defaults: when URL lacks scope/item, write them once data is ready.
  React.useEffect(() => {
    if (queueQuery.isPending) return
    const hasScope = searchParams.has("scope")
    const hasItem = searchParams.has("currentWorkItemId")
    if (hasScope && (hasItem || tasks.length === 0)) return
    const nextItem =
      workItemFromUrl && tasks.some((t) => t.id === workItemFromUrl)
        ? workItemFromUrl
        : (tasks[0]?.id ?? null)
    replaceQueueUrl({
      scopeLabel: scope,
      currentWorkItemId: nextItem,
    })
  }, [
    queueQuery.isPending,
    replaceQueueUrl,
    scope,
    searchParams,
    tasks,
    workItemFromUrl,
  ])

  const breadcrumbs = def.breadcrumbs.map((item, index) =>
    index === def.breadcrumbs.length - 1 || !item.href
      ? { id: item.id, label: item.label, current: true as const }
      : {
          id: item.id,
          label: item.label,
          href: item.href,
          current: false as const,
        }
  )

  const selectTaskAt = React.useCallback(
    (index: number) => {
      const next = tasks[index]
      replaceQueueUrl({
        scopeLabel: scope,
        currentWorkItemId: next?.id ?? null,
      })
    },
    [replaceQueueUrl, scope, tasks]
  )

  const applyOutcome = React.useCallback(
    async (outcome: "succeeded" | "blocked" | "rejected") => {
      if (!task) return
      // 暂挂保留在有效队列：打开下一条时跳过当前项，但不从 fetch 中删除。
      // 完成/退回：从有效队列移除后，同 index 自然滑入下一项。
      const nextId =
        outcome === "blocked"
          ? (tasks[currentIndex + 1]?.id ?? tasks[0]?.id ?? null)
          : (tasks.filter((item) => item.id !== task.id)[currentIndex]?.id ??
            tasks.filter((item) => item.id !== task.id)[0]?.id ??
            null)
      const result = await completeMutation.mutateAsync({
        taskId: task.id,
        outcome,
      })
      const titles = {
        succeeded: "正式处理已完成",
        blocked: "当前项已暂挂",
        rejected: "已退回补充",
      } as const
      const descriptions = {
        succeeded: "终局成功已写入会话队列状态；刷新后该项不再出现在有效队列。",
        blocked:
          "暂挂已标记为「已暂挂」并仍保留在有效队列；可在「已暂挂」范围查看。已打开下一条。",
        rejected: "退回结论已记录并从有效队列移除（再入列由服务端负责）。",
      } as const
      setLastResult({
        status: outcome,
        title: titles[outcome],
        description: descriptions[outcome],
        reference: result.reference,
      })
      replaceQueueUrl({
        scopeLabel: scope,
        currentWorkItemId: nextId === task.id ? null : nextId,
      })
    },
    [completeMutation, currentIndex, replaceQueueUrl, scope, task, tasks]
  )

  const openSpecializedHandler = React.useCallback(() => {
    if (!task?.handlerHref) return
    router.push(task.handlerHref)
  }, [router, task])

  const onPrimaryAction = React.useCallback(() => {
    if (!task) return
    if (task.handlerHref) {
      openSpecializedHandler()
      return
    }
    setConfirmOpen(true)
  }, [openSpecializedHandler, task])

  if (queueQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title={def.title} description="正在加载队列…" />
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title={def.title}
        description={def.description}
        breadcrumbs={breadcrumbs}
        metadata={
          <span className="text-xs text-muted-foreground">
            当前范围：{scope} · 上下文 {queueContextId} · 截止时间优先
          </span>
        }
      />

      <ToggleGroup
        value={[scope]}
        onValueChange={(values) => {
          const next = values[0]
          if (!next) return
          setLastResult(null)
          const filtered = filterQueueTasks(sourceTasks, {
            scope: next,
            scopeLabels: payload.scopeLabels,
          })
          replaceQueueUrl({
            scopeLabel: next,
            currentWorkItemId: filtered[0]?.id ?? null,
            queueContextId: `queue:${workspaceId}:${scopeLabelToSlug(next, payload.scopeLabels)}`,
          })
        }}
        variant="outline"
        size="sm"
        spacing={0}
        className="w-fit"
      >
        {payload.scopeLabels.map((label) => (
          <ToggleGroupItem key={label} value={label}>
            {label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>

      {lastResult ? (
        <FormalActionResult
          status={lastResult.status}
          title={lastResult.title}
          description={lastResult.description}
          reference={lastResult.reference}
          facts={[
            {
              label: "队列位置",
              value: completed
                ? "本筛选已完成"
                : `第 ${currentIndex + 1} 条待处理`,
            },
          ]}
        />
      ) : null}

      {completed ? (
        <BusinessEmptyState
          kind="no-tasks"
          title="本筛选项已处理完"
          description="当前队列已经清空，可以返回工作台或切换其它责任范围。"
          action={
            <Button render={<Link href="/workspace" />}>返回今日工作台</Button>
          }
        />
      ) : task ? (
        <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(16rem,1fr)_minmax(0,2fr)]">
          <Card size="sm" className="min-w-0">
            <CardHeader className="border-b">
              <CardTitle>任务队列</CardTitle>
              <CardDescription>
                共 {tasks.length} 项 · 当前第 {currentIndex + 1} 项
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-2">
              {tasks.map((item, index) => (
                <button
                  key={item.id}
                  type="button"
                  className={
                    index === currentIndex
                      ? "w-full rounded-lg ring-2 ring-primary"
                      : "w-full"
                  }
                  onClick={() => selectTaskAt(index)}
                >
                  <WorkTaskItem
                    taskType={item.taskType}
                    businessObject={item.businessObject}
                    counterparty={item.counterparty}
                    enteredAt={item.enteredAt}
                    enteredDateTime={item.enteredDateTime}
                    dueAt={item.dueAt}
                    dueDateTime={item.dueDateTime}
                    responsibleParty={item.responsibleParty}
                    reason={item.reason}
                    impact={item.impact}
                    status={item.status}
                  />
                </button>
              ))}
            </CardContent>
          </Card>

          <div className="space-y-4">
            <SequentialProcessBar
              current={currentIndex + 1}
              total={tasks.length}
              leaseStatus="active"
              leaseStatusLabel={
                completeMutation.isPending
                  ? "正在提交正式结果…"
                  : "处理租约有效 · 会话内"
              }
              processLabel={
                task.handlerHref ? "打开专用处理器" : "完成当前项"
              }
              processNextLabel={
                task.handlerHref ? "打开专用处理器" : "完成并打开下一条"
              }
              processDisabled={completeMutation.isPending}
              onBack={() => {
                router.push("/workspace")
              }}
              onProcess={onPrimaryAction}
              onProcessNext={onPrimaryAction}
              onReclaim={() => undefined}
            />

            <Card size="sm">
              <CardHeader className="border-b">
                <div className="flex flex-wrap items-center gap-2">
                  <CardTitle>
                    {task.businessObject} · {task.counterparty}
                  </CardTitle>
                  <BusinessStatusBadge context="list" {...task.status} />
                </div>
                <CardDescription>
                  {task.taskType} · {task.responsibleParty}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-5">
                <section>
                  <h3 className="text-sm font-semibold">任务摘要</h3>
                  <dl className="mt-3 grid gap-px overflow-hidden rounded-lg border border-grid bg-grid sm:grid-cols-2">
                    {task.summaryFields.map((field) => (
                      <div key={field.label} className="bg-card p-3">
                        <dt className="text-xs text-muted-foreground">
                          {field.label}
                        </dt>
                        <dd
                          className={
                            field.numeric
                              ? "num mt-1 font-medium"
                              : "mt-1 font-medium"
                          }
                        >
                          {field.value}
                        </dd>
                      </div>
                    ))}
                  </dl>
                </section>

                <div className="space-y-1 text-sm">
                  <p>
                    <span className="text-muted-foreground">原因：</span>
                    {task.reason}
                  </p>
                  <p>
                    <span className="text-muted-foreground">影响：</span>
                    {task.impact}
                  </p>
                </div>

                {task.checkItems && task.checkItems.length > 0 ? (
                  <>
                    <Separator />
                    <ul className="space-y-2 text-sm" role="list">
                      {task.checkItems.map((item) => (
                        <li key={item} className="flex items-start gap-2">
                          <CircleCheckIcon
                            className="mt-0.5 size-4 shrink-0 text-success"
                            aria-hidden="true"
                          />
                          <span>{item}</span>
                        </li>
                      ))}
                    </ul>
                  </>
                ) : null}

                <div className="flex flex-wrap justify-end gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    disabled={completeMutation.isPending}
                    onClick={() => {
                      void applyOutcome("blocked")
                    }}
                  >
                    <PauseIcon data-icon="inline-start" aria-hidden="true" />
                    暂挂并看下一条
                  </Button>
                  <Button
                    type="button"
                    variant="destructive"
                    disabled={completeMutation.isPending}
                    onClick={() => {
                      void applyOutcome("rejected")
                    }}
                  >
                    <XIcon data-icon="inline-start" aria-hidden="true" />
                    退回
                  </Button>
                  <Button
                    type="button"
                    disabled={completeMutation.isPending}
                    onClick={onPrimaryAction}
                  >
                    {task.actionLabel ??
                      (task.handlerHref ? "打开专用处理器" : "正式处理")}
                    <ArrowRightIcon
                      data-icon="inline-end"
                      aria-hidden="true"
                    />
                  </Button>
                </div>
              </CardContent>
            </Card>
          </div>
        </div>
      ) : null}

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={`确认处理：${task?.taskType ?? ""}`}
        actionLabel="完成"
        confirmLabel="确认完成并打开下一条"
        fromStatus={{ label: task?.status.label ?? "待处理", tone: "warning" }}
        toStatus={{ label: "已完成", tone: "success" }}
        lockedFields={["对象版本", "当前任务租约"]}
        effects={["记录正式处理结果", "从有效队列移除本项"]}
        nextDepartment="相关责任组"
        onConfirm={async () => {
          await applyOutcome("succeeded")
        }}
      />
    </div>
  )
}
