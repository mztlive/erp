"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ArrowRightIcon,
  CircleCheckIcon,
  PauseIcon,
  SaveIcon,
} from "lucide-react"
import { z } from "zod"

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
  WorkTaskItem,
  type ValidationIssue,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
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
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import type {
  DeferOutcome,
  DeferReasonCode,
  FulfillmentDraft,
  FulfillmentFormalOutcome,
  FulfillmentOperationType,
  FulfillmentTask,
} from "@/features/fulfillment-operations/types"
import {
  CORRECTION_NOTICE,
  DEFER_REASON_LABEL,
  FACT_TYPE_LABEL,
  NOT_ACCEPTANCE_NOTICE,
  OPERATION_TYPE_LABEL,
  OPERATION_TYPE_SHORT,
  RESULT_LABEL,
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
import { cn } from "@/lib/utils"

type SessionLease = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expiresAt: string
}

type ResultState =
  | {
      status: "succeeded" | "blocked" | "unknown"
      title: string
      description: string
      reference?: string
      outcome?: FulfillmentFormalOutcome | DeferOutcome
      pendingIdempotencyKey?: string
      stayOnItem?: boolean
    }
  | null

const ALL_TYPES: FulfillmentOperationType[] = [
  "RECEIPT",
  "WAREHOUSE_SHIP",
  "SUPPLIER_DIRECT",
  "ELECTRONIC",
  "SERVICE",
]

const deferSchema = z.object({
  reasonCode: z.enum([
    "WAITING_SUPPLIER",
    "WAITING_WAREHOUSE",
    "WAITING_PAYMENT",
    "NEED_CLARIFICATION",
    "OTHER",
  ]),
  reasonNote: z.string(),
})

function parseTypeParam(
  raw: string | null
): FulfillmentOperationType[] | undefined {
  if (!raw || raw === "all") return undefined
  const parts = raw.split(",").map((p) => p.trim()).filter(Boolean)
  const types = parts
    .map((p) => SLUG_TO_TYPE[p] ?? (ALL_TYPES.includes(p as FulfillmentOperationType) ? (p as FulfillmentOperationType) : null))
    .filter((t): t is FulfillmentOperationType => t != null)
  return types.length > 0 ? types : undefined
}

function typeParamValue(types: FulfillmentOperationType[] | undefined): string {
  if (!types || types.length === 0) return "all"
  if (types.length === 1) return TYPE_SLUG[types[0]]
  return types.map((t) => TYPE_SLUG[t]).join(",")
}

function cloneDraft(draft: FulfillmentDraft): FulfillmentDraft {
  return structuredClone(draft)
}

function clientValidation(
  task: FulfillmentTask,
  draft: FulfillmentDraft
): ValidationIssue[] {
  const issues: ValidationIssue[] = []
  if (draft.type !== task.operationType) {
    issues.push({
      id: "type-mismatch",
      label: "作业类型",
      message: "草稿类型与当前作业不一致",
    })
    return issues
  }
  if (task.gate.state === "BLOCKED" && draft.type !== "WAREHOUSE_SHIP") {
    issues.push({
      id: "gate",
      label: "付款门禁",
      message: task.gate.message,
      targetId: "prepayment-gate",
    })
  }

  if (draft.type === "RECEIPT") {
    draft.lines.forEach((line, i) => {
      const recv = Number(line.receivedQuantity)
      const qual = Number(line.qualifiedQuantity)
      const rej = Number(line.rejectedQuantity)
      if (!(recv > 0)) {
        issues.push({
          id: `recv-${i}`,
          label: "到货数量",
          message: "必须大于 0",
          targetId: `receipt-recv-${i}`,
        })
      }
      if (qual + rej > recv + 1e-9) {
        issues.push({
          id: `qty-sum-${i}`,
          label: "质量数量",
          message: "合格 + 不合格不得超过到货",
          targetId: `receipt-qual-${i}`,
        })
      }
    })
  }
  if (draft.type === "WAREHOUSE_SHIP") {
    if (!draft.carrier.trim()) {
      issues.push({
        id: "carrier",
        label: "承运方",
        message: "必填",
        targetId: "ship-carrier",
      })
    }
    if (!draft.trackingNo.trim()) {
      issues.push({
        id: "tracking",
        label: "物流单号",
        message: "必填",
        targetId: "ship-tracking",
      })
    }
    draft.lines.forEach((line, i) => {
      const qty = Number(line.quantity)
      const src = task.lines.find((l) => l.salesOrderLineId === line.salesOrderLineId)
      const cap = Number(src?.reservedQuantity ?? src?.remainingQuantity ?? 0)
      if (!(qty > 0)) {
        issues.push({
          id: `ship-qty-${i}`,
          label: "发货数量",
          message: "必须大于 0",
          targetId: `ship-qty-${i}`,
        })
      } else if (qty > cap + 1e-9) {
        issues.push({
          id: `ship-cap-${i}`,
          label: "发货数量",
          message: `不得超过有效预占 ${cap}`,
          targetId: `ship-qty-${i}`,
        })
      }
      if (!line.stockReservationId) {
        issues.push({
          id: `ship-rsv-${i}`,
          label: "销售预占",
          message: "仓发必须引用预占",
        })
      }
    })
  }
  if (draft.type === "SUPPLIER_DIRECT") {
    if (!draft.carrier.trim()) {
      issues.push({
        id: "d-carrier",
        label: "承运方",
        message: "必填",
        targetId: "direct-carrier",
      })
    }
    if (!draft.trackingNo.trim()) {
      issues.push({
        id: "d-tracking",
        label: "物流单号",
        message: "必填",
        targetId: "direct-tracking",
      })
    }
  }
  if (draft.type === "SERVICE") {
    if (draft.endedAt && draft.startedAt && draft.endedAt < draft.startedAt) {
      issues.push({
        id: "svc-time",
        label: "服务时间",
        message: "结束不得早于开始",
        targetId: "service-ended",
      })
    }
    if (!draft.completionNote.trim() || draft.completionNote.trim().length < 4) {
      issues.push({
        id: "svc-note",
        label: "完成说明",
        message: "至少 4 个字",
        targetId: "service-note",
      })
    }
  }
  return issues
}

