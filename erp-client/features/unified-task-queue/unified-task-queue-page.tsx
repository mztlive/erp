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
} from "lucide-react"
import { z } from "zod"

import {
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  ConflictResolutionDialog,
  DataFreshness,
  FormalActionConfirmDialog,
  FormalActionResult,
  ListToolbar,
  OptionCombobox,
  PageHeader,
  PageScaffold,
  SequentialProcessBar,
  surfacePanelClassName,
  WorkTaskItem,
} from "@/components/business"
import { cn } from "@/lib/utils"
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
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Separator } from "@/components/ui/separator"
import {
  FAMILY_LABELS,
  type SessionLease,
  type WorkItemFamily,
} from "@/features/unified-task-queue/types"

import {
  actionLabelForWorkItemType,
  freshnessText,
  leaseText,
  sequentialText,
  versionText,
} from "@/lib/ui-text"
import {
  buildFilterSummary,
  filterAndSortWorkItems,
} from "./filter-work-items"
import {
  buildW02SearchParams,
  parseDue,
  parseFamily,
  parseScopeSlug,
  readW02FocusId,
  scopeLabel,
  writeW02FocusId,
} from "./queue-url"
import {
  useBatchClaimWorkItemMutation,
  useClaimWorkItemMutation,
  useCloseWorkItemMutation,
  useCompleteWorkItemMutation,
  useTransferWorkItemMutation,
  useUnifiedTaskQueueQuery,
  useWorkItemActionMutation,
  WorkItemApiError,
} from "./queries"
import type {
  QueueScopeSlug,
  QueueWorkItemView,
  UnifiedQueueFilters,
} from "./types"
import { useTeamOptionsQuery } from "@/hooks/use-options"

const decisionSchema = z.object({
  note: z.string().max(500, "备注不超过 500 字"),
})

type LastResult = {
  status: "succeeded" | "blocked" | "rejected" | "unknown" | "failed"
  title: string
  description: string
  reference: string
  /** 结果编号的标签；批量动作等场景改用业务说法。 */
  referenceLabel?: string
}

