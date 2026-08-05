"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ArrowRightIcon,
  CircleCheckIcon,
  EyeIcon,
  PauseIcon,
  SaveIcon,
  Undo2Icon,
} from "lucide-react"

import {
  BusinessEmptyState,
  BusinessStatusBadge,
  DataFreshness,
  FormalActionConfirmDialog,
  FormalActionResult,
  MetricFilterItem,
  MetricStrip,
  PageHeader,
  PrepaymentGate,
  SequentialProcessBar,
  ValidationSummary,
} from "@/components/business"
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
import { Separator } from "@/components/ui/separator"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import type {
  DeferOutcome,
  FulfillmentDraft,
  FulfillmentFormalOutcome,
  FulfillmentOperationType,
} from "@/features/fulfillment-operations/types"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
import {
  CORRECTION_NOTICE,
  DEFER_REASON_LABEL,
  NOT_ACCEPTANCE_NOTICE,
  OPERATION_ACTION_LABEL,
  OPERATION_CLEARED_LABEL,
  OPERATION_CONFIRM_TITLE,
  OPERATION_DONE_LABEL,
  OPERATION_TYPE_LABEL,
  OPERATION_TYPE_SHORT,
  SLUG_TO_TYPE,
  TYPE_SLUG,
} from "@/features/fulfillment-operations/types"
import {
  useClaimFulfillmentMutation,
  useDeferFulfillmentMutation,
  useFulfillmentQueueQuery,
  usePostFulfillmentMutation,
  useResolveUnknownFulfillmentMutation,
  useSaveFulfillmentMutation,
} from "@/features/fulfillment-operations/queries"
import {
  resolveRole,
  ROLE_OPTIONS,
  type FulfillmentRole,
} from "@/features/fulfillment-operations/fulfillment-roles"
import {
  FULFILLMENT_LANES,
  laneHeader,
  resolveLane,
} from "@/features/fulfillment-operations/lanes"
import {
  parseDueParam,
  parseGateParam,
  parseTypeParam,
  typeParamValue,
} from "@/features/fulfillment-operations/filters"
import { cn } from "@/lib/utils"
import {
  buildPostedFacts,
  clientValidation,
  cloneDraft,
  impactPreview,
} from "@/features/fulfillment-operations/validation"
import {
  FIRST_INPUT_ID,
  FulfillmentDraftForm,
} from "@/features/fulfillment-operations/fulfillment-draft-form"
import { FulfillmentQueueList } from "@/features/fulfillment-operations/fulfillment-queue-list"
import { FulfillmentQueueToolbar } from "@/features/fulfillment-operations/fulfillment-queue-toolbar"
import {
  FulfillmentDeferDialog,
  type DeferSubmitValue,
} from "@/features/fulfillment-operations/fulfillment-defer-dialog"
import { freshnessText, resultText, workspaceLabel } from "@/lib/ui-text"
import type { WorkspaceId } from "@/lib/workspace-registry"

type SessionLease = {
  workItemId: string
}

type ResultState = SharedResultState<FulfillmentFormalOutcome | DeferOutcome>

/**
 * 演示专用控件（人为制造「结果未确定」、跳过查询直接结算）只在开发环境出现。
 * unknown 态仅由演示开关产生，故其「查一下到底成没成」始终保留。
 */
const DEV_SIMULATION_ENABLED = process.env.NODE_ENV !== "production"

