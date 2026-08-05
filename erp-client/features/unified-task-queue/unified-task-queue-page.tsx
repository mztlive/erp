"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ArrowRightIcon,
  CircleCheckIcon,
  PauseIcon,
  RefreshCwIcon,
  SearchIcon,
  XIcon,
} from "lucide-react"
import { z } from "zod"

import {
  BusinessEmptyState,
  BusinessStatusBadge,
  ConflictResolutionDialog,
  DataFreshness,
  FormalActionConfirmDialog,
  FormalActionResult,
  PageHeader,
  SequentialProcessBar,
  WorkTaskItem,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Separator } from "@/components/ui/separator"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { FAMILY_LABELS, type WorkItemFamily } from "@/mock/work-items"
import type { SessionLease } from "@/mock/session-state"

import { freshnessText, leaseText, sequentialText, versionText } from "@/lib/ui-text"
import {
  buildFilterSummary,
  filterAndSortWorkItems,
} from "./filter-work-items"
import {
  buildW02SearchParams,
  parseDue,
  parseFamily,
  parseScopeSlug,
  scopeLabel,
} from "./queue-url"
import {
  useClaimWorkItemMutation,
  useCloseWorkItemMutation,
  useCompleteWorkItemMutation,
  useTransferWorkItemMutation,
  useUnifiedTaskQueueQuery,
  useWorkItemActionMutation,
  WorkItemMockError,
} from "./queries"
import type {
  QueueScopeSlug,
  QueueWorkItemView,
  UnifiedQueueFilters,
} from "./types"

const decisionSchema = z.object({
  note: z.string().max(500, "备注不超过 500 字"),
})

type LastResult = {
  status: "succeeded" | "blocked" | "rejected" | "unknown" | "failed"
  title: string
  description: string
  reference: string
}