export function UnifiedTaskQueuePage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const { data: teamOptions } = useTeamOptionsQuery()

  const scope = parseScopeSlug(searchParams.get("scope"))
  const family = parseFamily(searchParams.get("family"))
  const workItemType = searchParams.get("type") ?? undefined
  const due = parseDue(searchParams.get("due"))
  const q = searchParams.get("q") ?? ""
  // 兼容外部来源页携带的深链参数（W07/W18 等）；本页自身不再写入该参数。
  const workItemFromUrl = searchParams.get("currentWorkItemId")
  const converge =
    searchParams.get("converge") === "1" || Boolean(workItemType)

  // 当前任务焦点走 sessionStorage，内部 ID 不落地址栏。
  const [sessionFocusId, setSessionFocusId] = React.useState<string | null>(
    () => readW02FocusId()
  )

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
  const batchClaimMutation = useBatchClaimWorkItemMutation()
  const actionMutation = useWorkItemActionMutation()
  const completeMutation = useCompleteWorkItemMutation()
  const closeMutation = useCloseWorkItemMutation()
  const transferMutation = useTransferWorkItemMutation()

  const [activeLeases, setActiveLeases] = React.useState<
    ReadonlyMap<string, SessionLease>
  >(new Map())
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [closeConfirmOpen, setCloseConfirmOpen] = React.useState(false)
  const [transferConfirmOpen, setTransferConfirmOpen] = React.useState(false)
  const [transferTarget, setTransferTarget] = React.useState<string>(
    teamOptions?.[0]?.userId ?? ""
  )
  const transferTargetLabel =
    teamOptions?.find((t) => t.userId === transferTarget)?.displayName ??
    transferTarget
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

  // 本会话内确曾领取过的任务集合：区分「从未领取」与「领取后失效」（P1-5）。
  const claimedThisSessionRef = React.useRef<Set<string>>(new Set())

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

  const sourceItems = React.useMemo(
    () => queueQuery.data?.items ?? [],
    [queueQuery.data?.items]
  )

  const focusCandidate = React.useMemo(() => {
    const target = workItemFromUrl ?? sessionFocusId
    if (!target) return null
    return sourceItems.find((item) => item.id === target) ?? null
  }, [sessionFocusId, sourceItems, workItemFromUrl])

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

  const currentFocusId = workItemFromUrl ?? sessionFocusId
  const urlIndex = currentFocusId
    ? tasks.findIndex((item) => item.id === currentFocusId)
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
      converge?: boolean
    }) => {
      if (next.currentWorkItemId === null) {
        writeW02FocusId(null)
        setSessionFocusId(null)
      } else if (next.currentWorkItemId) {
        writeW02FocusId(next.currentWorkItemId)
        setSessionFocusId(next.currentWorkItemId)
      }
      const qs = buildW02SearchParams({
        scope: next.scope ?? scope,
        family: next.family === null ? null : (next.family ?? family),
        workItemType:
          next.workItemType === null
            ? null
            : (next.workItemType ?? workItemType),
        due: next.due === null ? null : (next.due ?? due),
        q: next.q === null ? null : (next.q ?? q),
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
      router,
      scope,
      workItemType,
    ]
  )

  // 搜索框与 URL q 双向同步：浏览器后退后输入框与结果保持一致（P2-6）。
  React.useEffect(() => {
    setSearchDraft(q)
  }, [q])

  // Shareable defaults: write scope when missing; focus stays in sessionStorage.
  React.useEffect(() => {
    if (queueQuery.isPending) return
    if (searchParams.has("scope")) return
    const nextItem = workItemFromUrl ?? tasks[0]?.id ?? null
    replaceQueueUrl({ currentWorkItemId: nextItem })
  }, [queueQuery.isPending, replaceQueueUrl, searchParams, tasks, workItemFromUrl])

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
      claimedThisSessionRef.current.add(item.id)
      return lease
    },
    [activeLeases, claimMutation]
  )

  const handleActionError = React.useCallback(
    (error: unknown, item: QueueWorkItemView) => {
      if (!(error instanceof WorkItemApiError)) {
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
        title: "操作结果已记录",
        description: "已记录本次操作结果，任务仍待处理，未自动打开下一条。",
        reference: record.actionRecordId,
      })
    } catch (error) {
      handleActionError(error, task)
    }
  }, [
    actionMutation,
    decisionForm.state.values.note,
    ensureClaimed,
    handleActionError,
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
      handleActionError(error, task)
    }
  }, [
    actionMutation,
    advanceToNext,
    decisionForm.state.values.note,
    dropActiveLease,
    ensureClaimed,
    handleActionError,
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
      handleActionError(error, task)
    }
  }, [
    advanceToNext,
    completeMutation,
    decisionForm.state.values.note,
    dropActiveLease,
    ensureClaimed,
    handleActionError,
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
        title: "已关闭重复任务",
        description:
          "已关闭当前重复任务，保留已有确认任务，不改动业务记录。",
        reference: result.closureRecordId,
      })
      advanceToNext(task.id, tasks)
    } catch (error) {
      handleActionError(error, task)
    }
  }, [
    advanceToNext,
    closeMutation,
    decisionForm.state.values.note,
    dropActiveLease,
    ensureClaimed,
    handleActionError,
    permissionRevoked,
    task,
    tasks,
  ])

  const onTransfer = React.useCallback(
    async (targetUserId: string) => {
      if (!task || permissionRevoked) return
      if (!task.allowedActions.includes("TRANSFER")) return
      try {
        const lease = await ensureClaimed(task)
        const result = await transferMutation.mutateAsync({
          workItemId: task.id,
          expectedSubjectVersion: lease.subjectVersion,
          transfer: {
            targetUserId,
            reason: decisionForm.state.values.note || `转交${transferTargetLabel}处理`,
          },
        })
        dropActiveLease(task.id)
        setLastResult({
          status: "succeeded",
          title: "已转交",
          description: `任务已转交 ${transferTargetLabel}，仍在待处理列表，已自动打开下一条。`,
          reference: result.transferRecordId,
        })
        advanceToNext(task.id, tasks)
      } catch (error) {
        handleActionError(error, task)
      }
    },
    [
      advanceToNext,
      decisionForm.state.values.note,
      dropActiveLease,
      ensureClaimed,
      handleActionError,
      permissionRevoked,
      task,
      tasks,
      transferMutation,
      transferTargetLabel,
    ]
  )

  const onBatchClaim = React.useCallback(async () => {
    if (!task || permissionRevoked) return
    const unclaimed = tasks.filter(
      (item) =>
        item.effectiveStatusCode === "UNCLAIMED" &&
        !activeLeases.has(item.id)
    )
    if (unclaimed.length === 0) return
    const subjectVersions = Object.fromEntries(
      unclaimed.map((item) => [item.id, item.subjectVersion])
    )
    try {
      const leases = await batchClaimMutation.mutateAsync({
        workItemIds: unclaimed.map((item) => item.id),
        subjectVersions,
      })
      setActiveLeases((prev) => {
        const next = new Map(prev)
        for (const lease of leases) next.set(lease.workItemId, lease)
        return next
      })
      for (const lease of leases) {
        claimedThisSessionRef.current.add(lease.workItemId)
      }
      setLastResult({
        status: "succeeded",
        title: `已领取 ${leases.length} 个任务`,
        description:
          leases.length === unclaimed.length
            ? "当前筛选内的待认领任务已全部领取，可逐个处理。"
            : "部分任务领取失败，其余任务已可处理。",
        reference: `${leases.length} 个`,
        referenceLabel: "领取任务数",
      })
    } catch (error) {
      handleActionError(error, task)
    }
  }, [
    activeLeases,
    batchClaimMutation,
    handleActionError,
    permissionRevoked,
    task,
    tasks,
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

  // 区分「从未领取」与「领取后失效」（P1-5）：从未领取的任务显示「任务待认领」，
  // 只有本会话确曾领取过且处理权失效时才显示「操作已失效 / 重新领取」。
  const leaseStatus = React.useMemo(() => {
    if (permissionRevoked) return "lost" as const
    if (!task) return "unclaimed" as const
    if (activeClaim) return "active" as const
    if (task.effectiveStatusCode === "UNCLAIMED") return "unclaimed" as const
    // Mine-scope tasks are auto-claimable on formal action; treat as active
    if (scope === "mine") return "active" as const
    if (claimedThisSessionRef.current.has(task.id)) return "lost" as const
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
      <PageScaffold>
        <PageHeader title="统一待办队列" description="正在加载队列…" />
        <div className="grid gap-4 xl:grid-cols-[34%_minmax(0,1fr)]">
          <div className="h-64 animate-pulse rounded-lg bg-muted" />
          <div className="h-64 animate-pulse rounded-lg bg-muted" />
        </div>
      </PageScaffold>
    )
  }

  if (queueQuery.isError) {
    return (
      <PageScaffold>
        <PageHeader title="统一待办队列" description="队列加载失败" />
        <BusinessFailureState
          kind="system"
          description="待办队列暂时无法加载，业务记录未被修改。请重试，或返回工作台稍后再来。"
          action={
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => void queueQuery.refetch()}
              >
                重试
              </Button>
              <Button
                type="button"
                variant="ghost"
                render={<Link href="/workspace" />}
              >
                返回工作台
              </Button>
            </div>
          }
        />
      </PageScaffold>
    )
  }

  return (
    <PageScaffold>
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
            variant="ghost"
            size="sm"
            className="text-muted-foreground hover:text-foreground"
            disabled={queueQuery.isFetching}
            onClick={() => void queueQuery.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" aria-hidden="true" />
            刷新
          </Button>
        }
      />

      {/* M3 sticky 处理面：第 0 层 scope + 第 1/2 层 ListToolbar（ui-filter-design §2.3） */}
      <div
        className={cn(
          surfacePanelClassName,
          "sticky top-0 z-10 space-y-2.5 px-3 py-2.5"
        )}
      >
        <div className="flex flex-wrap items-center gap-2">
          <div
            role="group"
            aria-label="责任范围"
            className="inline-flex max-w-full flex-wrap items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
          >
            {(
              [
                ["mine", "我的待办", queueQuery.data?.counts.mine],
                ["role_pool", "团队待认领", queueQuery.data?.counts.rolePool],
                ["team", "团队", queueQuery.data?.counts.team],
                ["hold", "已跳过", queueQuery.data?.counts.hold],
              ] as const
            ).map(([value, label, count]) => {
              const active = scope === value
              return (
                <button
                  key={value}
                  type="button"
                  aria-pressed={active}
                  onClick={() => {
                    setLastResult(null)
                    replaceQueueUrl({
                      scope: value,
                      currentWorkItemId: null,
                      workItemType: null,
                      converge: false,
                    })
                  }}
                  className={cn(
                    "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    active
                      ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                      : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
                  )}
                >
                  {label}
                  {typeof count === "number" ? (
                    <span
                      className={cn(
                        "num",
                        active
                          ? "text-muted-foreground"
                          : "text-muted-foreground/80"
                      )}
                    >
                      {count}
                    </span>
                  ) : null}
                </button>
              )
            })}
          </div>
        </div>

        <ListToolbar
          aria-label="队列筛选"
          search={
            <form
              onSubmit={(event) => {
                event.preventDefault()
                replaceQueueUrl({
                  q: searchDraft.trim() || null,
                  currentWorkItemId: null,
                })
              }}
            >
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  value={searchDraft}
                  onChange={(event) => setSearchDraft(event.target.value)}
                  onBlur={() => {
                    const next = searchDraft.trim()
                    if (next === (q ?? "")) return
                    replaceQueueUrl({
                      q: next || null,
                      currentWorkItemId: null,
                    })
                  }}
                  placeholder="搜单号、对象或往来方"
                  aria-label="搜索任务"
                />
              </InputGroup>
            </form>
          }
          filters={
            <>
              <OptionCombobox
                value={family ?? null}
                options={(
                  Object.entries(FAMILY_LABELS) as [WorkItemFamily, string][]
                ).map(([value, label]) => ({ value, label }))}
                placeholder="任务族：全部"
                size="sm"
                aria-label="任务族"
                inputClassName="w-[10.5rem]"
                onValueChange={(v) =>
                  replaceQueueUrl({
                    family: (v as WorkItemFamily | null) ?? null,
                    currentWorkItemId: null,
                    workItemType: null,
                    converge: false,
                  })
                }
              />
              <OptionCombobox
                value={due ?? null}
                options={[
                  {
                    value: "overdue",
                    label: queueQuery.data?.counts.overdue
                      ? `已超期（${queueQuery.data.counts.overdue}）`
                      : "已超期",
                  },
                  { value: "today", label: "今日到期" },
                ]}
                placeholder="时限：全部"
                size="sm"
                aria-label="到期时限"
                inputClassName="w-[9rem]"
                onValueChange={(v) =>
                  replaceQueueUrl({
                    due: (v as "today" | "overdue" | null) ?? null,
                    currentWorkItemId: null,
                  })
                }
              />
              {task ? (
                converge || workItemType ? (
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() =>
                      replaceQueueUrl({
                        workItemType: null,
                        converge: false,
                      })
                    }
                  >
                    回到全部类型
                  </Button>
                ) : (
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    title="仅保留与当前任务同类的任务，连续处理无需来回切换"
                    onClick={() =>
                      replaceQueueUrl({
                        workItemType: task.workItemType,
                        converge: true,
                        currentWorkItemId: task.id,
                      })
                    }
                  >
                    收敛到同类
                  </Button>
                )
              ) : null}
            </>
          }
          secondary={
            converge || workItemType ? (
              <BusinessStatusBadge
                context="list"
                label="已筛选为单一类型"
                tone="info"
              />
            ) : undefined
          }
          actions={
            <>
              <span className="text-xs text-muted-foreground" aria-live="polite">
                {filterSummary}
              </span>
              {scope === "role_pool" &&
              task &&
              tasks.some(
                (item) =>
                  item.effectiveStatusCode === "UNCLAIMED" &&
                  !activeLeases.has(item.id)
              ) ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={batchClaimMutation.isPending || permissionRevoked}
                  onClick={() => void onBatchClaim()}
                >
                  批量领取
                </Button>
              ) : null}
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
            </>
          }
        />
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
          referenceLabel={lastResult.referenceLabel}
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
        sourceItems.length === 0 ? (
          <BusinessEmptyState
            kind="no-tasks"
            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
            title="当前没有待办任务"
            description="当前责任范围下没有任务。可切换其它责任范围，或返回工作台。"
            action={
              <Button
                variant="secondary"
                className="rounded-lg shadow-none"
                render={<Link href="/workspace" />}
              >
                返回今日工作台
              </Button>
            }
          />
        ) : (
          <BusinessEmptyState
            kind="no-tasks"
            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
            title="本筛选项已处理完"
            description="当前队列已经清空，可以清除筛选、切换责任范围或返回工作台。"
            action={
              <div className="flex flex-wrap gap-2">
                {q || family || due || workItemType ? (
                  <Button
                    type="button"
                    variant="secondary"
                    className="rounded-lg shadow-none"
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
                <Button render={<Link href="/workspace" />}>
                  返回今日工作台
                </Button>
              </div>
            }
          />
        )
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
                : cn(
                    surfacePanelClassName,
                    "min-w-0 max-h-[min(70vh,40rem)] overflow-hidden lg:max-h-[calc(100vh-12rem)]"
                  )
            }
          >
            <CardHeader className="border-b border-border/30">
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
                    nextAction={
                      item.priorityLabel !== "普通" ? (
                        <BusinessStatusBadge
                          context="list"
                          label={item.priorityLabel}
                          tone={
                            item.priorityLabel === "紧急"
                              ? "destructive"
                              : "warning"
                          }
                        />
                      ) : undefined
                    }
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
                      : leaseStatus === "lost"
                        ? leaseText.lostRefresh
                        : leaseText.unclaimed
                }
                processLabel={
                  task.showClose
                    ? (task.actionLabel ?? "关闭重复任务")
                    : task.handlerHref
                      ? (task.actionLabel ??
                        actionLabelForWorkItemType(task.workItemTypeLabel))
                      : sequentialText.completeAndOpenNext
                }
                // 非 handler 任务只保留一个「完成并打开下一条」主按钮（P1-6）
                showProcessNext={false}
                processDisabled={processDisabled}
                pending={
                  completeMutation.isPending || actionMutation.isPending
                }
                backLabel="返回工作台"
                onBack={() => {
                  router.push("/workspace")
                }}
                onProcess={() => {
                  if (task.handlerHref) {
                    router.push(task.handlerHref)
                    return
                  }
                  if (task.showClose) {
                    setCloseConfirmOpen(true)
                    return
                  }
                  setConfirmOpen(true)
                }}
                onProcessNext={() => {
                  if (task.handlerHref) {
                    router.push(task.handlerHref)
                    return
                  }
                  if (task.showClose) {
                    setCloseConfirmOpen(true)
                    return
                  }
                  setConfirmOpen(true)
                }}
                onReclaim={() => {
                  const wasFirstClaim =
                    !claimedThisSessionRef.current.has(task.id)
                  void ensureClaimed(task)
                    .then(() => {
                      setLastResult({
                        status: "succeeded",
                        title: wasFirstClaim ? "已领取任务" : "已重新领取",
                        description:
                          "已恢复对该任务的处理，可继续提交。",
                        reference: task.id,
                      })
                    })
                    .catch((error) => handleActionError(error, task))
                }}
              />
            </div>

            <Card size="sm" className={surfacePanelClassName}>
              <CardHeader className="border-b border-border/30">
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
                  {task.workItemTypeLabel} · {task.responsibleParty}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-5">
                {task.effectiveStatusCode === "UNCLAIMED" && !activeClaim ? (
                  <Alert>
                    <AlertTitle>任务待认领</AlertTitle>
                    <AlertDescription className="flex flex-wrap items-center gap-2">
                      {leaseText.reclaimHint}。
                      <Button
                        type="button"
                        size="sm"
                        onClick={() =>
                          void ensureClaimed(task)
                            .then(() => {
                              // 领取只认领当前任务，不自动收敛队列（P2-9）
                              replaceQueueUrl({
                                currentWorkItemId: task.id,
                              })
                            })
                            .catch((error) => handleActionError(error, task))
                        }
                        disabled={claimMutation.isPending || permissionRevoked}
                      >
                        领取任务
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
                    版本 {task.subjectVersion}
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
                    <div className="flex items-center gap-2">
                      <label
                        htmlFor="transfer-target"
                        className="text-xs text-muted-foreground"
                      >
                        转交至
                      </label>
                      <select
                        id="transfer-target"
                        value={transferTarget}
                        onChange={(event) =>
                          setTransferTarget(event.target.value)
                        }
                        disabled={
                          transferMutation.isPending ||
                          permissionRevoked ||
                          leaseStatus === "lost"
                        }
                        className="h-9 rounded-md border border-input bg-background px-2 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      >
                        {teamOptions?.map((t) => (
                          <option key={t.userId} value={t.userId}>
                            {t.displayName}
                          </option>
                        )) ?? []}
                      </select>
                      <Button
                        type="button"
                        variant="outline"
                        disabled={
                          transferMutation.isPending ||
                          permissionRevoked ||
                          leaseStatus === "lost"
                        }
                        onClick={() => setTransferConfirmOpen(true)}
                      >
                        转交
                      </Button>
                    </div>
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
                      onClick={() => setCloseConfirmOpen(true)}
                    >
                      关闭重复任务
                    </Button>
                  ) : null}
                  <Button
                    type="button"
                    disabled={processDisabled && !task.handlerHref}
                    onClick={() => {
                      if (task.handlerHref) {
                        router.push(task.handlerHref)
                        return
                      }
                      if (task.showClose) {
                        setCloseConfirmOpen(true)
                        return
                      }
                      setConfirmOpen(true)
                    }}
                  >
                    {task.actionLabel ??
                      (task.handlerHref
                        ? actionLabelForWorkItemType(task.workItemTypeLabel)
                        : sequentialText.completeAndOpenNext)}
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
      ) : currentFocusId ? (
        <BusinessEmptyState
          kind="filter"
          className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
          title="当前任务不在筛选结果中"
          description="该任务可能已完成、已转交或不匹配当前筛选。"
          action={
            <Button
              type="button"
              variant="secondary"
              className="rounded-lg shadow-none"
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
          `将提交「${task?.workItemTypeLabel ?? "本任务"}」的处理结论`,
          "处理结论与任务完成在同一次提交中生效",
          "确认后自动打开下一条任务",
        ]}
        nextDepartment="相关责任组"
        onConfirm={async () => {
          await onComplete()
        }}
      />

      <FormalActionConfirmDialog
        open={closeConfirmOpen}
        onOpenChange={setCloseConfirmOpen}
        title={`关闭任务：${task?.workItemTypeLabel ?? ""}`}
        description="将关闭当前重复任务，保留已有确认任务。"
        actionLabel="关闭重复任务"
        confirmLabel="确认关闭重复任务"
        fromStatus={{ label: task?.status.label ?? "待处理", tone: "warning" }}
        toStatus={{ label: "已关闭", tone: "neutral" }}
        lockedFields={[
          versionText.version,
          leaseText.currentProcessState,
          versionText.dataVersion,
        ]}
        effects={[
          "关闭当前重复任务，不改动业务记录",
          "确认后自动打开下一条任务",
        ]}
        irreversibleEffects={[
          "已关闭的任务无法恢复，如需再次处理需重新建任务",
        ]}
        onConfirm={async () => {
          await onCloseDuplicate()
        }}
      />

      <FormalActionConfirmDialog
        open={transferConfirmOpen}
        onOpenChange={setTransferConfirmOpen}
        title={`转交任务：${task?.workItemTypeLabel ?? ""}`}
        description={`任务将转交 ${transferTargetLabel}，由对方继续处理。`}
        actionLabel="转交"
        confirmLabel={`转交${transferTargetLabel}`}
        fromStatus={{ label: task?.status.label ?? "待处理", tone: "warning" }}
        toStatus={{ label: "已转交", tone: "info" }}
        lockedFields={[
          versionText.version,
          leaseText.currentProcessState,
          versionText.dataVersion,
        ]}
        effects={[
          `任务处理权将移交 ${transferTargetLabel}`,
          "转交后任务仍在待处理列表，不改动业务记录",
          "确认后自动打开下一条任务",
        ]}
        onConfirm={async () => {
          await onTransfer(transferTarget)
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
    </PageScaffold>
  )
}