export function FulfillmentOperationsPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const lane = resolveLane(
    searchParams.get("lane"),
    searchParams.get("demoRole")
  )
  const header = laneHeader(lane)
  // 显式 demoRole 优先；否则按岗位通道默认（仓储/采购经办）；都没有时回落默认角色
  const role = resolveRole(
    searchParams.get("demoRole") ??
      (lane ? FULFILLMENT_LANES[lane].defaultDemoRole : null)
  )
  const scope: "mine" | "role_pool" =
    searchParams.get("scope") === "role_pool" ? "role_pool" : "mine"
  const operationTypes = parseTypeParam(searchParams.get("type"))
  const warehouseId = searchParams.get("warehouseId") ?? undefined
  const q = searchParams.get("q") ?? undefined
  const due = parseDueParam(searchParams.get("due"))
  const gate = parseGateParam(searchParams.get("gate"))
  const salesOrderId = searchParams.get("salesOrderId") ?? undefined
  const purchaseOrderId = searchParams.get("purchaseOrderId") ?? undefined
  const currentWorkItemId =
    searchParams.get("currentWorkItemId") ?? undefined
  /** 队列上下文里的岗位段；无岗位时用 any，避免拼出 "queue:W09:null:…" */
  const laneKey = lane ?? "any"
  const queueContextId =
    searchParams.get("queueContextId") ??
    `queue:W09:${laneKey}:${role.value}:${scope}`
  const returnTo = searchParams.get("returnTo") ?? undefined
  const fromWorkspace = searchParams.get("from") ?? undefined

  const autoNextExplicit = searchParams.get("autoNext")
  const [sessionAutoNext, setSessionAutoNext] = React.useState(true)
  const autoNext =
    autoNextExplicit === "0"
      ? false
      : autoNextExplicit === "1"
        ? true
        : sessionAutoNext

  const filters = React.useMemo(
    (): import("@/features/fulfillment-operations/api").FulfillmentQueueFilters => ({
      role: role.value,
      scope,
      operationTypes,
      warehouseId,
      q,
      due,
      gate,
      salesOrderId,
      purchaseOrderId,
      currentWorkItemId,
      queueContextId,
    }),
    [
      role.value,
      scope,
      operationTypes,
      warehouseId,
      q,
      due,
      gate,
      salesOrderId,
      purchaseOrderId,
      currentWorkItemId,
      queueContextId,
    ]
  )

  const queueQuery = useFulfillmentQueueQuery(filters)
  const claimMutation = useClaimFulfillmentMutation()
  const saveMutation = useSaveFulfillmentMutation()
  const postMutation = usePostFulfillmentMutation()
  const deferMutation = useDeferFulfillmentMutation()
  const resolveUnknownMutation = useResolveUnknownFulfillmentMutation()

  const view = queueQuery.data
  const tasks = view?.tasks ?? []
  const context = view?.context
  const visibleTypes = context?.visibleTypes ?? role.types
  const metrics = view?.metrics ?? []
  const task =
    tasks.find((t) => t.workItemId === currentWorkItemId) ??
    view?.current ??
    tasks[0]
  const currentIndex = task
    ? Math.max(
        0,
        tasks.findIndex((t) => t.workItemId === task.workItemId)
      )
    : 0
  const completed = Boolean(view) && tasks.length === 0

  const [draft, setDraft] = React.useState<FulfillmentDraft | null>(null)
  const [dirty, setDirty] = React.useState(false)
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [deferOpen, setDeferOpen] = React.useState(false)
  const [lastResult, setLastResult] = React.useState<ResultState>(null)
  const [forceUnknownOnce, setForceUnknownOnce] = React.useState(false)
  const [shortcutsOpen, setShortcutsOpen] = React.useState(false)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [saveMessage, setSaveMessage] = React.useState<string | null>(null)
  const headingRef = React.useRef<HTMLHeadingElement>(null)
  const resultRef = React.useRef<HTMLDivElement>(null)
  const leaseRef = React.useRef<SessionLease | null>(null)
  const [leaseEpoch, setLeaseEpoch] = React.useState(0)

  React.useEffect(() => {
    if (!task) {
      setDraft(null)
      setDirty(false)
      return
    }
    setDraft(cloneDraft(task.draft))
    setDirty(false)
    setActionError(null)
    setSaveMessage(null)
  }, [task?.workItemId, task?.editVersion])

  React.useEffect(() => {
    if (queueQuery.isPending || !view) return
    const hasLane = searchParams.has("lane")
    const hasScope = searchParams.has("scope")
    const hasItem = searchParams.has("currentWorkItemId")
    const hasCtx = searchParams.has("queueContextId")
    // 没有确定岗位（只读角色 / 未声明岗位的深链）就不写 lane，
    // 否则等于替用户认领一个他没选的岗位，侧栏还会跟着高亮错的入口。
    const laneSettled = hasLane || lane === null
    if (
      laneSettled &&
      hasScope &&
      hasCtx &&
      (hasItem || tasks.length === 0)
    ) {
      return
    }
    const params = new URLSearchParams(searchParams.toString())
    if (!hasLane && lane) params.set("lane", lane)
    if (!hasScope) params.set("scope", scope)
    if (!hasCtx) params.set("queueContextId", queueContextId)
    if (!hasItem && task) {
      params.set("currentWorkItemId", task.workItemId)
    }
    const qs = params.toString()
    router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
  }, [
    queueQuery.isPending,
    view,
    searchParams,
    lane,
    scope,
    queueContextId,
    task,
    tasks.length,
    pathname,
    router,
  ])

  React.useEffect(() => {
    if (!task) return
    // 只读角色不占用别人的处理权
    if (!role.canExecute) return
    if (leaseRef.current?.workItemId === task.workItemId) return
    if (claimMutation.isPending) return
    let cancelled = false
    void claimMutation
      .mutateAsync(task.workItemId)
      .then((lease) => {
        if (cancelled) return
        leaseRef.current = {
          workItemId: lease.workItemId,
        }
        setLeaseEpoch((n) => n + 1)
      })
      .catch(() => {
        /* 未领取 */
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅任务切换时接手
  }, [task?.workItemId, role.canExecute])

  React.useEffect(() => {
    if (lastResult) {
      resultRef.current?.focus()
      return
    }
    if (!task) return
    // 可执行角色直接落到第一个要填的框并全选，省一次鼠标；
    // 标题挂了 aria-live，换条时仍会播报，不靠抢焦点来通知。
    if (role.canExecute) {
      const el = document.getElementById(
        FIRST_INPUT_ID[task.operationType]
      ) as HTMLInputElement | HTMLTextAreaElement | null
      if (el) {
        el.focus()
        el.select?.()
        return
      }
    }
    headingRef.current?.focus()
  }, [task?.workItemId, task?.operationType, lastResult?.status, role.canExecute])

  const replaceUrl = React.useCallback(
    (patch: Record<string, string | null | undefined>) => {
      const params = new URLSearchParams(searchParams.toString())
      for (const [key, value] of Object.entries(patch)) {
        if (value == null || value === "") params.delete(key)
        else params.set(key, value)
      }
      const qs = params.toString()
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    },
    [pathname, router, searchParams]
  )

  const goToWorkItem = React.useCallback(
    (workItemId: string | undefined | null) => {
      setLastResult(null)
      setActionError(null)
      replaceUrl({
        currentWorkItemId: workItemId ?? null,
      })
    },
    [replaceUrl]
  )

  const neighborId = React.useCallback(
    (delta: number) => {
      const idx = currentIndex + delta
      return tasks[idx]?.workItemId
    },
    [currentIndex, tasks]
  )

  const activeLease =
    leaseRef.current?.workItemId === task?.workItemId
      ? leaseRef.current
      : null
  void leaseEpoch

  const validationIssues =
    task && draft ? clientValidation(task, draft) : []
  const canPost =
    role.canExecute &&
    Boolean(task && draft) &&
    validationIssues.length === 0 &&
    !(task?.gate.state === "BLOCKED" && task.operationType !== "WAREHOUSE_SHIP") &&
    !(task?.actionBlockers.some((b) => b.action === "POST"))

  const formalPending =
    postMutation.isPending ||
    deferMutation.isPending ||
    claimMutation.isPending

  const ensureLease = React.useCallback(async () => {
    if (!task) throw new Error("无当前任务")
    if (leaseRef.current?.workItemId === task.workItemId) {
      return leaseRef.current
    }
    const lease = await claimMutation.mutateAsync(task.workItemId)
    const session: SessionLease = {
      workItemId: lease.workItemId,
    }
    leaseRef.current = session
    setLeaseEpoch((n) => n + 1)
    return session
  }, [claimMutation, task])

  const updateDraft = React.useCallback((next: FulfillmentDraft) => {
    setDraft(next)
    setDirty(true)
  }, [])

  /** 回到最近一次保存的草稿；多处「请先保存或放弃」提示都指向这里 */
  const handleDiscard = React.useCallback(() => {
    if (!task) return
    setDraft(cloneDraft(task.draft))
    setDirty(false)
    setActionError(null)
    setSaveMessage(null)
  }, [task])

  const handleSave = React.useCallback(async () => {
    if (!task || !draft) return
    try {
      await ensureLease()
      await saveMutation.mutateAsync({
        workItemId: task.workItemId,
        expectedEditVersion: task.editVersion,
        draft,
      })
      setDirty(false)
      setSaveMessage("草稿已保存")
      setActionError(null)
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "保存失败")
    }
  }, [draft, ensureLease, saveMutation, task])

  const advanceIfNeeded = React.useCallback(
    (shouldAdvance: boolean, preferredNext?: string) => {
      if (!shouldAdvance) return
      const nextId =
        preferredNext ??
        neighborId(1) ??
        tasks.find((t) => t.workItemId !== task?.workItemId)?.workItemId
      leaseRef.current = null
      setLeaseEpoch((n) => n + 1)
      if (nextId) goToWorkItem(nextId)
      else replaceUrl({ currentWorkItemId: null })
    },
    [goToWorkItem, neighborId, replaceUrl, task?.workItemId, tasks]
  )

  const handlePost = React.useCallback(async () => {
    if (!task || !draft) return
    setActionError(null)
    try {
      await ensureLease()
      const nextId = neighborId(1)
      const response = await postMutation.mutateAsync({
        workItemId: task.workItemId,
        expectedSubjectVersion: task.sourceVersion,
        expectedSourceVersion: task.sourceVersion,
        expectedEditVersion: task.editVersion,
        draft,
        nextWorkItemId: nextId,
        forceUnknown: forceUnknownOnce,
      })
      setForceUnknownOnce(false)
      setConfirmOpen(false)

      if (response.status === "unknown") {
        setLastResult({
          status: "unknown",
          title: resultText.unknown,
          description: response.message,
          pendingIdempotencyKey: response.idempotencyKey,
          stayOnItem: true,
        })
        return
      }
      if (response.status === "failed") {
        setActionError(response.message)
        return
      }
      if (response.outcome.kind !== "POSTED") return
      leaseRef.current = null
      setLeaseEpoch((n) => n + 1)
      setLastResult({
        status: "succeeded",
        title: OPERATION_DONE_LABEL[response.outcome.operationType],
        description: autoNext
          ? "已记下来了，马上打开下一条。"
          : "已记下来了。可以先核对一下库存变化再继续。",
        reference: response.outcome.factNo,
        outcome: response.outcome,
        stayOnItem: !autoNext,
      })
      if (autoNext) {
        advanceIfNeeded(true, response.outcome.nextWorkItemId)
      }
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "没能提交成功")
    }
  }, [
    advanceIfNeeded,
    autoNext,
    draft,
    ensureLease,
    forceUnknownOnce,
    neighborId,
    postMutation,
    task,
  ])

  const handleDefer = React.useCallback(
    async (value: DeferSubmitValue) => {
      if (!task) return
      setActionError(null)
      try {
        if (dirty) await handleSave()
        await ensureLease()
        const nextId = neighborId(1)
        const response = await deferMutation.mutateAsync({
          workItemId: task.workItemId,
          queueContextId,
          reasonCode: value.reasonCode,
          reasonNote: value.reasonNote?.trim() || undefined,
          nextWorkItemId: nextId,
        })
        setDeferOpen(false)
        if (
          response.status !== "succeeded" ||
          response.outcome.kind !== "DEFERRED"
        ) {
          if (response.status === "failed") setActionError(response.message)
          return
        }
        leaseRef.current = null
        setLeaseEpoch((n) => n + 1)
        setLastResult({
          status: "blocked",
          title: "已跳过这一条",
          description: `原因：${DEFER_REASON_LABEL[response.outcome.reasonCode]}${
            response.outcome.reasonNote
              ? ` · ${response.outcome.reasonNote}`
              : ""
          }。任务仍在待处理队列；本次处理已结束并进入下一条。`,
          reference: response.outcome.reference,
          outcome: response.outcome,
        })
        if (nextId) goToWorkItem(nextId)
      } catch (error) {
        setActionError(error instanceof Error ? error.message : "没能跳过")
      }
    },
    [
      deferMutation,
      dirty,
      ensureLease,
      goToWorkItem,
      handleSave,
      neighborId,
      queueContextId,
      task,
    ]
  )

  const handleResolveUnknown = React.useCallback(
    async (settle: boolean) => {
      if (!task || !draft) return
      const lease = leaseRef.current
      const response = await resolveUnknownMutation.mutateAsync({
        workItemId: task.workItemId,
        settle,
        settlePayload:
          settle && lease
            ? {
                workItemId: task.workItemId,
                expectedSubjectVersion: task.sourceVersion,
                expectedSourceVersion: task.sourceVersion,
                expectedEditVersion: task.editVersion,
                draft,
                nextWorkItemId: neighborId(1),
              }
            : undefined,
      })
      if (response.status === "unknown") {
        setLastResult({
          status: "unknown",
          title: "还是没查到结果",
          description: response.message,
          pendingIdempotencyKey: response.idempotencyKey,
          stayOnItem: true,
        })
        return
      }
      if (response.status === "failed") {
        setActionError(response.message)
        return
      }
      if (response.outcome.kind === "POSTED") {
        setLastResult({
          status: "succeeded",
          title: "查到了：这一条已经做完",
          description: "查到的是同一条记录，库存和留货没有被重复改动。",
          reference: response.outcome.factNo,
          outcome: response.outcome,
          stayOnItem: !autoNext,
        })
        if (autoNext) advanceIfNeeded(true, response.outcome.nextWorkItemId)
      }
    },
    [
      advanceIfNeeded,
      autoNext,
      draft,
      neighborId,
      resolveUnknownMutation,
      task,
    ]
  )

  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      const tag = target?.tagName
      const inField =
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        target?.isContentEditable

      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
        event.preventDefault()
        // 只读角色按 Ctrl+S 不能触发保存 —— 那会去抢处理权
        if (role.canExecute) void handleSave()
        return
      }
      if (
        (event.metaKey || event.ctrlKey) &&
        event.key === "Enter" &&
        !inField
      ) {
        event.preventDefault()
        // 与底部主按钮同条件：处理权由 handlePost 内的 ensureLease 兜底补领
        if (canPost && !formalPending) setConfirmOpen(true)
        return
      }
      if (inField) return
      if (event.key === "?") {
        event.preventDefault()
        setShortcutsOpen((v) => !v)
        return
      }
      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault()
        if (dirty) {
          setActionError("有没保存的修改，先保存或放弃再切换")
          return
        }
        const next = neighborId(1)
        if (next) goToWorkItem(next)
      }
      if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault()
        if (dirty) {
          setActionError("有未保存修改，请先保存或放弃后再切换")
          return
        }
        const prev = neighborId(-1)
        if (prev) goToWorkItem(prev)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [
    canPost,
    dirty,
    formalPending,
    goToWorkItem,
    handleSave,
    neighborId,
    role.canExecute,
  ])

  const setTypeFilter = React.useCallback(
    (next: FulfillmentOperationType | "all") => {
      if (dirty) {
        setActionError("有没保存的修改，先保存或放弃再切换类型")
        return
      }
      setLastResult(null)
      replaceUrl({
        type: next === "all" ? null : TYPE_SLUG[next],
        currentWorkItemId: null,
        queueContextId: `queue:W09:${laneKey}:${role.value}:${scope}:${next === "all" ? "all" : TYPE_SLUG[next]}`,
      })
    },
    [dirty, laneKey, replaceUrl, role.value, scope]
  )

  /** 空态出口：类型、单号、仓库、到期、门禁和来源对象筛选一次清干净 */
  const clearAllFilters = React.useCallback(() => {
    if (dirty) {
      setActionError("有没保存的修改，先保存或放弃再清除筛选")
      return
    }
    setLastResult(null)
    replaceUrl({
      type: null,
      q: null,
      warehouseId: null,
      due: null,
      gate: null,
      salesOrderId: null,
      purchaseOrderId: null,
      currentWorkItemId: null,
      queueContextId: `queue:W09:${laneKey}:${role.value}:${scope}:all`,
    })
  }, [dirty, laneKey, replaceUrl, role.value, scope])

  /** 演示身份切换时同步岗位通道，侧栏高亮跟当前干活的人走 */
  const handleRolePatch = React.useCallback(
    (patch: Record<string, string | null | undefined>) => {
      if (dirty) {
        setActionError("有没保存的修改，先保存或放弃再改筛选")
        return
      }
      setLastResult(null)
      const next: Record<string, string | null | undefined> = { ...patch }
      if (patch.demoRole != null) {
        const nextRole = patch.demoRole as FulfillmentRole
        // 只读角色（销售/财务）队列里五类都在，没有归属岗位：清掉 lane 走中性页头，
        // 否则页头会顶着「收货与发货」列出电子交付和线下服务。
        const nextLane =
          nextRole === "warehouse" || nextRole === "procurement"
            ? nextRole
            : null
        next.lane = nextLane
        next.currentWorkItemId = null
        next.queueContextId = `queue:W09:${nextLane ?? "any"}:${nextRole}:${scope}:all`
      }
      replaceUrl(next)
    },
    [dirty, replaceUrl, scope]
  )

  const leaseStatus = !task
    ? "unclaimed"
    : lastResult?.status === "unknown"
      ? "active"
      : activeLease
        ? "active"
        : "unclaimed"

  const leaseLabel = !role.canExecute
    ? "只能查看"
    : activeLease
      ? "你正在处理这一条"
      : claimMutation.isPending
        ? "正在接手…"
        : "点「确认」即可接手"

  /** 只读角色看到的一句话：谁在处理、什么时候要交 */
  const readOnlyNote = task
    ? `你只能查看。这条由 ${task.responsibleLabel} 处理，${
        task.overdue ? `原定 ${task.dueLabel}，已超期` : `预计 ${task.dueLabel} 前完成`
      }。`
    : "你只能查看这些任务的进度。"

  const activeTypeSlug = typeParamValue(operationTypes)
  const sourceReturnHref =
    returnTo ??
    (fromWorkspace === "W05" && task
      ? `/sales/orders/${task.source.salesOrderId}`
      : fromWorkspace === "W08" && task?.source.purchaseOrderId
        ? `/procurement/orders`
        : fromWorkspace === "W10"
          ? `/inventory`
          : undefined)

  if (queueQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title={header.label} description="正在加载队列…" />
        <div className="h-20 animate-pulse rounded-2xl bg-muted" />
        <div className="grid gap-4 xl:grid-cols-[minmax(16rem,1fr)_minmax(0,2fr)]">
          <div className="h-80 animate-pulse rounded-2xl bg-muted" />
          <div className="h-96 animate-pulse rounded-2xl bg-muted" />
        </div>
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:p-5">
      <PageHeader
        title={header.label}
        description={header.description}
        breadcrumbs={[
          ...(header.group
            ? [
                {
                  id: "group",
                  label: header.group.label,
                  href: header.group.href,
                },
              ]
            : []),
          { id: "fulfillment", label: header.label, current: true },
        ]}
        metadata={
          <div className="flex flex-wrap items-center gap-3">
            <DataFreshness
              updatedAt="刚刚"
              dateTime={context?.snapshotUpdatedAt}
              state="fresh"
              label={freshnessText.dataUpdatedAt}
            />
            <span className="text-xs text-muted-foreground" aria-live="polite">
              {context?.filterSummary ?? "仅我的"} · 待处理{" "}
              {context?.total ?? 0}
            </span>
          </div>
        }
      />

      {sourceReturnHref ? (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-2xl border border-border bg-card px-4 py-2.5 text-sm">
          <span className="text-muted-foreground">
            从
            {fromWorkspace
              ? workspaceLabel(fromWorkspace as WorkspaceId)
              : "关联页面"}
            进来的
            {task
              ? ` · 已经定位到 ${task.source.salesOrderNo}${
                  task.source.purchaseNo ? ` / ${task.source.purchaseNo}` : ""
                }`
              : ""}
            。返回时会回到原来的位置。
          </span>
          <Button
            type="button"
            size="sm"
            variant="outline"
            render={<Link href={sourceReturnHref} />}
          >
            返回来源
          </Button>
        </div>
      ) : null}


      <div className="flex flex-wrap items-center gap-2">
        <ToggleGroup
          value={[activeTypeSlug === "all" ? "all" : activeTypeSlug]}
          onValueChange={(values) => {
            const next = values[0]
            if (!next) return
            if (next === "all") setTypeFilter("all")
            else {
              const t = SLUG_TO_TYPE[next]
              if (t) setTypeFilter(t)
            }
          }}
          variant="outline"
          size="sm"
          spacing={0}
          className="w-fit flex-wrap"
        >
          <ToggleGroupItem value="all">全部</ToggleGroupItem>
          {visibleTypes.map((t) => (
            <ToggleGroupItem key={t} value={TYPE_SLUG[t]}>
              {OPERATION_TYPE_SHORT[t]}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>

      </div>

      <FulfillmentQueueToolbar
        q={q}
        warehouseId={warehouseId}
        warehouseOptions={context?.warehouseOptions ?? []}
        due={due}
        gate={gate}
        salesOrderId={salesOrderId}
        purchaseOrderId={purchaseOrderId}
        autoNext={autoNext}
        total={context?.total ?? tasks.length}
        scope={scope}
        showScope={Boolean(role.userLabel)}
        showAutoNext={role.canExecute}
        roleValue={role.value}
        roleOptions={ROLE_OPTIONS}
        onPatch={handleRolePatch}
        onAutoNextChange={(next) => {
          setSessionAutoNext(next)
          replaceUrl({ autoNext: next ? "1" : "0" })
        }}
        devSimulation={
          DEV_SIMULATION_ENABLED ? (
            <label className="flex items-center gap-2 text-xs text-muted-foreground">
              <input
                type="checkbox"
                className="size-3.5"
                checked={forceUnknownOnce}
                onChange={(e) => setForceUnknownOnce(e.target.checked)}
              />
              模拟：提交结果不确定（仅开发环境）
            </label>
          ) : null
        }
      />

      {lastResult ? (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
          <FormalActionResult
            status={lastResult.status === "failed" ? "blocked" : lastResult.status}
            title={lastResult.title}
            description={
              lastResult.outcome?.kind === "POSTED" &&
              lastResult.outcome.acceptanceRequired ? (
                <span className="block space-y-1">
                  <span className="block">{lastResult.description}</span>
                  <span className="block text-muted-foreground">
                    {NOT_ACCEPTANCE_NOTICE}
                  </span>
                </span>
              ) : (
                lastResult.description
              )
            }
            reference={lastResult.reference}
            facts={
              lastResult.outcome?.kind === "POSTED"
                ? buildPostedFacts(lastResult.outcome)
                : lastResult.outcome?.kind === "DEFERRED"
                  ? [
                      {
                        label: "跳过原因",
                        value: DEFER_REASON_LABEL[lastResult.outcome.reasonCode],
                      },
                      {
                        label: "任务状态",
                        value: lastResult.outcome.workItemStatus,
                      },
                      {
                        label: "处理状态",
                        value: "这次先放着，之后还会回到你的列表",
                      },
                    ]
                  : undefined
            }
            actions={
              <div className="flex flex-wrap gap-2">
                {lastResult.status === "unknown" ? (
                  <>
                    <Button
                      type="button"
                      size="sm"
                      onClick={() => void handleResolveUnknown(false)}
                    >
                      查询最终结果
                    </Button>
                    {DEV_SIMULATION_ENABLED ? (
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => void handleResolveUnknown(true)}
                      >
                        模拟结算并确认（仅开发环境）
                      </Button>
                    ) : null}
                  </>
                ) : null}
                {lastResult.outcome?.kind === "POSTED" &&
                lastResult.outcome.acceptanceRequired ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    render={
                      <Link
                        href={`/sales/orders/${lastResult.outcome.salesOrderId}?section=acceptance&from=W09&returnTo=${encodeURIComponent(
                          `${pathname}?${searchParams.toString()}`
                        )}`}
                      />
                    }
                  >
                    去登记客户验收
                    <ArrowRightIcon data-icon="inline-end" />
                  </Button>
                ) : null}
                {lastResult.stayOnItem === false || lastResult.status === "blocked" ? null : (
                  <Button
                    type="button"
                    size="sm"
                    onClick={() => {
                      const next =
                        (lastResult.outcome &&
                          "nextWorkItemId" in lastResult.outcome &&
                          lastResult.outcome.nextWorkItemId) ||
                        neighborId(1) ||
                        tasks[0]?.workItemId
                      goToWorkItem(next)
                    }}
                  >
                    下一条
                  </Button>
                )}
              </div>
            }
          />
        </div>
      ) : null}

      {actionError ? (
        <Alert variant="destructive">
          <AlertTitle>没有生效</AlertTitle>
          <AlertDescription>{actionError}</AlertDescription>
        </Alert>
      ) : null}

      {completed ? (
        <BusinessEmptyState
          kind="no-tasks"
          title={
            operationTypes?.length === 1
              ? OPERATION_CLEARED_LABEL[operationTypes[0]]
              : "这批活都干完了"
          }
          description="可以换个类型看看，或者清掉筛选、回工作台。"
          action={
            <div className="flex flex-wrap gap-2">
              <Button type="button" variant="outline" onClick={clearAllFilters}>
                清除全部筛选
              </Button>
              <Button render={<Link href="/workspace" />}>回今日工作台</Button>
            </div>
          }
        />
      ) : task && draft ? (
        <div className="grid min-h-[28rem] min-w-0 gap-4 xl:grid-cols-[minmax(15rem,0.9fr)_minmax(0,2.1fr)]">
          <FulfillmentQueueList
            tasks={tasks}
            currentIndex={currentIndex}
            position={context?.position ?? currentIndex + 1}
            total={context?.total ?? tasks.length}
            onSelect={(workItemId) => {
              if (dirty && workItemId !== task.workItemId) {
                setActionError("有未保存修改，请先保存或放弃后再切换")
                return
              }
              goToWorkItem(workItemId)
            }}
          />

          <div className="min-w-0 space-y-3">
            <SequentialProcessBar
              current={context?.position ?? currentIndex + 1}
              total={context?.total ?? tasks.length}
              leaseStatus={role.canExecute ? leaseStatus : "active"}
              leaseStatusLabel={leaseLabel}
              showProcess={role.canExecute}
              processLabel={OPERATION_ACTION_LABEL[task.operationType]}
              // 没有独立的「并下一项」路径：两个 handler 同义，第二个按钮名不副实
              showProcessNext={false}
              processDisabled={formalPending || !canPost}
              statusExtras={
                task.gate.state !== "NOT_APPLICABLE" ? (
                  <PrepaymentGate
                    id="prepayment-gate"
                    presentation="badge"
                    copy={{
                      title: "先款条件",
                      description:
                        "只认已经到账并核销过的货款，付款申请和附件不算。",
                      allowedBadge: "可以收货",
                      blockedBadge: "暂时不能收货",
                      amountTerm: "至少要付",
                      ratioTerm: "至少要付比例",
                      allocatedTerm: "已经付了",
                      gapTerm: "还差",
                      updatedTerm: "算到什么时候",
                      allowedTitle: "货款已到，可以收货",
                      blockedTitle: "先款未到，暂时不能收货",
                      allowedBody: "货款已经够了，这一单可以继续。",
                      blockedBody:
                        "差额补齐之前，入库、直发、电子交付和服务都确认不了。",
                    }}
                    condition={{
                      kind: "amount",
                      required: task.gate.requiredAmount ?? "—",
                      description: task.gate.message,
                    }}
                    allocated={task.gate.effectivePaidAmount ?? "—"}
                    gap={
                      task.gate.state === "BLOCKED" &&
                      task.gate.requiredAmount &&
                      task.gate.effectivePaidAmount
                        ? String(
                            Math.max(
                              0,
                              Number(task.gate.requiredAmount) -
                                Number(task.gate.effectivePaidAmount)
                            )
                          )
                        : "0"
                    }
                    updatedAt={{
                      label: "刚刚",
                      dateTime: context?.snapshotUpdatedAt ?? "",
                    }}
                    allowed={task.gate.state === "SATISFIED"}
                    paymentAction={
                      task.gate.state === "BLOCKED" ? (
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          render={
                            <Link
                              href={`/finance/supplier-accounts?from=W09&purchaseOrderId=${task.source.purchaseOrderId ?? ""}&returnTo=${encodeURIComponent(
                                `${pathname}?${searchParams.toString()}`
                              )}`}
                            />
                          }
                        >
                          去登记付款
                        </Button>
                      ) : undefined
                    }
                  />
                ) : (
                  <BusinessStatusBadge
                    context="list"
                    id="prepayment-gate"
                    tone="neutral"
                    label={
                      task.operationType === "WAREHOUSE_SHIP"
                        ? "发货条件：无先款要求"
                        : "无先款要求"
                    }
                    description={task.gate.message}
                  />
                )
              }
              onBack={() => {
                if (sourceReturnHref) router.push(sourceReturnHref)
                else router.push("/workspace")
              }}
              onProcess={() => setConfirmOpen(true)}
              onProcessNext={() => setConfirmOpen(true)}
              onReclaim={() => {
                void ensureLease().catch((e) =>
                  setActionError(e instanceof Error ? e.message : "没能接手这一条")
                )
              }}
            />

            <button
              type="button"
              onClick={() => setShortcutsOpen((v) => !v)}
              aria-expanded={shortcutsOpen}
              className="self-start text-xs text-muted-foreground hover:text-foreground"
            >
              {shortcutsOpen
                ? `快捷键：J / K 上下条${
                    role.canExecute
                      ? " · Ctrl+S 保存 · Ctrl+Enter 确认"
                      : ""
                  } · 再按 ? 收起`
                : "按 ? 看快捷键"}
            </button>

            <Card size="sm">
              <CardHeader className="border-b">
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div>
                    <CardTitle
                      ref={headingRef}
                      tabIndex={-1}
                      aria-live="polite"
                      className="outline-none"
                    >
                      {OPERATION_TYPE_LABEL[task.operationType]} ·{" "}
                      {task.source.salesOrderNo}
                    </CardTitle>
                    <CardDescription>
                      {task.source.customerLabel}
                      {task.source.purchaseNo
                        ? ` · 采购 ${task.source.purchaseNo}`
                        : ""}
                      {task.source.supplierLabel
                        ? ` · ${task.source.supplierLabel}`
                        : ""}

                    </CardDescription>
                  </div>
                  <BusinessStatusBadge
                    context="list"
                    label={task.held ? "已跳过" : task.statusLabel}
                    tone={task.held ? "warning" : task.statusTone}
                  />
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                <section aria-label="来源上下文">
                  <dl className="grid gap-px overflow-hidden rounded-lg border border-grid bg-grid sm:grid-cols-2 lg:grid-cols-3">
                    {[
                      {
                        label: "销售单",
                        value: task.source.salesOrderNo,
                        href: `/sales/orders/${task.source.salesOrderId}?from=W09&returnTo=${encodeURIComponent(
                          `${pathname}?${searchParams.toString()}`
                        )}`,
                      },
                      {
                        label: "采购单",
                        value: task.source.purchaseNo ?? "—",
                      },
                      {
                        label: "仓库",
                        value: task.source.warehouseLabel ?? "不涉及仓库",
                      },
                      {
                        label: "还剩多少",
                        value: task.lines
                          .map(
                            (l) =>
                              `${l.itemName} ${l.remainingQuantity}${l.unitCode}`
                          )
                          .join("；"),
                        numeric: true,
                      },
                      {
                        label: "供应商",
                        value: task.source.supplierLabel ?? "—",
                      },
                      {
                        label: "客户",
                        value: task.source.customerLabel,
                      },
                    ].map((field) => (
                      <div key={field.label} className="bg-card p-3">
                        <dt className="text-xs text-muted-foreground">
                          {field.label}
                        </dt>
                        <dd
                          className={cn(
                            "mt-1 font-medium",
                            field.numeric && "num"
                          )}
                        >
                          {field.href && field.value !== "—" ? (
                            <Link
                              href={field.href}
                              className="text-primary underline-offset-4 hover:underline"
                            >
                              {field.value}
                            </Link>
                          ) : (
                            field.value
                          )}
                        </dd>
                      </div>
                    ))}
                  </dl>
                </section>

                <Separator />

                <FulfillmentDraftForm
                  task={task}
                  draft={draft}
                  onChange={updateDraft}
                  disabled={
                    formalPending ||
                    !role.canExecute ||
                    lastResult?.status === "unknown"
                  }
                />

                {validationIssues.length > 0 ? (
                  <ValidationSummary
                    title="还差这些没填好"
                    issues={validationIssues}
                  />
                ) : null}

                {saveMessage ? (
                  <p className="text-xs text-muted-foreground">{saveMessage}</p>
                ) : null}

                {role.canExecute ? (
                  <div className="sticky bottom-0 flex flex-wrap justify-end gap-2 border-t border-border bg-card/95 py-3 backdrop-blur">
                    <Button
                      type="button"
                      variant="outline"
                      disabled={formalPending}
                      onClick={() => setDeferOpen(true)}
                    >
                      <PauseIcon data-icon="inline-start" />
                      先跳过
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      disabled={formalPending || !dirty}
                      onClick={handleDiscard}
                    >
                      <Undo2Icon data-icon="inline-start" />
                      放弃修改
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      disabled={formalPending || !dirty}
                      onClick={() => void handleSave()}
                    >
                      <SaveIcon data-icon="inline-start" />
                      保存草稿
                    </Button>
                    <Button
                      type="button"
                      disabled={formalPending || !canPost}
                      onClick={() => setConfirmOpen(true)}
                    >
                      <CircleCheckIcon data-icon="inline-start" />
                      {autoNext
                        ? `${OPERATION_ACTION_LABEL[task.operationType]}并下一条`
                        : OPERATION_ACTION_LABEL[task.operationType]}
                    </Button>
                  </div>
                ) : (
                  /* 只读角色：与其摆一排点不动的按钮，不如说清楚谁在处理 */
                  <div className="sticky bottom-0 flex flex-wrap items-center justify-between gap-2 border-t border-border bg-card/95 py-3 backdrop-blur">
                    <p className="flex items-center gap-2 text-sm text-muted-foreground">
                      <EyeIcon className="size-4 shrink-0" aria-hidden="true" />
                      {readOnlyNote}
                    </p>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      render={
                        <Link
                          href={`/sales/orders/${task.source.salesOrderId}?from=W09&returnTo=${encodeURIComponent(
                            `${pathname}?${searchParams.toString()}`
                          )}`}
                        />
                      }
                    >
                      打开销售单
                      <ArrowRightIcon data-icon="inline-end" />
                    </Button>
                  </div>
                )}
              </CardContent>
            </Card>
          </div>
        </div>
      ) : view?.emptyReason === "NO_PERMISSION" ? (
        <BusinessEmptyState
          kind="no-scope"
          title="你没有这类任务的权限"
          description={`${role.label}能处理的是：${visibleTypes
            .map((t) => OPERATION_TYPE_SHORT[t])
            .join("、")}。`}
          action={
            <Button type="button" variant="outline" onClick={clearAllFilters}>
              回到我能处理的
            </Button>
          }
        />
      ) : (
        <BusinessEmptyState
          kind="filter"
          title="没有符合条件的任务"
          description={context?.filterSummary ?? "换个类型或单号试试"}
          action={
            <Button type="button" variant="outline" onClick={clearAllFilters}>
              清除全部筛选
            </Button>
          }
        />
      )}

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={task ? OPERATION_CONFIRM_TITLE[task.operationType] : "确认？"}
        description="没确认成功之前，库存和留货都不会动。"
        actionLabel={task ? OPERATION_ACTION_LABEL[task.operationType] : "确认"}
        confirmLabel={task ? OPERATION_ACTION_LABEL[task.operationType] : "确认"}
        fromStatus={{ label: "待确认", tone: "warning" }}
        toStatus={{
          label: task ? OPERATION_DONE_LABEL[task.operationType] : "已完成",
          tone: "success",
        }}
        lockedFields={["来源单据版本", "任务类型", "对应的销售单和留货"]}
        effects={task && draft ? impactPreview(task, draft) : []}
        irreversibleEffects={[CORRECTION_NOTICE]}
        nextDepartment="做完之后由销售登记客户验收"
        pending={postMutation.isPending}
        onConfirm={async () => {
          await handlePost()
        }}
      />

      <FulfillmentDeferDialog
        open={deferOpen}
        onOpenChange={setDeferOpen}
        pending={deferMutation.isPending}
        onSubmit={handleDefer}
      />
    </div>
  )
}