export function UnifiedTaskQueuePage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const scope = parseScopeSlug(searchParams.get("scope"))
  const family = parseFamily(searchParams.get("family"))
  const workItemType = searchParams.get("type") ?? undefined
  const due = parseDue(searchParams.get("due"))
  const q = searchParams.get("q") ?? ""
  const workItemFromUrl = searchParams.get("currentWorkItemId")
  const converge =
    searchParams.get("converge") === "1" || Boolean(workItemType)
  const queueContextId =
    searchParams.get("queueContextId") ?? `queue:W02:${scope}`

  const filters: UnifiedQueueFilters = React.useMemo(
    () => ({
      scope,
      family,
      workItemType,
      due,
      query: q || undefined,
      converge,
    }),
    [converge, due, family, q, scope, workItemType]
  )

  const queueQuery = useUnifiedTaskQueueQuery(filters)
  const claimMutation = useClaimWorkItemMutation()
  const actionMutation = useWorkItemActionMutation()
  const completeMutation = useCompleteWorkItemMutation()
  const closeMutation = useCloseWorkItemMutation()
  const transferMutation = useTransferWorkItemMutation()

  const [activeLeases, setActiveLeases] = React.useState<
    ReadonlyMap<string, SessionLease>
  >(new Map())
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [conflictOpen, setConflictOpen] = React.useState(false)
  const [conflictInfo, setConflictInfo] = React.useState<{
    localVersion: string
    serverVersion: string
  } | null>(null)
  const [lastResult, setLastResult] = React.useState<LastResult | null>(null)
  const [queueCollapsed, setQueueCollapsed] = React.useState(false)
  const [searchDraft, setSearchDraft] = React.useState(q)
  const titleRef = React.useRef<HTMLHeadingElement>(null)
  const liveRef = React.useRef<HTMLDivElement>(null)

  const sourceItems = queueQuery.data?.items ?? []
  const permissionRevoked = queueQuery.data?.permissionRevoked ?? false

  const dropActiveLease = React.useCallback((workItemId: string) => {
    setActiveLeases((prev) => {
      if (!prev.has(workItemId)) return prev
      const next = new Map(prev)
      next.delete(workItemId)
      return next
    })
  }, [])

  const clearActiveLeases = React.useCallback(() => {
    setActiveLeases(new Map())
  }, [])

  const focusCandidate = React.useMemo(() => {
    if (!workItemFromUrl) return null
    return sourceItems.find((item) => item.id === workItemFromUrl) ?? null
  }, [sourceItems, workItemFromUrl])

  const tasks = React.useMemo(() => {
    const filtered = filterAndSortWorkItems(sourceItems, filters, {
      focus: focusCandidate ?? sourceItems[0] ?? null,
    })
    // Deep-link focus from W01 must appear even if outside current scope tags
    if (
      focusCandidate &&
      !filtered.some((item) => item.id === focusCandidate.id)
    ) {
      return [focusCandidate, ...filtered]
    }
    return filtered
  }, [filters, focusCandidate, sourceItems])

  const urlIndex = workItemFromUrl
    ? tasks.findIndex((item) => item.id === workItemFromUrl)
    : -1
  const currentIndex = urlIndex >= 0 ? urlIndex : 0
  const task = tasks[currentIndex]
  const completed = tasks.length === 0

  const activeClaim = task ? activeLeases.get(task.id) : undefined

  const decisionForm = useAppForm({
    defaultValues: { note: "" },
    validators: { onChange: decisionSchema },
    onSubmit: async () => {
      /* submit handled by explicit complete button */
    },
  })

  // Preserve draft note per work item in a map so lease loss keeps local input
  const draftsRef = React.useRef<Map<string, string>>(new Map())
  React.useEffect(() => {
    if (!task) return
    const saved = draftsRef.current.get(task.id) ?? ""
    decisionForm.setFieldValue("note", saved)
    // focus object title after queue switch
    titleRef.current?.focus()
    if (liveRef.current) {
      liveRef.current.textContent = `第 ${currentIndex + 1}/${tasks.length} 项、${task.workItemTypeLabel}、${task.businessObject}`
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only when current task identity changes
  }, [task?.id, currentIndex, tasks.length])

  const replaceQueueUrl = React.useCallback(
    (next: {
      scope?: QueueScopeSlug
      family?: WorkItemFamily | null
      workItemType?: string | null
      due?: "today" | "overdue" | null
      q?: string | null
      currentWorkItemId?: string | null
      queueContextId?: string
      converge?: boolean
    }) => {
      const qs = buildW02SearchParams({
        scope: next.scope ?? scope,
        family: next.family === null ? null : (next.family ?? family),
        workItemType:
          next.workItemType === null
            ? null
            : (next.workItemType ?? workItemType),
        due: next.due === null ? null : (next.due ?? due),
        q: next.q === null ? null : (next.q ?? q),
        currentWorkItemId: next.currentWorkItemId,
        queueContextId: next.queueContextId ?? queueContextId,
        converge: next.converge ?? converge,
      })
      router.replace(`${pathname}${qs}`, { scroll: false })
    },
    [
      converge,
      due,
      family,
      pathname,
      q,
      queueContextId,
      router,
      scope,
      workItemType,
    ]
  )

  // Shareable defaults: write scope + current item when missing
  React.useEffect(() => {
    if (queueQuery.isPending) return
    const hasScope = searchParams.has("scope")
    const hasItem = searchParams.has("currentWorkItemId")
    if (hasScope && (hasItem || tasks.length === 0)) return
    const nextItem =
      workItemFromUrl && tasks.some((t) => t.id === workItemFromUrl)
        ? workItemFromUrl
        : (tasks[0]?.id ?? null)
    replaceQueueUrl({ currentWorkItemId: nextItem })
  }, [
    queueQuery.isPending,
    replaceQueueUrl,
    searchParams,
    tasks,
    workItemFromUrl,
  ])

  const selectTaskAt = React.useCallback(
    (index: number) => {
      const next = tasks[index]
      if (!next) return
      // Save current draft
      if (task) {
        draftsRef.current.set(task.id, decisionForm.state.values.note)
      }
      replaceQueueUrl({ currentWorkItemId: next.id })
    },
    [decisionForm.state.values.note, replaceQueueUrl, task, tasks]
  )

  const advanceToNext = React.useCallback(
    (fromId: string, list: QueueWorkItemView[]) => {
      const remaining = list.filter((item) => item.id !== fromId)
      const fromIndex = list.findIndex((item) => item.id === fromId)
      const next =
        remaining[fromIndex] ?? remaining[fromIndex - 1] ?? remaining[0] ?? null
      replaceQueueUrl({
        currentWorkItemId: next?.id ?? null,
        // After formal process, converge to same type
        workItemType: next?.workItemType ?? task?.workItemType ?? null,
        converge: true,
      })
    },
    [replaceQueueUrl, task?.workItemType]
  )

  const ensureClaimed = React.useCallback(
    async (item: QueueWorkItemView): Promise<SessionLease> => {
      const existing = activeLeases.get(item.id)
      if (existing) return existing
      const lease = await claimMutation.mutateAsync({
        workItemId: item.id,
        subjectVersion: item.subjectVersion,
      })
      setActiveLeases((prev) => new Map(prev).set(item.id, lease))
      return lease
    },
    [activeLeases, claimMutation]
  )

  const handleMockError = React.useCallback(
    (error: unknown, item: QueueWorkItemView) => {
      if (!(error instanceof WorkItemMockError)) {
        setLastResult({
          status: "failed",
          title: "提交失败",
          description: error instanceof Error ? error.message : "未知错误",
          reference: item.id,
        })
        return
      }
      if (error.code === "LEASE_LOST") {
        dropActiveLease(item.id)
        setLastResult({
          status: "failed",
          title: leaseText.lost,
          description: error.message,
          reference: item.id,
        })
        return
      }
      if (error.code === "VERSION_CONFLICT") {
        setConflictInfo({
          localVersion: item.subjectVersion,
          serverVersion: "已变化（请刷新）",
        })
        setConflictOpen(true)
        setLastResult({
          status: "failed",
          title: "数据已更新",
          description: error.message,
          reference: item.id,
        })
        return
      }
      if (error.code === "PERMISSION_REVOKED") {
        clearActiveLeases()
        setLastResult({
          status: "failed",
          title: "权限已收回",
          description:
            "临时信息已清除。仅保留任务编号与返回上下文。",
          reference: item.id,
        })
        return
      }
      setLastResult({
        status: "failed",
        title: "动作被拒绝",
        description: error.message,
        reference: item.id,
      })
    },
    [clearActiveLeases, dropActiveLease]
  )

  const onSaveEvidence = React.useCallback(async () => {
    if (!task || permissionRevoked) return
    const note = decisionForm.state.values.note
    draftsRef.current.set(task.id, note)
    try {
      const lease = await ensureClaimed(task)
      const record = await actionMutation.mutateAsync({
        workItemId: task.id,
        expectedSubjectVersion: lease.subjectVersion,
        action: { kind: "SAVE_EVIDENCE", note },
      })
      // Task stays PENDING/IN_PROGRESS — do NOT auto-advance
      setLastResult({
        status: "succeeded",
        title: "任务内动作已记录",
        description: `动作 ${record.actionKind} 成功；任务仍为 ${record.workItemStatus}，未完成、未自动下一项。`,
        reference: record.actionRecordId,
      })
    } catch (error) {
      handleMockError(error, task)
    }
  }, [
    actionMutation,
    decisionForm.state.values.note,
    ensureClaimed,
    handleMockError,
    permissionRevoked,
    task,
  ])

  const onDefer = React.useCallback(async () => {
    if (!task || permissionRevoked) return
    const note = decisionForm.state.values.note
    draftsRef.current.set(task.id, note)
    try {
      const lease = await ensureClaimed(task)
      const record = await actionMutation.mutateAsync({
        workItemId: task.id,
        expectedSubjectVersion: lease.subjectVersion,
        action: { kind: "DEFER", note },
      })
      dropActiveLease(task.id)
      setLastResult({
        status: "blocked",
        title: "已跳过",
        description:
          "已记录跳过原因，任务仍在待处理列表，已自动打开下一项。",
        reference: record.actionRecordId,
      })
      // DEFER opens next per §7
      advanceToNext(task.id, tasks)
    } catch (error) {
      handleMockError(error, task)
    }
  }, [
    actionMutation,
    advanceToNext,
    decisionForm.state.values.note,
    dropActiveLease,
    ensureClaimed,
    handleMockError,
    permissionRevoked,
    task,
    tasks,
  ])

  const onComplete = React.useCallback(async () => {
    if (!task || permissionRevoked) return
    if (task.handlerHref) {
      // Domain-bound completion lives in specialized handler; open it with focus
      router.push(task.handlerHref)
      return
    }
    const note = decisionForm.state.values.note
    draftsRef.current.set(task.id, note)
    try {
      const lease = await ensureClaimed(task)
      const result = await completeMutation.mutateAsync({
        workItemId: task.id,
        expectedSubjectVersion: lease.subjectVersion,
        decision: {
          kind: task.completionAction,
          note,
          summary: `${task.workItemTypeLabel}结论与任务完成同一事务`,
        },
      })
      dropActiveLease(task.id)
      setLastResult({
        status: "succeeded",
        title: "完成已生效",
        description: result.businessResult.summary,
        reference: result.completionRecordId,
      })
      // Only advance after confirmed success
      advanceToNext(task.id, tasks)
    } catch (error) {
      handleMockError(error, task)
    }
  }, [
    advanceToNext,
    completeMutation,
    decisionForm.state.values.note,
    dropActiveLease,
    ensureClaimed,
    handleMockError,
    permissionRevoked,
    router,
    task,
    tasks,
  ])

  const onCloseDuplicate = React.useCallback(async () => {
    if (!task || !task.showClose || permissionRevoked) return
    try {
      const lease = await ensureClaimed(task)
      const result = await closeMutation.mutateAsync({
        workItemId: task.id,
        expectedSubjectVersion: lease.subjectVersion,
        closeAllowed: task.closeAllowed,
        closure: {
          kind: "CLOSE_DUPLICATE",
          reasonCode: "DUPLICATE_OF_ACTIVE",
          replacementWorkItemId: "wi_pc_01",
          comment: decisionForm.state.values.note || undefined,
        },
      })
      dropActiveLease(task.id)
      setLastResult({
        status: "succeeded",
        title: "任务已关闭（不改业务记录）",
        description: `关闭原因 ${result.reasonCode}；替代任务 ${result.replacementWorkItemId ?? "—"}`,
        reference: result.closureRecordId,
      })
      advanceToNext(task.id, tasks)
    } catch (error) {
      handleMockError(error, task)
    }
  }, [
    advanceToNext,
    closeMutation,
    decisionForm.state.values.note,
    dropActiveLease,
    ensureClaimed,
    handleMockError,
    permissionRevoked,
    task,
    tasks,
  ])

  const onTransfer = React.useCallback(async () => {
    if (!task || permissionRevoked) return
    if (!task.allowedActions.includes("TRANSFER")) return
    try {
      const lease = await ensureClaimed(task)
      const result = await transferMutation.mutateAsync({
        workItemId: task.id,
        expectedSubjectVersion: lease.subjectVersion,
        transfer: {
          targetUserId: "陈琳",
          reason: decisionForm.state.values.note || "转交业务责任人",
        },
      })
      dropActiveLease(task.id)
      setLastResult({
        status: "succeeded",
        title: "已转交",
        description: `任务已转交 ${result.targetUserId}，仍在处理中，未创建后继任务。`,
        reference: result.transferRecordId,
      })
      advanceToNext(task.id, tasks)
    } catch (error) {
      handleMockError(error, task)
    }
  }, [
    advanceToNext,
    decisionForm.state.values.note,
    dropActiveLease,
    ensureClaimed,
    handleMockError,
    permissionRevoked,
    task,
    tasks,
    transferMutation,
  ])

  // Keyboard: j/k next/prev when not in input
  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      const tag = target?.tagName
      if (
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        target?.isContentEditable
      ) {
        return
      }
      if (event.key === "j" || event.key === "J") {
        event.preventDefault()
        if (currentIndex < tasks.length - 1) selectTaskAt(currentIndex + 1)
      } else if (event.key === "k" || event.key === "K") {
        event.preventDefault()
        if (currentIndex > 0) selectTaskAt(currentIndex - 1)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [currentIndex, selectTaskAt, tasks.length])

  const leaseStatus = React.useMemo(() => {
    if (permissionRevoked) return "lost" as const
    if (!task) return "unclaimed" as const
    if (task.effectiveStatusCode === "UNCLAIMED" && !activeClaim) {
      return "unclaimed" as const
    }
    if (activeClaim) return "active" as const
    if (
      task.effectiveStatusCode === "IN_PROGRESS" ||
      task.effectiveStatusCode === "PENDING"
    ) {
      // Mine-scope tasks are auto-claimable on formal action; treat as active
      if (scope === "mine") {
        return "active" as const
      }
      return "lost" as const
    }
    return "unclaimed" as const
  }, [activeClaim, permissionRevoked, scope, task])

  const processDisabled =
    permissionRevoked ||
    completeMutation.isPending ||
    actionMutation.isPending ||
    leaseStatus === "lost"

  const filterSummary = buildFilterSummary(
    filters,
    tasks.length,
    task?.workItemTypeLabel
  )

  if (queueQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="统一待办队列" description="正在加载队列…" />
        <div className="grid gap-4 xl:grid-cols-[34%_minmax(0,1fr)]">
          <div className="h-64 animate-pulse rounded-xl bg-muted" />
          <div className="h-64 animate-pulse rounded-xl bg-muted" />
        </div>
      </div>
    )
  }

  if (queueQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="统一待办队列" description="队列加载失败" />
        <Button type="button" onClick={() => void queueQuery.refetch()}>
          重试
        </Button>
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="sr-only" aria-live="polite" ref={liveRef} />

      {/* 筛选结果只读给读屏用户；视觉上由工具栏本身表达 */}
      <p className="sr-only" aria-live="polite">
        {filterSummary}
      </p>

      <PageHeader
        title="统一待办队列"
        metadata={
          <DataFreshness
            updatedAt={queueQuery.isFetching ? "正在刷新" : "刚刚"}
            dateTime={queueQuery.data?.freshness.updatedAt}
            state={queueQuery.isFetching ? "syncing" : "fresh"}
            label={freshnessText.dataUpdatedAt}
          />
        }
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={queueQuery.isFetching}
            onClick={() => void queueQuery.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" aria-hidden="true" />
            刷新
          </Button>
        }
      />

      {/* Toolbar: scope · family · due · search，单排排布 */}
      <div className="flex flex-col gap-2 sm:flex-row sm:flex-wrap sm:items-center">
        <ToggleGroup
          value={[scope]}
          onValueChange={(values) => {
            const next = values[0] as QueueScopeSlug | undefined
            if (!next) return
            setLastResult(null)
            replaceQueueUrl({
              scope: next,
              queueContextId: `queue:W02:${next}`,
              currentWorkItemId: null,
              workItemType: null,
              converge: false,
            })
          }}
          variant="outline"
          size="sm"
          spacing={0}
          className="w-fit max-w-full flex-wrap"
        >
          {(
            [
              ["mine", "我的待办", queueQuery.data?.counts.mine],
              ["role_pool", "待领取", queueQuery.data?.counts.rolePool],
              ["team", "团队", undefined],
              ["hold", "已跳过", undefined],
            ] as const
          ).map(([value, label, count]) => (
            <ToggleGroupItem key={value} value={value}>
              {label}
              {typeof count === "number" ? (
                <span className="num ml-1 text-muted-foreground">{count}</span>
              ) : null}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>

        <ToggleGroup
          value={family ? [family] : []}
          onValueChange={(values) => {
            const next = (values[0] as WorkItemFamily | undefined) ?? null
            replaceQueueUrl({
              family: next,
              currentWorkItemId: null,
              workItemType: null,
              converge: false,
            })
          }}
          variant="outline"
          size="sm"
          spacing={0}
          className="w-fit max-w-full flex-wrap"
        >
          {(Object.entries(FAMILY_LABELS) as [WorkItemFamily, string][]).map(
            ([value, label]) => (
              <ToggleGroupItem key={value} value={value}>
                {label}
              </ToggleGroupItem>
            )
          )}
        </ToggleGroup>

        <ToggleGroup
          value={due ? [due] : []}
          onValueChange={(values) => {
            const next = (values[0] as "today" | "overdue" | undefined) ?? null
            replaceQueueUrl({ due: next, currentWorkItemId: null })
          }}
          variant="outline"
          size="sm"
          spacing={0}
          className="w-fit"
        >
          <ToggleGroupItem value="overdue">
            已超期
            {queueQuery.data?.counts.overdue ? (
              <span className="num ml-1 text-destructive">
                {queueQuery.data.counts.overdue}
              </span>
            ) : null}
          </ToggleGroupItem>
          <ToggleGroupItem value="today">今日到期</ToggleGroupItem>
        </ToggleGroup>

        {converge || workItemType ? (
          <BusinessStatusBadge
            context="list"
            label="已筛选为单一类型"
            tone="info"
          />
        ) : null}

        <form
          className="flex min-w-0 flex-1 basis-64 items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault()
            replaceQueueUrl({ q: searchDraft, currentWorkItemId: null })
          }}
        >
          <div className="relative min-w-0 flex-1 max-w-md">
            <SearchIcon
              className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
              aria-hidden="true"
            />
            <Input
              value={searchDraft}
              onChange={(event) => setSearchDraft(event.target.value)}
              placeholder="搜单号、对象或任务编号"
              className="pl-8"
              aria-label="搜索任务"
            />
          </div>
          <Button type="submit" variant="secondary" size="sm">
            搜索
          </Button>
          {q || family || due || workItemType ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => {
                setSearchDraft("")
                replaceQueueUrl({
                  family: null,
                  due: null,
                  q: null,
                  workItemType: null,
                  converge: false,
                  currentWorkItemId: null,
                })
              }}
            >
              清除筛选
            </Button>
          ) : null}
        </form>
      </div>

      {lastResult ? (
        <FormalActionResult
          status={
            lastResult.status === "failed" || lastResult.status === "unknown"
              ? "unknown"
              : lastResult.status
          }
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
            {
              label: "责任范围",
              value: scopeLabel(scope),
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
            <Button render={<Link href="/workspace" />}>
              返回今日工作台
            </Button>
          }
        />
      ) : task ? (
        <div
          className={
            queueCollapsed
              ? "grid min-w-0 gap-4"
              : "grid min-w-0 gap-4 lg:grid-cols-[minmax(12rem,32%)_minmax(0,1fr)] xl:grid-cols-[minmax(14rem,34%)_minmax(0,1fr)]"
          }
        >
          {/* Left queue — collapsible below lg */}
          <Card
            size="sm"
            className={
              queueCollapsed
                ? "hidden"
                : "min-w-0 max-h-[min(70vh,40rem)] overflow-hidden lg:max-h-[calc(100vh-12rem)]"
            }
          >
            <CardHeader className="border-b">
              <div className="flex items-center justify-between gap-2">
                <CardTitle>任务队列</CardTitle>
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  className="lg:hidden"
                  onClick={() => setQueueCollapsed(true)}
                >
                  收起
                </Button>
              </div>
              <CardDescription>
                共 {tasks.length} 项 · 当前第 {currentIndex + 1} 项
                <span className="hidden md:inline"> · j/k 切换上下项</span>
              </CardDescription>
            </CardHeader>
            <CardContent className="max-h-[min(60vh,36rem)] space-y-2 overflow-y-auto lg:max-h-[calc(100vh-16rem)]">
              {tasks.map((item, index) => (
                <button
                  key={item.id}
                  type="button"
                  className={
                    index === currentIndex
                      ? "w-full rounded-lg ring-2 ring-primary"
                      : "w-full rounded-lg hover:bg-muted/50"
                  }
                  onClick={() => selectTaskAt(index)}
                  aria-current={index === currentIndex ? "true" : undefined}
                >
                  <WorkTaskItem
                    // 原因/影响右侧详情已完整展示，队列里只留定位信息
                    density="compact"
                    taskType={item.workItemTypeLabel}
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
            {queueCollapsed ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="lg:hidden"
                onClick={() => setQueueCollapsed(false)}
              >
                显示队列（{currentIndex + 1}/{tasks.length}）
              </Button>
            ) : null}

            {/* Mobile task switcher */}
            <div className="flex items-center gap-2 md:hidden">
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={currentIndex <= 0}
                onClick={() => selectTaskAt(currentIndex - 1)}
              >
                上一项
              </Button>
              <span className="text-sm text-muted-foreground">
                {currentIndex + 1}/{tasks.length}
              </span>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={currentIndex >= tasks.length - 1}
                onClick={() => selectTaskAt(currentIndex + 1)}
              >
                下一项
              </Button>
            </div>

            <div className="sticky top-2 z-10 space-y-2">
              <SequentialProcessBar
                current={currentIndex + 1}
                total={tasks.length}
                leaseStatus={leaseStatus}
                leaseStatusLabel={
                  permissionRevoked
                    ? leaseText.permissionRevokedCleared
                    : activeClaim
                      ? leaseText.activeDoNotReopen
                      : leaseStatus === "unclaimed"
                        ? leaseText.unclaimed
                        : leaseStatus === "lost"
                          ? leaseText.lostRefresh
                          : leaseText.reclaimAfterLost
                }
                processLabel={
                  task.handlerHref
                    ? sequentialText.goProcess
                    : sequentialText.completeCurrent
                }
                processNextLabel={sequentialText.completeAndOpenNext}
                // 跳专用处理器会离开本页，没有「并打开下一条」语义
                showProcessNext={!task.handlerHref}
                processDisabled={processDisabled}
                pending={
                  completeMutation.isPending || actionMutation.isPending
                }
                onBack={() => {
                  router.push("/workspace")
                }}
                onProcess={() => {
                  if (task.handlerHref) {
                    router.push(task.handlerHref)
                    return
                  }
                  if (task.showClose) {
                    void onCloseDuplicate()
                    return
                  }
                  setConfirmOpen(true)
                }}
                onProcessNext={() => {
                  if (task.handlerHref) {
                    router.push(task.handlerHref)
                    return
                  }
                  setConfirmOpen(true)
                }}
                onReclaim={() => {
                  void ensureClaimed(task).then(() => {
                    setLastResult({
                      status: "succeeded",
                      title: "已重新领取",
                      description:
                        "处理进度仅保存在当前页面，未写入 URL 或查询视图。",
                      reference: task.id,
                    })
                  }).catch((error) => handleMockError(error, task))
                }}
              />
            </div>

            <Card size="sm">
              <CardHeader className="border-b">
                <div className="flex flex-wrap items-center gap-2">
                  <CardTitle
                    ref={titleRef}
                    tabIndex={-1}
                    className="outline-none"
                  >
                    {task.businessObject} · {task.counterparty}
                  </CardTitle>
                  <BusinessStatusBadge context="list" {...task.status} />
                  {task.priorityLabel !== "普通" ? (
                    <BusinessStatusBadge
                      context="list"
                      label={task.priorityLabel}
                      tone={
                        task.priorityLabel === "紧急"
                          ? "destructive"
                          : "warning"
                      }
                    />
                  ) : null}
                </div>
                <CardDescription>
                  {task.workItemTypeLabel} · {task.responsibleParty} · 编号{" "}
                  {task.id}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-5">
                {task.effectiveStatusCode === "UNCLAIMED" && !activeClaim ? (
                  <Alert>
                    <AlertTitle>团队任务待认领</AlertTitle>
                    <AlertDescription className="flex flex-wrap items-center gap-2">
                      {leaseText.reclaimHint}。
                      <Button
                        type="button"
                        size="sm"
                        onClick={() =>
                          void ensureClaimed(task)
                            .then(() => {
                              // Enter continuous process for this type
                              replaceQueueUrl({
                                currentWorkItemId: task.id,
                                workItemType: task.workItemType,
                                converge: true,
                              })
                            })
                            .catch((error) => handleMockError(error, task))
                        }
                        disabled={claimMutation.isPending || permissionRevoked}
                      >
                        领取
                      </Button>
                    </AlertDescription>
                  </Alert>
                ) : null}

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
                  {task.impactSensitive ? (
                    <p>
                      <span className="text-muted-foreground">敏感摘要：</span>
                      {task.impactSensitive}
                    </p>
                  ) : null}
                  <p className="text-xs text-muted-foreground">
                    版本 {task.subjectVersion} · 完成动作{" "}
                    {task.completionAction}
                    {task.closeAllowed
                      ? " · 允许关闭（重复/误派）"
                      : " · 无人工关闭入口"}
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

                <Separator />

                <decisionForm.AppForm>
                  <div className="space-y-2">
                    <p className="text-sm font-medium">决策备注</p>
                    <decisionForm.AppField name="note">
                      {(field) => (
                        <field.TextareaField
                          label="备注"
                          placeholder="操作已失效或数据已更新时备注仍保留，但不能提交"
                          rows={3}
                          disabled={permissionRevoked}
                        />
                      )}
                    </decisionForm.AppField>
                  </div>
                </decisionForm.AppForm>

                <div className="flex flex-wrap justify-end gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    disabled={
                      processDisabled && leaseStatus !== "lost"
                        ? true
                        : actionMutation.isPending || permissionRevoked
                    }
                    onClick={() => void onSaveEvidence()}
                  >
                    保存证据
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    disabled={
                      actionMutation.isPending ||
                      permissionRevoked ||
                      leaseStatus === "lost"
                    }
                    onClick={() => void onDefer()}
                  >
                    <PauseIcon data-icon="inline-start" aria-hidden="true" />
                    先跳过并看下一条
                  </Button>
                  {task.allowedActions.includes("TRANSFER") ? (
                    <Button
                      type="button"
                      variant="outline"
                      disabled={
                        transferMutation.isPending ||
                        permissionRevoked ||
                        leaseStatus === "lost"
                      }
                      onClick={() => void onTransfer()}
                    >
                      转交
                    </Button>
                  ) : null}
                  {task.showClose ? (
                    <Button
                      type="button"
                      variant="secondary"
                      disabled={
                        closeMutation.isPending ||
                        permissionRevoked ||
                        leaseStatus === "lost"
                      }
                      onClick={() => void onCloseDuplicate()}
                    >
                      关闭重复任务
                    </Button>
                  ) : null}
                  {task.handlerHref ? (
                    <Button
                      type="button"
                      variant="outline"
                      render={<Link href={task.handlerHref} />}
                    >
                      前往任务页面
                      <ArrowRightIcon
                        data-icon="inline-end"
                        aria-hidden="true"
                      />
                    </Button>
                  ) : null}
                  <Button
                    type="button"
                    variant="destructive"
                    disabled={permissionRevoked}
                    onClick={() => {
                      // generic reject path for non-handler items: domain reject via complete envelope is wrong;
                      // use transfer/return as action — for demo, mark rejected via complete not allowed.
                      // Instead: DEFER-style leave with note is the safe path; show blocked.
                      setLastResult({
                        status: "rejected",
                        title: "退回需在任务对应页面处理",
                        description:
                          "统一队列不提供独立“标记退回完成”伪动作；请前往任务对应页面提交业务退回，或先跳过后再转交。",
                        reference: task.id,
                      })
                    }}
                  >
                    <XIcon data-icon="inline-start" aria-hidden="true" />
                    退回说明
                  </Button>
                  <Button
                    type="button"
                    disabled={processDisabled && !task.handlerHref}
                    onClick={() => {
                      if (task.handlerHref) {
                        router.push(task.handlerHref)
                        return
                      }
                      setConfirmOpen(true)
                    }}
                  >
                    {task.actionLabel ??
                      (task.handlerHref
                        ? sequentialText.goProcess
                        : sequentialText.process)}
                    <ArrowRightIcon
                      data-icon="inline-end"
                      aria-hidden="true"
                    />
                  </Button>
                </div>

                {!converge && !workItemType ? (
                  <Button
                    type="button"
                    size="xs"
                    variant="secondary"
                    onClick={() =>
                      replaceQueueUrl({
                        workItemType: task.workItemType,
                        converge: true,
                        currentWorkItemId: task.id,
                      })
                    }
                  >
                    收敛到同类连续处理
                  </Button>
                ) : (
                  <Button
                    type="button"
                    size="xs"
                    variant="ghost"
                    onClick={() =>
                      replaceQueueUrl({
                        workItemType: null,
                        converge: false,
                      })
                    }
                  >
                    回到全部类型
                  </Button>
                )}
              </CardContent>
            </Card>
          </div>
        </div>
      ) : workItemFromUrl ? (
        <BusinessEmptyState
          kind="filter"
          title="当前任务不在筛选结果中"
          description={`任务 ${workItemFromUrl} 可能已完成、已转交或不匹配当前筛选。`}
          action={
            <Button
              type="button"
              onClick={() =>
                replaceQueueUrl({
                  family: null,
                  due: null,
                  q: null,
                  workItemType: null,
                  converge: false,
                  currentWorkItemId: null,
                })
              }
            >
              回默认有效待办
            </Button>
          }
        />
      ) : null}

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={`确认处理：${task?.workItemTypeLabel ?? ""}`}
        actionLabel="完成"
        confirmLabel="确认完成并打开下一条"
        fromStatus={{ label: task?.status.label ?? "待处理", tone: "warning" }}
        toStatus={{ label: "已完成", tone: "success" }}
        lockedFields={[
          versionText.version,
          leaseText.currentProcessState,
          versionText.dataVersion,
        ]}
        effects={[
          `执行 ${task?.completionAction ?? "领域完成动作"}`,
          "业务记录与任务完成状态在同一提交中返回",
          "无独立「标记完成」伪动作",
        ]}
        nextDepartment="相关责任组"
        onConfirm={async () => {
          await onComplete()
        }}
      />

      <ConflictResolutionDialog
        open={conflictOpen}
        onOpenChange={setConflictOpen}
        currentVersion={conflictInfo?.serverVersion ?? "—"}
        localBaseline={conflictInfo?.localVersion ?? task?.subjectVersion ?? "—"}
        actor="其他处理人或对象变更"
        changedAt="刚刚"
        diff={
          <p className="text-sm">
            任务信息已更新，无法直接提交。你填写的备注已保留，请刷新后重新提交。
          </p>
        }
        onReload={() => {
          void queueQuery.refetch().then(() => {
            // After refresh, claim again with new subject version
            if (task) {
              dropActiveLease(task.id)
            }
            setConflictOpen(false)
          })
        }}
        onSaveCopy={() => {
          if (task) {
            draftsRef.current.set(task.id, decisionForm.state.values.note)
          }
          setConflictOpen(false)
          setLastResult({
            status: "blocked",
            title: "备注已自动保留",
            description: "请重新打开任务后提交。",
            reference: task?.id ?? "—",
          })
        }}
        onCompare={() => {
          setConflictOpen(false)
        }}
      />
    </div>
  )
}