function buildPostedFacts(outcome: FulfillmentFormalOutcome) {
  const facts: { label: string; value: string }[] = [
    {
      label: "记录类型",
      value: FACT_TYPE_LABEL[outcome.factType],
    },
    { label: "记录编号", value: outcome.factNo },
    { label: "当前状态", value: outcome.formalStatus },
    { label: "库存/预占影响", value: outcome.inventoryImpactSummary },
  ]
  if (outcome.inventoryDelta.length > 0) {
    facts.push({
      label: "库存流水",
      value: outcome.inventoryDelta
        .map(
          (d) =>
            `${d.warehouseLabel} · ${d.skuLabel} ${d.direction === "INCREASE" ? "+" : "−"}${d.quantity}`
        )
        .join("；"),
    })
  }
  if (outcome.reservationDelta.length > 0) {
    facts.push({
      label: "预占变化",
      value: outcome.reservationDelta
        .map(
          (d) =>
            `${d.action === "CREATE" ? "建立" : "消耗"} ${d.reservationId} × ${d.quantity}`
        )
        .join("；"),
    })
  }
  facts.push({
    label: "剩余待处理",
    value:
      outcome.remainingByLine
        .map((l) => `${l.itemName} ${l.quantity}`)
        .join("；") || "0",
  })
  facts.push({
    label: "验收下一步",
    value: outcome.acceptanceNextStep,
  })
  return facts
}

function impactPreview(task: FulfillmentTask, draft: FulfillmentDraft): string[] {
  if (draft.type === "RECEIPT") {
    const qual = draft.lines.reduce(
      (s, l) => s + Number(l.qualifiedQuantity || 0),
      0
    )
    const rej = draft.lines.reduce(
      (s, l) => s + Number(l.rejectedQuantity || 0),
      0
    )
    return [
      `合格 ${qual} 将增加库存并沿采购销售分配建立预占`,
      `不合格 ${rej} 不入库、不建立预占`,
      "不触发客户验收",
    ]
  }
  if (draft.type === "WAREHOUSE_SHIP") {
    const qty = draft.lines.reduce((s, l) => s + Number(l.quantity || 0), 0)
    return [
      `发货 ${qty}：消耗本销售明细有效预占并减少自有库存`,
      "物流签收 ≠ 客户验收；下一步在客户验收登记验收",
    ]
  }
  if (draft.type === "SUPPLIER_DIRECT") {
    return [
      "形成供应商直发记录",
      "不影响自有库存流水",
      "下一步：销售在客户验收登记",
    ]
  }
  if (draft.type === "ELECTRONIC") {
    return [
      `电子交付结果：${RESULT_LABEL[draft.result]}`,
      "不改变自有库存；失败不可覆盖，重做新建",
      "成功后可进入客户验收",
    ]
  }
  return [
    `服务结果：${RESULT_LABEL[draft.result]}`,
    "形成服务履约记录，不影响库存",
    "成功后可进入客户验收",
  ]
}

export function FulfillmentOperationsPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const scope: "mine" | "role_pool" =
    searchParams.get("scope") === "role_pool" ? "role_pool" : "mine"
  const operationTypes = parseTypeParam(searchParams.get("type"))
  const warehouseId = searchParams.get("warehouseId") ?? undefined
  const q = searchParams.get("q") ?? undefined
  const dueParam = searchParams.get("due")
  const due: "active" | "today" | "overdue" | undefined =
    dueParam === "today" || dueParam === "overdue" || dueParam === "active"
      ? dueParam
      : undefined
  const gateParam = searchParams.get("gate")
  const gate: "blocked" | "satisfied" | undefined =
    gateParam === "blocked" || gateParam === "satisfied" ? gateParam : undefined
  const salesOrderId = searchParams.get("salesOrderId") ?? undefined
  const purchaseOrderId = searchParams.get("purchaseOrderId") ?? undefined
  const currentWorkItemId =
    searchParams.get("currentWorkItemId") ?? undefined
  const queueContextId =
    searchParams.get("queueContextId") ??
    `queue:fulfillment:demo:${scope}`
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
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [saveMessage, setSaveMessage] = React.useState<string | null>(null)
  const headingRef = React.useRef<HTMLHeadingElement>(null)
  const resultRef = React.useRef<HTMLDivElement>(null)
  const leaseRef = React.useRef<SessionLease | null>(null)
  const [leaseEpoch, setLeaseEpoch] = React.useState(0)
  const idempotencyRef = React.useRef<{ post?: string; defer?: string }>({})

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
    idempotencyRef.current = {}
  }, [task?.workItemId, task?.editVersion])

  React.useEffect(() => {
    if (queueQuery.isPending || !view) return
    const hasScope = searchParams.has("scope")
    const hasItem = searchParams.has("currentWorkItemId")
    const hasCtx = searchParams.has("queueContextId")
    if (hasScope && hasCtx && (hasItem || tasks.length === 0)) return
    const params = new URLSearchParams(searchParams.toString())
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
    scope,
    queueContextId,
    task,
    tasks.length,
    pathname,
    router,
  ])

  React.useEffect(() => {
    if (!task) return
    if (leaseRef.current?.workItemId === task.workItemId) return
    if (claimMutation.isPending) return
    let cancelled = false
    void claimMutation
      .mutateAsync(task.workItemId)
      .then((lease) => {
        if (cancelled) return
        leaseRef.current = {
          workItemId: lease.workItemId,
          claimToken: lease.claimToken,
          leaseVersion: lease.leaseVersion,
          expiresAt: lease.expiresAt,
        }
        setLeaseEpoch((n) => n + 1)
      })
      .catch(() => {
        /* 未领取 */
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅任务切换时领取
  }, [task?.workItemId])

  React.useEffect(() => {
    if (lastResult) {
      resultRef.current?.focus()
    } else if (task) {
      headingRef.current?.focus()
    }
  }, [task?.workItemId, lastResult?.status])

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
    Boolean(task && draft) &&
    validationIssues.length === 0 &&
    !(task?.gate.state === "BLOCKED" && task.operationType !== "WAREHOUSE_SHIP") &&
    !(task?.actionBlockers.some((b) => b.action === "POST"))

  const ensureLease = React.useCallback(async () => {
    if (!task) throw new Error("无当前任务")
    if (
      leaseRef.current?.workItemId === task.workItemId &&
      leaseRef.current.claimToken
    ) {
      return leaseRef.current
    }
    const lease = await claimMutation.mutateAsync(task.workItemId)
    const session: SessionLease = {
      workItemId: lease.workItemId,
      claimToken: lease.claimToken,
      leaseVersion: lease.leaseVersion,
      expiresAt: lease.expiresAt,
    }
    leaseRef.current = session
    setLeaseEpoch((n) => n + 1)
    return session
  }, [claimMutation, task])

  const updateDraft = React.useCallback((next: FulfillmentDraft) => {
    setDraft(next)
    setDirty(true)
  }, [])

  const handleSave = React.useCallback(async () => {
    if (!task || !draft) return
    try {
      const lease = await ensureLease()
      const result = await saveMutation.mutateAsync({
        workItemId: task.workItemId,
        expectedEditVersion: task.editVersion,
        claimToken: lease.claimToken,
        leaseVersion: lease.leaseVersion,
        draft,
        idempotencyKey: `save_${task.workItemId}_${Date.now()}`,
      })
      setDirty(false)
      setSaveMessage(`已保存 · 编辑版本 ${result.editVersion}`)
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
      const lease = await ensureLease()
      if (!idempotencyRef.current.post) {
        idempotencyRef.current.post = `post_${task.workItemId}_${crypto.randomUUID()}`
      }
      const nextId = neighborId(1)
      const response = await postMutation.mutateAsync({
        workItemId: task.workItemId,
        claimToken: lease.claimToken,
        leaseVersion: lease.leaseVersion,
        expectedSubjectHash: task.subjectHash,
        expectedSourceVersion: task.sourceVersion,
        expectedEditVersion: task.editVersion,
        draft,
        idempotencyKey: idempotencyRef.current.post,
        nextWorkItemId: nextId,
        forceUnknown: forceUnknownOnce,
      })
      setForceUnknownOnce(false)
      setConfirmOpen(false)

      if (response.status === "unknown") {
        setLastResult({
          status: "unknown",
          title: "处理结果待确认",
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
        title: `${OPERATION_TYPE_LABEL[response.outcome.operationType]}已过账`,
        description: autoNext
          ? "强类型记录已写入会话 mock；将打开同筛选下一项。"
          : "强类型记录已写入。可核对库存/预占影响后再继续。",
        reference: response.outcome.factNo,
        outcome: response.outcome,
        stayOnItem: !autoNext,
      })
      if (autoNext) {
        advanceIfNeeded(true, response.outcome.nextWorkItemId)
      }
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "过账失败")
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

  const deferForm = useAppForm({
    defaultValues: {
      reasonCode: "WAITING_WAREHOUSE" as DeferReasonCode,
      reasonNote: "",
    },
    validators: { onChange: deferSchema },
    onSubmit: async ({ value }) => {
      if (!task) return
      setActionError(null)
      try {
        if (dirty) await handleSave()
        const lease = await ensureLease()
        if (!idempotencyRef.current.defer) {
          idempotencyRef.current.defer = `defer_${task.workItemId}_${crypto.randomUUID()}`
        }
        const nextId = neighborId(1)
        const response = await deferMutation.mutateAsync({
          workItemId: task.workItemId,
          claimToken: lease.claimToken,
          leaseVersion: lease.leaseVersion,
          queueContextId,
          reasonCode: value.reasonCode as DeferReasonCode,
          reasonNote: value.reasonNote?.trim() || undefined,
          nextWorkItemId: nextId,
          idempotencyKey: idempotencyRef.current.defer,
        })
        setDeferOpen(false)
        if (response.status !== "succeeded" || response.outcome.kind !== "DEFERRED") {
          if (response.status === "failed") setActionError(response.message)
          return
        }
        leaseRef.current = null
        setLeaseEpoch((n) => n + 1)
        setLastResult({
          status: "blocked",
          title: "当前项已暂挂",
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
        setActionError(error instanceof Error ? error.message : "暂挂失败")
      }
    },
  })

  const handleResolveUnknown = React.useCallback(
    async (settle: boolean) => {
      if (!lastResult?.pendingIdempotencyKey || !task || !draft) return
      const lease = leaseRef.current
      const response = await resolveUnknownMutation.mutateAsync({
        idempotencyKey: lastResult.pendingIdempotencyKey,
        settle,
        settlePayload:
          settle && lease
            ? {
                workItemId: task.workItemId,
                claimToken: lease.claimToken,
                leaseVersion: lease.leaseVersion,
                expectedSubjectHash: task.subjectHash,
                expectedSourceVersion: task.sourceVersion,
                expectedEditVersion: task.editVersion,
                draft,
                idempotencyKey: lastResult.pendingIdempotencyKey,
                nextWorkItemId: neighborId(1),
              }
            : undefined,
      })
      if (response.status === "unknown") {
        setLastResult({
          status: "unknown",
          title: "处理结果仍待确认",
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
          title: "查询确认：履约已过账",
          description: "同一任务号返回同一业务记录，未重复改库存/预占。",
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
      lastResult?.pendingIdempotencyKey,
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
        void handleSave()
        return
      }
      if (
        (event.metaKey || event.ctrlKey) &&
        event.key === "Enter" &&
        !inField
      ) {
        event.preventDefault()
        if (canPost && activeLease) setConfirmOpen(true)
        return
      }
      if (inField) return
      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault()
        if (dirty) {
          setActionError("有未保存修改，请先保存或放弃后再切换")
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
  }, [activeLease, canPost, dirty, goToWorkItem, handleSave, neighborId])

  const setTypeFilter = React.useCallback(
    (next: FulfillmentOperationType | "all") => {
      if (dirty) {
        setActionError("有未保存修改，请先保存或放弃后再切换类型")
        return
      }
      setLastResult(null)
      replaceUrl({
        type: next === "all" ? null : TYPE_SLUG[next],
        currentWorkItemId: null,
        queueContextId: `queue:fulfillment:demo:${scope}:${next === "all" ? "all" : TYPE_SLUG[next]}`,
      })
    },
    [dirty, replaceUrl, scope]
  )

  const formalPending =
    postMutation.isPending ||
    deferMutation.isPending ||
    claimMutation.isPending

  const leaseStatus = !task
    ? "unclaimed"
    : lastResult?.status === "unknown"
      ? "active"
      : activeLease
        ? "active"
        : "unclaimed"

  const leaseLabel = activeLease
    ? "已领取 · 处理中"
    : claimMutation.isPending
      ? "正在取得处理权…"
      : "待领取"

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
        <PageHeader title="履约作业" description="正在加载队列…" />
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
        title="履约作业"
        breadcrumbs={[
          {
            id: "proc",
            label: "采购与履约",
            href: "/procurement/confirm",
          },
          { id: "fulfillment", label: "履约作业", current: true },
        ]}
        metadata={
          <div className="flex flex-wrap items-center gap-3">
            <DataFreshness
              updatedAt="刚刚"
              dateTime={
                context?.snapshotUpdatedAt ?? new Date().toISOString()
              }
              state="fresh"
              label="数据更新时间"
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
            自 {fromWorkspace ?? "关联工作面"} 打开
            {task
              ? ` · 已定位 ${task.source.salesOrderNo}${
                  task.source.purchaseNo ? ` / ${task.source.purchaseNo}` : ""
                }`
              : ""}
            ；返回将保留来源页签状态（returnTo）。
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

      <MetricStrip columns={5}>
        {metrics.map((m) => (
          <MetricFilterItem
            key={m.operationType}
            label={m.label}
            value={m.count}
            detail={OPERATION_TYPE_LABEL[m.operationType]}
            active={
              operationTypes?.length === 1 &&
              operationTypes[0] === m.operationType
            }
            onClick={() => {
              if (
                operationTypes?.length === 1 &&
                operationTypes[0] === m.operationType
              ) {
                setTypeFilter("all")
              } else {
                setTypeFilter(m.operationType)
              }
            }}
          />
        ))}
      </MetricStrip>

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
          {ALL_TYPES.map((t) => (
            <ToggleGroupItem key={t} value={TYPE_SLUG[t]}>
              {OPERATION_TYPE_SHORT[t]}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>

        <div className="ml-auto flex flex-wrap items-center gap-3 text-sm">
          <div className="flex items-center gap-2">
            <Label htmlFor="ff-auto-next" className="text-muted-foreground">
              自动下一项
            </Label>
            <Switch
              id="ff-auto-next"
              checked={autoNext}
              onCheckedChange={(next) => {
                setSessionAutoNext(next)
                replaceUrl({ autoNext: next ? "1" : "0" })
              }}
            />
          </div>
          <label className="flex items-center gap-2 text-xs text-muted-foreground">
            <input
              type="checkbox"
              className="size-3.5"
              checked={forceUnknownOnce}
              onChange={(e) => setForceUnknownOnce(e.target.checked)}
            />
            下次过账模拟结果未知
          </label>
        </div>
      </div>

      <Alert>
        <AlertTitle>边界说明</AlertTitle>
        <AlertDescription className="space-y-1">
          <p>{NOT_ACCEPTANCE_NOTICE}</p>
          <p className="text-muted-foreground">{CORRECTION_NOTICE}</p>
        </AlertDescription>
      </Alert>

      {lastResult ? (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
          <FormalActionResult
            status={lastResult.status}
            title={lastResult.title}
            description={lastResult.description}
            reference={lastResult.reference}
            facts={
              lastResult.outcome?.kind === "POSTED"
                ? buildPostedFacts(lastResult.outcome)
                : lastResult.outcome?.kind === "DEFERRED"
                  ? [
                      {
                        label: "暂挂原因",
                        value: DEFER_REASON_LABEL[lastResult.outcome.reasonCode],
                      },
                      {
                        label: "任务状态",
                        value: lastResult.outcome.workItemStatus,
                      },
                      {
                        label: "处理状态",
                        value: "本次已结束（未写暂停状态）",
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
                      variant="outline"
                      size="sm"
                      onClick={() => void handleResolveUnknown(false)}
                    >
                      查询最终结果
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      onClick={() => void handleResolveUnknown(true)}
                    >
                      演示结算并确认
                    </Button>
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
                        href={`/sales/orders/${lastResult.outcome.salesOrderId}?tab=acceptance&from=W09&returnTo=${encodeURIComponent(
                          `${pathname}?${searchParams.toString()}`
                        )}`}
                      />
                    }
                  >
                    去客户验收
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
                    继续下一项
                  </Button>
                )}
              </div>
            }
          />
        </div>
      ) : null}

      {actionError ? (
        <Alert variant="destructive">
          <AlertTitle>操作未生效</AlertTitle>
          <AlertDescription>{actionError}</AlertDescription>
        </Alert>
      ) : null}

      {completed ? (
        <BusinessEmptyState
          kind="no-tasks"
          title="本筛选作业已处理完"
          description="可切换类型分段、清除单号/对象筛选，或返回工作台。"
          action={
            <div className="flex flex-wrap gap-2">
              <Button type="button" variant="outline" onClick={() => setTypeFilter("all")}>
                查看全部类型
              </Button>
              <Button render={<Link href="/workspace" />}>返回今日工作台</Button>
            </div>
          }
        />
      ) : task && draft ? (
        <div className="grid min-h-[28rem] min-w-0 gap-4 xl:grid-cols-[minmax(15rem,0.9fr)_minmax(0,2.1fr)]">
          <Card size="sm" className="min-w-0 self-start">
            <CardHeader className="border-b">
              <CardTitle>作业队列</CardTitle>
              <CardDescription>
                第 {context?.position ?? currentIndex + 1}/{context?.total ?? tasks.length} ·
                同页五类共用队列语言
              </CardDescription>
            </CardHeader>
            <CardContent className="max-h-[min(36rem,70vh)] space-y-2 overflow-y-auto">
              {tasks.map((item, index) => (
                <button
                  key={item.workItemId}
                  type="button"
                  className={cn(
                    "w-full text-left",
                    index === currentIndex && "rounded-lg ring-2 ring-primary"
                  )}
                  onClick={() => {
                    if (dirty && item.workItemId !== task.workItemId) {
                      setActionError("有未保存修改，请先保存或放弃后再切换")
                      return
                    }
                    goToWorkItem(item.workItemId)
                  }}
                >
                  <WorkTaskItem
                    density="compact"
                    taskType={OPERATION_TYPE_LABEL[item.operationType]}
                    businessObject={`${item.source.salesOrderNo}${
                      item.source.purchaseNo
                        ? ` · ${item.source.purchaseNo}`
                        : ""
                    }`}
                    counterparty={item.source.customerLabel}
                    enteredAt={item.dueLabel}
                    enteredDateTime={item.dueAt}
                    dueAt={item.dueLabel}
                    dueDateTime={item.dueAt}
                    responsibleParty={item.responsibleLabel}
                    reason={item.summary}
                    impact={item.impact}
                    status={{
                      label: item.held
                        ? "已暂挂"
                        : item.overdue
                          ? "已超期"
                          : item.statusLabel,
                      tone: item.held
                        ? "warning"
                        : item.overdue
                          ? "destructive"
                          : item.statusTone,
                    }}
                  />
                  <div className="mt-1 flex flex-wrap gap-1 px-1 pb-1">
                    <Badge variant="outline" className="font-normal">
                      {OPERATION_TYPE_SHORT[item.operationType]}
                    </Badge>
                    <Badge variant="secondary" className="font-normal num">
                      待处理 {item.lines[0]?.remainingQuantity ?? "—"}{" "}
                      {item.lines[0]?.unitCode ?? ""}
                    </Badge>
                  </div>
                </button>
              ))}
            </CardContent>
          </Card>

          <div className="min-w-0 space-y-3">
            <SequentialProcessBar
              current={context?.position ?? currentIndex + 1}
              total={context?.total ?? tasks.length}
              leaseStatus={leaseStatus}
              leaseStatusLabel={leaseLabel}
              processLabel="确认过账"
              // 没有独立的「并下一项」路径：两个 handler 同义，第二个按钮名不副实
              showProcessNext={false}
              processDisabled={formalPending || !canPost}
              onBack={() => {
                if (sourceReturnHref) router.push(sourceReturnHref)
                else router.push("/workspace")
              }}
              onProcess={() => setConfirmOpen(true)}
              onProcessNext={() => setConfirmOpen(true)}
              onReclaim={() => {
                void ensureLease().catch((e) =>
                  setActionError(e instanceof Error ? e.message : "领取失败")
                )
              }}
            />

            <Card size="sm">
              <CardHeader className="border-b">
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div>
                    <CardTitle
                      ref={headingRef}
                      tabIndex={-1}
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
                      {" · 来源版本 "}
                      {task.sourceVersion}
                    </CardDescription>
                  </div>
                  <BusinessStatusBadge
                    context="list"
                    label={task.held ? "已暂挂" : task.statusLabel}
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
                        value: task.source.warehouseLabel ?? "不适用",
                      },
                      {
                        label: "待处理量",
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

                {task.gate.state !== "NOT_APPLICABLE" ? (
                  <div id="prepayment-gate">
                    <PrepaymentGate
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
                        dateTime: context?.snapshotUpdatedAt ?? new Date().toISOString(),
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
                            去供应商往来登记付款
                          </Button>
                        ) : undefined
                      }
                    />
                  </div>
                ) : (
                  <Alert>
                    <AlertTitle>仓发门禁</AlertTitle>
                    <AlertDescription>{task.gate.message}</AlertDescription>
                  </Alert>
                )}

                <Separator />

                <FulfillmentTypeForm
                  task={task}
                  draft={draft}
                  onChange={updateDraft}
                  disabled={formalPending || lastResult?.status === "unknown"}
                />

                {validationIssues.length > 0 ? (
                  <ValidationSummary
                    title="提交前校验"
                    issues={validationIssues}
                  />
                ) : (
                  <p className="text-xs text-muted-foreground">
                    客户端预检通过；过账仍以服务端守恒与门禁为准。
                  </p>
                )}

                {saveMessage ? (
                  <p className="text-xs text-muted-foreground">{saveMessage}</p>
                ) : null}

                <div className="sticky bottom-0 flex flex-wrap justify-end gap-2 border-t border-border bg-card/95 py-3 backdrop-blur">
                  <Button
                    type="button"
                    variant="outline"
                    disabled={formalPending}
                    onClick={() => setDeferOpen(true)}
                  >
                    <PauseIcon data-icon="inline-start" />
                    暂挂
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
                    确认过账并下一项
                  </Button>
                </div>
              </CardContent>
            </Card>
          </div>
        </div>
      ) : (
        <BusinessEmptyState
          kind="filter"
          title="筛选无结果"
          description={context?.filterSummary ?? "请调整类型或单号筛选"}
          action={
            <Button type="button" variant="outline" onClick={() => setTypeFilter("all")}>
              清除类型筛选
            </Button>
          }
        />
      )}

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={`确认${task ? OPERATION_TYPE_LABEL[task.operationType] : "履约"}过账`}
        description="将写入业务记录。结果未确定前不会乐观修改库存、预占或队列位置。"
        actionLabel="过账"
        confirmLabel="确认过账"
        fromStatus={{ label: "待过账", tone: "warning" }}
        toStatus={{ label: "已过账", tone: "success" }}
        lockedFields={[
          "来源销售/采购版本",
          "作业类型",
          "预占/采购销售分配引用",
        ]}
        effects={task && draft ? impactPreview(task, draft) : []}
        irreversibleEffects={[
          "已过账记录不可覆盖；纠错走冲正/退货/新记录",
          "库存与预占仅在处理结果确定后更新",
        ]}
        nextDepartment="成功后可进入客户验收（物流≠验收）"
        pending={postMutation.isPending}
        onConfirm={async () => {
          await handlePost()
        }}
      />

      <Dialog open={deferOpen} onOpenChange={setDeferOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>暂挂当前作业</DialogTitle>
            <DialogDescription>
              须填写受控原因。暂挂只释放处理权并移动本轮游标，任务仍在有效队列，不写
              paused 业务/任务状态。
            </DialogDescription>
          </DialogHeader>
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault()
              void deferForm.handleSubmit()
            }}
          >
            <deferForm.AppField
              name="reasonCode"
              children={(field) => (
                <div className="space-y-2">
                  <Label htmlFor="defer-reason">暂挂原因</Label>
                  <Select
                    value={field.state.value}
                    onValueChange={(v) => field.handleChange(v as DeferReasonCode)}
                  >
                    <SelectTrigger id="defer-reason">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {(
                        Object.keys(DEFER_REASON_LABEL) as DeferReasonCode[]
                      ).map((code) => (
                        <SelectItem key={code} value={code}>
                          {DEFER_REASON_LABEL[code]}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
            />
            <deferForm.AppField
              name="reasonNote"
              children={(field) => (
                <div className="space-y-2">
                  <Label htmlFor="defer-note">补充说明（可选）</Label>
                  <Textarea
                    id="defer-note"
                    value={field.state.value ?? ""}
                    onChange={(e) => field.handleChange(e.target.value)}
                    rows={3}
                  />
                </div>
              )}
            />
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                取消
              </DialogClose>
              <deferForm.AppForm>
                <Button type="submit" disabled={deferMutation.isPending}>
                  确认暂挂
                </Button>
              </deferForm.AppForm>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function FulfillmentTypeForm({
  task,
  draft,
  onChange,
  disabled,
}: {
  task: FulfillmentTask
  draft: FulfillmentDraft
  onChange: (d: FulfillmentDraft) => void
  disabled?: boolean
}) {
  if (draft.type === "RECEIPT") {
    return (
      <section className="space-y-3" aria-label="入库表单">
        <h3 className="text-sm font-semibold">入库作业</h3>
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label>入库仓</Label>
            <Input value={draft.warehouseLabel} disabled readOnly />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="receipt-at">入库时间</Label>
            <Input
              id="receipt-at"
              type="datetime-local"
              value={draft.occurredAt}
              disabled={disabled}
              onChange={(e) =>
                onChange({ ...draft, occurredAt: e.target.value })
              }
            />
          </div>
        </div>
        {draft.lines.map((line, i) => {
          const src = task.lines.find(
            (l) => l.purchaseRevisionLineId === line.purchaseRevisionLineId
          )
          return (
            <div
              key={line.purchaseRevisionLineId}
              className="space-y-3 rounded-xl border border-border p-3"
            >
              <p className="text-sm font-medium">
                {src?.itemName ?? line.purchaseRevisionLineId} · 剩余可收{" "}
                <span className="num">{src?.remainingQuantity}</span>
                {src?.unitCode}
              </p>
              <div className="grid gap-3 sm:grid-cols-3">
                <div className="space-y-1.5">
                  <Label htmlFor={`receipt-recv-${i}`}>到货数量</Label>
                  <Input
                    id={`receipt-recv-${i}`}
                    className="num"
                    inputMode="decimal"
                    value={line.receivedQuantity}
                    disabled={disabled}
                    onChange={(e) => {
                      const lines = draft.lines.map((l, idx) =>
                        idx === i
                          ? { ...l, receivedQuantity: e.target.value }
                          : l
                      )
                      onChange({ ...draft, lines })
                    }}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor={`receipt-qual-${i}`}>合格数量</Label>
                  <Input
                    id={`receipt-qual-${i}`}
                    className="num"
                    inputMode="decimal"
                    value={line.qualifiedQuantity}
                    disabled={disabled}
                    onChange={(e) => {
                      const lines = draft.lines.map((l, idx) =>
                        idx === i
                          ? { ...l, qualifiedQuantity: e.target.value }
                          : l
                      )
                      onChange({ ...draft, lines })
                    }}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor={`receipt-rej-${i}`}>不合格数量</Label>
                  <Input
                    id={`receipt-rej-${i}`}
                    className="num"
                    inputMode="decimal"
                    value={line.rejectedQuantity}
                    disabled={disabled}
                    onChange={(e) => {
                      const lines = draft.lines.map((l, idx) =>
                        idx === i
                          ? { ...l, rejectedQuantity: e.target.value }
                          : l
                      )
                      onChange({ ...draft, lines })
                    }}
                  />
                </div>
              </div>
              <div className="space-y-1.5">
                <Label htmlFor={`receipt-qr-${i}`}>质量结果</Label>
                <Input
                  id={`receipt-qr-${i}`}
                  value={line.qualityResult}
                  disabled={disabled}
                  onChange={(e) => {
                    const lines = draft.lines.map((l, idx) =>
                      idx === i ? { ...l, qualityResult: e.target.value } : l
                    )
                    onChange({ ...draft, lines })
                  }}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                仅合格量入库并建立预占；不合格量排除在库存之外。
              </p>
            </div>
          )
        })}
      </section>
    )
  }

  if (draft.type === "WAREHOUSE_SHIP") {
    return (
      <section className="space-y-3" aria-label="公司仓发表单">
        <h3 className="text-sm font-semibold">公司仓发</h3>
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label>发货仓</Label>
            <Input value={draft.warehouseLabel} disabled readOnly />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="ship-at">发货时间</Label>
            <Input
              id="ship-at"
              type="datetime-local"
              value={draft.shippedAt}
              disabled={disabled}
              onChange={(e) =>
                onChange({ ...draft, shippedAt: e.target.value })
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="ship-carrier">承运方</Label>
            <Input
              id="ship-carrier"
              value={draft.carrier}
              disabled={disabled}
              onChange={(e) =>
                onChange({ ...draft, carrier: e.target.value })
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="ship-tracking">物流单号</Label>
            <Input
              id="ship-tracking"
              value={draft.trackingNo}
              disabled={disabled}
              onChange={(e) =>
                onChange({ ...draft, trackingNo: e.target.value })
              }
            />
          </div>
        </div>
        {draft.lines.map((line, i) => {
          const src = task.lines.find(
            (l) => l.salesOrderLineId === line.salesOrderLineId
          )
          return (
            <div
              key={line.salesOrderLineId}
              className="space-y-3 rounded-xl border border-border p-3"
            >
              <p className="text-sm font-medium">
                {src?.itemName} · 预占{" "}
                <span className="num">{src?.reservedQuantity}</span>
                {src?.unitCode} · 可用{" "}
                <span className="num">{src?.availableOnHand}</span>
              </p>
              <p className="text-xs text-muted-foreground">
                预占 ID：{line.stockReservationId}（必须归属本销售明细）
              </p>
              <div className="space-y-1.5">
                <Label htmlFor={`ship-qty-${i}`}>本次发货数量</Label>
                <Input
                  id={`ship-qty-${i}`}
                  className="num"
                  inputMode="decimal"
                  value={line.quantity}
                  disabled={disabled}
                  onChange={(e) => {
                    const lines = draft.lines.map((l, idx) =>
                      idx === i ? { ...l, quantity: e.target.value } : l
                    )
                    onChange({ ...draft, lines })
                  }}
                />
              </div>
            </div>
          )
        })}
      </section>
    )
  }

  if (draft.type === "SUPPLIER_DIRECT") {
    return (
      <section className="space-y-3" aria-label="供应商直发表单">
        <h3 className="text-sm font-semibold">供应商直发</h3>
        <p className="text-xs text-muted-foreground">
          必须引用采购销售分配；过账不写自有库存流水。
        </p>
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label htmlFor="direct-carrier">承运方</Label>
            <Input
              id="direct-carrier"
              value={draft.carrier}
              disabled={disabled}
              onChange={(e) =>
                onChange({ ...draft, carrier: e.target.value })
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="direct-tracking">物流单号</Label>
            <Input
              id="direct-tracking"
              value={draft.trackingNo}
              disabled={disabled}
              onChange={(e) =>
                onChange({ ...draft, trackingNo: e.target.value })
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="direct-at">发货时间</Label>
            <Input
              id="direct-at"
              type="datetime-local"
              value={draft.shippedAt}
              disabled={disabled}
              onChange={(e) =>
                onChange({ ...draft, shippedAt: e.target.value })
              }
            />
          </div>
        </div>
        {draft.lines.map((line, i) => {
          const src = task.lines.find(
            (l) => l.salesOrderLineId === line.salesOrderLineId
          )
          return (
            <div
              key={line.salesOrderLineId}
              className="space-y-2 rounded-xl border border-border p-3"
            >
              <p className="text-sm font-medium">
                {src?.itemName} · 分配 {line.purchaseLineSalesAllocationId}
              </p>
              <div className="space-y-1.5">
                <Label htmlFor={`direct-qty-${i}`}>发货数量</Label>
                <Input
                  id={`direct-qty-${i}`}
                  className="num"
                  value={line.quantity}
                  disabled={disabled}
                  onChange={(e) => {
                    const lines = draft.lines.map((l, idx) =>
                      idx === i ? { ...l, quantity: e.target.value } : l
                    )
                    onChange({ ...draft, lines })
                  }}
                />
              </div>
            </div>
          )
        })}
      </section>
    )
  }

  if (draft.type === "ELECTRONIC") {
    return (
      <section className="space-y-3" aria-label="电子交付表单">
        <h3 className="text-sm font-semibold">电子交付</h3>
        <p className="text-xs text-muted-foreground">
          交付对象仅展示掩码；明文不进入日志/普通 Query 缓存。失败不可覆盖。
        </p>
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label>交付对象</Label>
            <Input value={draft.recipientMasked} disabled readOnly />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="el-at">实际时间</Label>
            <Input
              id="el-at"
              type="datetime-local"
              value={draft.occurredAt}
              disabled={disabled}
              onChange={(e) =>
                onChange({ ...draft, occurredAt: e.target.value })
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="el-result">履约结果</Label>
            <Select
              value={draft.result}
              onValueChange={(v) =>
                onChange({
                  ...draft,
                  result: v as "SUCCESS" | "PARTIAL" | "FAILED",
                })
              }
              disabled={disabled}
            >
              <SelectTrigger id="el-result">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="SUCCESS">成功</SelectItem>
                <SelectItem value="PARTIAL">部分成功</SelectItem>
                <SelectItem value="FAILED">失败</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
        {draft.lines.map((line, i) => (
          <div
            key={line.salesOrderLineId}
            className="space-y-2 rounded-xl border border-border p-3"
          >
            <p className="text-sm font-medium">
              分配 {line.purchaseLineSalesAllocationId}
            </p>
            <div className="space-y-1.5">
              <Label htmlFor={`el-qty-${i}`}>交付数量</Label>
              <Input
                id={`el-qty-${i}`}
                className="num"
                value={line.quantity}
                disabled={disabled}
                onChange={(e) => {
                  const lines = draft.lines.map((l, idx) =>
                    idx === i ? { ...l, quantity: e.target.value } : l
                  )
                  onChange({ ...draft, lines })
                }}
              />
            </div>
          </div>
        ))}
      </section>
    )
  }

  // SERVICE
  return (
    <section className="space-y-3" aria-label="线下服务表单">
      <h3 className="text-sm font-semibold">线下服务</h3>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1.5 sm:col-span-2">
          <Label htmlFor="service-loc">服务地点</Label>
          <Input
            id="service-loc"
            value={draft.serviceLocation}
            disabled={disabled}
            onChange={(e) =>
              onChange({ ...draft, serviceLocation: e.target.value })
            }
          />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="service-start">开始时间</Label>
          <Input
            id="service-start"
            type="datetime-local"
            value={draft.startedAt}
            disabled={disabled}
            onChange={(e) =>
              onChange({ ...draft, startedAt: e.target.value })
            }
          />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="service-ended">结束时间</Label>
          <Input
            id="service-ended"
            type="datetime-local"
            value={draft.endedAt}
            disabled={disabled}
            onChange={(e) =>
              onChange({ ...draft, endedAt: e.target.value })
            }
          />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="svc-result">履约结果</Label>
          <Select
            value={draft.result}
            onValueChange={(v) =>
              onChange({
                ...draft,
                result: v as "SUCCESS" | "PARTIAL" | "FAILED",
              })
            }
            disabled={disabled}
          >
            <SelectTrigger id="svc-result">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="SUCCESS">成功</SelectItem>
              <SelectItem value="PARTIAL">部分成功</SelectItem>
              <SelectItem value="FAILED">失败</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1.5 sm:col-span-2">
          <Label htmlFor="service-note">完成说明</Label>
          <Textarea
            id="service-note"
            value={draft.completionNote}
            disabled={disabled}
            rows={3}
            onChange={(e) =>
              onChange({ ...draft, completionNote: e.target.value })
            }
          />
        </div>
      </div>
      {draft.lines.map((line, i) => (
        <div
          key={line.salesOrderLineId}
          className="space-y-2 rounded-xl border border-border p-3"
        >
          <p className="text-sm font-medium">
            分配 {line.purchaseLineSalesAllocationId}
          </p>
          <div className="space-y-1.5">
            <Label htmlFor={`svc-qty-${i}`}>服务数量</Label>
            <Input
              id={`svc-qty-${i}`}
              className="num"
              value={line.quantity}
              disabled={disabled}
              onChange={(e) => {
                const lines = draft.lines.map((l, idx) =>
                  idx === i ? { ...l, quantity: e.target.value } : l
                )
                onChange({ ...draft, lines })
              }}
            />
          </div>
        </div>
      ))}
    </section>
  )
}
