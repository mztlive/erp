"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ArrowRightIcon,
  CheckIcon,
  CircleCheckIcon,
  FileSearchIcon,
  PauseIcon,
  PlusIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react"
import { z } from "zod"

import {
  BusinessEmptyState,
  BusinessStatusBadge,
  DataFreshness,
  DocumentSummary,
  FormalActionConfirmDialog,
  FormalActionResult,
  PageHeader,
  SequentialProcessBar,
  ValidationSummary,
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
import type {
  ConfirmationLineDraft,
  FormalOutcome,
  FulfillmentMode,
  RejectReasonCode,
} from "@/features/procurement-confirmation/types"
import {
  FULFILLMENT_MODE_LABEL,
  NEXT_SALES_RESOLUTION_COPY,
  REJECT_REASON_LABEL,
} from "@/features/procurement-confirmation/types"
import {
  useClaimProcurementMutation,
  useCompleteProcurementMutation,
  useDeferProcurementMutation,
  useProcurementConfirmationQuery,
  useResolveUnknownProcurementMutation,
  useSaveProcurementConfirmationMutation,
} from "@/features/procurement-confirmation/queries"

const rejectSchema = z.object({
  reasonCode: z.enum([
    "UNFULFILLABLE",
    "COST_INCREASE",
    "DELIVERY_UNMET",
    "QUALIFICATION_INVALID",
  ]),
  comment: z.string().trim().min(5, "请填写至少 5 个字的补充说明"),
})

const money = new Intl.NumberFormat("zh-CN", {
  style: "currency",
  currency: "CNY",
  minimumFractionDigits: 2,
})

/** 会话内存中的 claimToken（不写 URL / localStorage） */
type SessionLease = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expiresAt: string
}

type ResultState =
  | {
      status: "succeeded" | "rejected" | "blocked" | "unknown"
      title: string
      description: string
      reference?: string
      outcome?: FormalOutcome
      stayOnItem?: boolean
      pendingIdempotencyKey?: string
    }
  | null

function shortHash(hash: string) {
  if (hash.length <= 16) return hash
  return `${hash.slice(0, 10)}…${hash.slice(-4)}`
}

function buildReturnHref(searchParams: URLSearchParams) {
  const qs = searchParams.toString()
  return qs ? `/procurement/confirm?${qs}` : "/procurement/confirm"
}

function w05Href(salesOrderId: string, returnTo: string, workItemId?: string) {
  const params = new URLSearchParams({
    from: "W07",
    returnTo,
  })
  if (workItemId) params.set("sourceWorkItemId", workItemId)
  return `/sales/orders/${salesOrderId}?${params.toString()}`
}

export function ProcurementConfirmationPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const scope: "mine" | "role_pool" =
    searchParams.get("scope") === "role_pool" ? "role_pool" : "mine"
  const dueParam = searchParams.get("due")
  const due: "active" | "today" | "overdue" =
    dueParam === "today" || dueParam === "overdue" || dueParam === "active"
      ? dueParam
      : "active"
  const sortParam = searchParams.get("sort")
  const sort: "due_at" | "submitted_at" | "priority" =
    sortParam === "submitted_at" || sortParam === "priority"
      ? sortParam
      : "due_at"
  const orderNo = searchParams.get("orderNo") ?? undefined
  const currentWorkItemId =
    searchParams.get("currentWorkItemId") ??
    searchParams.get("task") ??
    undefined
  const isProcessingEntry =
    searchParams.get("from") === "W02" && Boolean(currentWorkItemId)
  const queueContextId =
    searchParams.get("queueContextId") ??
    `queue:procurement-confirmation:demo:${scope}`

  // autoNext：显式 URL 优先；否则会话默认 true；不写 localStorage
  const autoNextExplicit = searchParams.get("autoNext")
  const [sessionAutoNext, setSessionAutoNext] = React.useState(true)
  const autoNext =
    autoNextExplicit === "0"
      ? false
      : autoNextExplicit === "1"
        ? true
        : sessionAutoNext

  const filters = React.useMemo(
    () => ({
      scope,
      due,
      sort,
      orderNo,
      currentWorkItemId,
      queueContextId,
    }),
    [scope, due, sort, orderNo, currentWorkItemId, queueContextId]
  )

  const queueQuery = useProcurementConfirmationQuery(filters)
  const claimMutation = useClaimProcurementMutation()
  const saveMutation = useSaveProcurementConfirmationMutation()
  const completeMutation = useCompleteProcurementMutation()
  const deferMutation = useDeferProcurementMutation()
  const resolveUnknownMutation = useResolveUnknownProcurementMutation()

  const view = queueQuery.data
  const tasks = view?.tasks ?? []
  const context = view?.context
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

  const [lineDrafts, setLineDrafts] = React.useState<ConfirmationLineDraft[]>(
    []
  )
  const [dirty, setDirty] = React.useState(false)
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [rejectOpen, setRejectOpen] = React.useState(false)
  const [advanceAfterConfirm, setAdvanceAfterConfirm] = React.useState(true)
  const [lastResult, setLastResult] = React.useState<ResultState>(null)
  const [forceUnknownOnce, setForceUnknownOnce] = React.useState(false)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [saveMessage, setSaveMessage] = React.useState<string | null>(null)
  const headingRef = React.useRef<HTMLHeadingElement>(null)
  const resultRef = React.useRef<HTMLDivElement>(null)
  /** 会话内存 claimToken，禁止序列化到 URL / storage */
  const leaseRef = React.useRef<SessionLease | null>(null)
  const [leaseEpoch, setLeaseEpoch] = React.useState(0)
  const idempotencyRef = React.useRef<{
    approve?: string
    reject?: string
    defer?: string
  }>({})

  // 同步当前任务分行草稿
  React.useEffect(() => {
    if (!task) {
      setLineDrafts([])
      setDirty(false)
      return
    }
    setLineDrafts(task.confirmation.lines.map((l) => ({ ...l })))
    setDirty(false)
    setActionError(null)
    setSaveMessage(null)
    idempotencyRef.current = {}
    // 切换任务时若租约不属于本任务则清空会话令牌展示态
    if (leaseRef.current?.workItemId !== task.workItemId) {
      // 保留令牌直至重新领取；UI 显示为未领取
    }
  }, [task?.workItemId, task?.confirmation.editVersion])

  // 默认 URL：scope / currentWorkItemId / queueContextId（不写 autoNext 除非用户切换）
  React.useEffect(() => {
    if (queueQuery.isPending || !view) return
    const hasScope = searchParams.has("scope")
    const hasItem = searchParams.has("currentWorkItemId") || searchParams.has("task")
    const hasCtx = searchParams.has("queueContextId")
    if (hasScope && hasCtx && (hasItem || tasks.length === 0)) return
    const params = new URLSearchParams(searchParams.toString())
    if (!hasScope) params.set("scope", scope)
    if (!hasCtx) params.set("queueContextId", queueContextId)
    if (!hasItem && task) {
      params.set("currentWorkItemId", task.workItemId)
      params.delete("task")
    }
    // 兼容旧 task= 参数：迁移到 currentWorkItemId
    if (searchParams.has("task") && task) {
      params.set("currentWorkItemId", task.workItemId)
      params.delete("task")
    }
    params.delete("completed")
    const qs = params.toString()
    const next = qs ? `${pathname}?${qs}` : pathname
    router.replace(next, { scroll: false })
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

  // Only an explicit W02 process transition may auto-claim. Sidebar/default
  // navigation and W05 read-only inspection must never acquire a lease.
  React.useEffect(() => {
    if (
      !task ||
      !isProcessingEntry ||
      task.workItemId !== currentWorkItemId
    ) {
      return
    }
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
        /* 保持未领取态 */
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅在任务切换时领取
  }, [currentWorkItemId, isProcessingEntry, task?.workItemId])

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
      params.delete("task")
      params.delete("completed")
      const qs = params.toString()
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    },
    [pathname, router, searchParams]
  )

  const goToWorkItem = React.useCallback(
    (workItemId: string | undefined | null) => {
      setLastResult(null)
      setActionError(null)
      if (!workItemId) {
        replaceUrl({ currentWorkItemId: null })
        return
      }
      replaceUrl({ currentWorkItemId: workItemId })
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
  // leaseEpoch 触发重渲染以读 ref
  void leaseEpoch

  const coverage = React.useMemo(() => {
    if (!task) return []
    return task.salesSubmission.lines.map((line) => {
      const confirmed = lineDrafts
        .filter((c) => c.submissionLineId === line.submissionLineId)
        .reduce((sum, c) => sum + Number(c.confirmedQuantity || 0), 0)
      const required = Number(line.committedQuantity)
      const complete = confirmed + 1e-9 >= required && required > 0
      const gap = Math.max(0, required - confirmed)
      return {
        submissionLineId: line.submissionLineId,
        itemName: line.itemName,
        confirmed: confirmed.toFixed(0),
        required: line.committedQuantity,
        complete,
        gap: gap.toFixed(0),
      }
    })
  }, [task, lineDrafts])

  const allCovered = coverage.every((c) => c.complete)
  const clientBlocking = coverage
    .filter((c) => !c.complete)
    .map((c) => ({
      id: c.submissionLineId,
      label: c.itemName,
      message: `已确认 ${c.confirmed}/${c.required}，缺口 ${c.gap}`,
      targetId: `submission-line-${c.submissionLineId}`,
    }))

  const estimatedPurchase = lineDrafts
    .reduce(
      (sum, l) =>
        sum +
        Number(l.confirmedQuantity || 0) * Number(l.latestCostGross || 0),
      0
    )
    .toFixed(2)

  const updateLine = React.useCallback(
    (lineKey: string, patch: Partial<ConfirmationLineDraft>) => {
      setLineDrafts((prev) =>
        prev.map((l) => (l.lineKey === lineKey ? { ...l, ...patch } : l))
      )
      setDirty(true)
    },
    []
  )

  const addSplitLine = React.useCallback(
    (submissionLineId: string) => {
      if (!task) return
      const sub = task.salesSubmission.lines.find(
        (l) => l.submissionLineId === submissionLineId
      )
      if (!sub) return
      const key = `cl_new_${submissionLineId}_${Date.now().toString(36)}`
      setLineDrafts((prev) => [
        ...prev,
        {
          lineKey: key,
          submissionLineId,
          supplierId: "sup_hd",
          supplierName: "华东优选供应链有限公司",
          confirmedQuantity: "0",
          latestCostGross: sub.referenceCost ?? "0.00",
          inputTaxRate: "0.13",
          expectedDeliveryDate: sub.requestedDeliveryDate,
          fulfillmentMode: "WAREHOUSE",
          capabilityRevisionId: "cap_hd_v3",
          capabilitySummary: "新拆分行 · 待核对能力",
          qualificationStatus: "VALID",
        },
      ])
      setDirty(true)
      requestAnimationFrame(() => {
        document.getElementById(`line-supplier-${key}`)?.focus()
      })
    },
    [task]
  )

  const removeLine = React.useCallback((lineKey: string) => {
    setLineDrafts((prev) => {
      const target = prev.find((l) => l.lineKey === lineKey)
      if (!target) return prev
      const same = prev.filter(
        (l) => l.submissionLineId === target.submissionLineId
      )
      if (same.length <= 1) return prev
      return prev.filter((l) => l.lineKey !== lineKey)
    })
    setDirty(true)
  }, [])

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

  const handleSave = React.useCallback(async () => {
    if (!task) return
    try {
      const lease = await ensureLease()
      const result = await saveMutation.mutateAsync({
        workItemId: task.workItemId,
        confirmationId: task.confirmation.confirmationId,
        submissionId: task.salesSubmission.submissionId,
        expectedEditVersion: task.confirmation.editVersion,
        claimToken: lease.claimToken,
        leaseVersion: lease.leaseVersion,
        lines: lineDrafts,
        idempotencyKey: `save_${task.workItemId}_${Date.now()}`,
      })
      setDirty(false)
      setSaveMessage(`已保存 · 编辑版本 ${result.editVersion}`)
      setActionError(null)
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "保存失败")
    }
  }, [ensureLease, lineDrafts, saveMutation, task])

  const advanceIfNeeded = React.useCallback(
    (shouldAdvance: boolean) => {
      if (!shouldAdvance) return
      const nextId = neighborId(1) ?? tasks.find((t) => t.workItemId !== task?.workItemId)
        ?.workItemId
      if (nextId) {
        // 清空本任务租约引用（任务已终局）
        leaseRef.current = null
        setLeaseEpoch((n) => n + 1)
        goToWorkItem(nextId)
      } else {
        replaceUrl({ currentWorkItemId: null })
      }
    },
    [goToWorkItem, neighborId, replaceUrl, task?.workItemId, tasks]
  )

  const handleApprove = React.useCallback(async () => {
    if (!task) return
    setActionError(null)
    try {
      const lease = await ensureLease()
      if (!idempotencyRef.current.approve) {
        idempotencyRef.current.approve = `approve_${task.workItemId}_${crypto.randomUUID()}`
      }
      const response = await completeMutation.mutateAsync({
        workItemId: task.workItemId,
        claimToken: lease.claimToken,
        leaseVersion: lease.leaseVersion,
        expectedSubjectHash: task.salesSubmission.subjectHash,
        expectedSubjectVersion: task.subjectVersion,
        idempotencyKey: idempotencyRef.current.approve,
        forceUnknown: forceUnknownOnce,
        decision: {
          reviewResult: "APPROVED",
          confirmationId: task.confirmation.confirmationId,
          submissionId: task.salesSubmission.submissionId,
          expectedConfirmationEditVersion: task.confirmation.editVersion,
          salesOrderId: task.salesSubmission.salesOrderId,
          salesOrderNo: task.salesSubmission.salesOrderNo,
          subjectHash: task.salesSubmission.subjectHash,
        },
      })
      setForceUnknownOnce(false)

      if (response.status === "unknown") {
        setLastResult({
          status: "unknown",
          title: "正式结果未知",
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

      const outcome = response.outcome
      if (outcome.kind !== "APPROVED_AND_SALES_EFFECTIVE") return
      leaseRef.current = null
      setLeaseEpoch((n) => n + 1)
      setLastResult({
        status: "succeeded",
        title: "采购确认已通过 · 销售单已生效",
        description: advanceAfterConfirm && autoNext
          ? "正式结果已记录；队列将打开下一条。"
          : "正式结果已记录。可核对采购创建依据后再打开下一条。",
        reference: outcome.reference,
        outcome,
        stayOnItem: !(advanceAfterConfirm && autoNext),
      })
      if (advanceAfterConfirm && autoNext) {
        advanceIfNeeded(true)
      }
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "通过失败")
    }
  }, [
    advanceAfterConfirm,
    advanceIfNeeded,
    autoNext,
    completeMutation,
    ensureLease,
    forceUnknownOnce,
    task,
  ])

  const handleRejectSubmit = React.useCallback(
    async (value: { reasonCode: RejectReasonCode; comment: string }) => {
      if (!task) return
      setActionError(null)
      try {
        const lease = await ensureLease()
        if (!idempotencyRef.current.reject) {
          idempotencyRef.current.reject = `reject_${task.workItemId}_${crypto.randomUUID()}`
        }
        const response = await completeMutation.mutateAsync({
          workItemId: task.workItemId,
          claimToken: lease.claimToken,
          leaseVersion: lease.leaseVersion,
          expectedSubjectHash: task.salesSubmission.subjectHash,
          expectedSubjectVersion: task.subjectVersion,
          idempotencyKey: idempotencyRef.current.reject,
          decision: {
            reviewResult: "REJECTED",
            confirmationId: task.confirmation.confirmationId,
            submissionId: task.salesSubmission.submissionId,
            expectedConfirmationEditVersion: task.confirmation.editVersion,
            salesOrderId: task.salesSubmission.salesOrderId,
            salesOrderNo: task.salesSubmission.salesOrderNo,
            subjectHash: task.salesSubmission.subjectHash,
            rejectReasonCode: value.reasonCode,
            comment: value.comment,
          },
        })
        setRejectOpen(false)
        if (response.status === "unknown") {
          setLastResult({
            status: "unknown",
            title: "正式结果未知",
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
        const outcome = response.outcome
        if (outcome.kind !== "REJECTED_TO_SALES") return
        leaseRef.current = null
        setLeaseEpoch((n) => n + 1)
        setLastResult({
          status: "rejected",
          title: "采购确认已驳回 · 旧任务已完成",
          description:
            "已形成本次采购确认的正式 REJECTED 结论；未创建采购单、变更单或后继任务。销售仅能在 W05 选择固定三路。",
          reference: outcome.reference,
          outcome,
          stayOnItem: !autoNext,
        })
        if (autoNext) {
          advanceIfNeeded(true)
        }
      } catch (error) {
        setActionError(error instanceof Error ? error.message : "驳回失败")
      }
    },
    [advanceIfNeeded, autoNext, completeMutation, ensureLease, task]
  )

  const rejectForm = useAppForm({
    defaultValues: {
      reasonCode: "UNFULFILLABLE" as RejectReasonCode,
      comment: "",
    },
    validators: { onChange: rejectSchema },
    onSubmit: async ({ value }) => {
      await handleRejectSubmit({
        reasonCode: value.reasonCode as RejectReasonCode,
        comment: value.comment.trim(),
      })
    },
  })

  const handleDefer = React.useCallback(async () => {
    if (!task) return
    setActionError(null)
    try {
      if (dirty) {
        await handleSave()
      }
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
        nextWorkItemId: nextId,
        idempotencyKey: idempotencyRef.current.defer,
      })
      if (response.status !== "succeeded" || response.outcome.kind !== "DEFERRED") {
        if (response.status === "failed") setActionError(response.message)
        return
      }
      leaseRef.current = null
      setLeaseEpoch((n) => n + 1)
      setLastResult({
        status: "blocked",
        title: "当前项已暂挂",
        description:
          "任务仍保留在有效队列（PENDING/IN_PROGRESS），未形成通过或驳回结论。已释放租约并打开下一条。",
        reference: response.outcome.reference,
        outcome: response.outcome,
      })
      if (nextId) goToWorkItem(nextId)
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "暂挂失败")
    }
  }, [
    dirty,
    deferMutation,
    ensureLease,
    goToWorkItem,
    handleSave,
    neighborId,
    queueContextId,
    task,
  ])

  const handleResolveUnknown = React.useCallback(
    async (settle: boolean) => {
      if (!lastResult?.pendingIdempotencyKey || !task) return
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
                expectedSubjectHash: task.salesSubmission.subjectHash,
                expectedSubjectVersion: task.subjectVersion,
                idempotencyKey: lastResult.pendingIdempotencyKey,
                decision: {
                  reviewResult: "APPROVED",
                  confirmationId: task.confirmation.confirmationId,
                  submissionId: task.salesSubmission.submissionId,
                  expectedConfirmationEditVersion:
                    task.confirmation.editVersion,
                  salesOrderId: task.salesSubmission.salesOrderId,
                  salesOrderNo: task.salesSubmission.salesOrderNo,
                  subjectHash: task.salesSubmission.subjectHash,
                },
              }
            : undefined,
      })
      if (response.status === "unknown") {
        setLastResult({
          status: "unknown",
          title: "正式结果仍未知",
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
      const outcome = response.outcome
      if (outcome.kind === "APPROVED_AND_SALES_EFFECTIVE") {
        setLastResult({
          status: "succeeded",
          title: "查询确认：采购确认已通过",
          description: "幂等键返回同一正式结果，未重复推进销售状态。",
          reference: outcome.reference,
          outcome,
          stayOnItem: !autoNext,
        })
        if (autoNext) advanceIfNeeded(true)
      }
    },
    [
      advanceIfNeeded,
      autoNext,
      lastResult?.pendingIdempotencyKey,
      resolveUnknownMutation,
      task,
    ]
  )

  // 键盘：无输入焦点时 j/k 切换；⌘S 保存；⌘↵ 打开通过
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
        if (allCovered && activeLease) {
          setAdvanceAfterConfirm(autoNext)
          setConfirmOpen(true)
        }
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
  }, [
    activeLease,
    allCovered,
    autoNext,
    dirty,
    goToWorkItem,
    handleSave,
    neighborId,
  ])

  const toggleAutoNext = React.useCallback(
    (next: boolean) => {
      // preferenceScope 未配置：只写显式 URL / 会话，不写 localStorage
      setSessionAutoNext(next)
      replaceUrl({ autoNext: next ? "1" : "0" })
    },
    [replaceUrl]
  )

  const formalPending =
    completeMutation.isPending ||
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
    ? `已领取 · 租约 v${activeLease.leaseVersion}`
    : claimMutation.isPending
      ? "正在取得处理权…"
      : "待领取"

  const returnTo = buildReturnHref(
    new URLSearchParams(searchParams.toString())
  )

  if (queueQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="采购二次确认" description="正在加载队列…" />
        <div
          className="h-24 animate-pulse rounded-2xl bg-muted"
          aria-hidden="true"
        />
        <div className="grid gap-4 xl:grid-cols-[minmax(0,2fr)_minmax(16rem,1fr)]">
          <div className="h-80 animate-pulse rounded-2xl bg-muted" />
          <div className="h-64 animate-pulse rounded-2xl bg-muted" />
        </div>
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="采购二次确认"
        description="连续核对不可变销售提交的供应商、可供数量、成本、税率与交期；通过/驳回/暂挂后无需返回列表。"
        breadcrumbs={[
          {
            id: "procurement",
            label: "采购与履约",
            href: "/procurement/confirm",
          },
          { id: "confirm", label: "二次确认", current: true },
        ]}
        metadata={
          <div className="flex flex-wrap items-center gap-3">
            <DataFreshness
              updatedAt="刚刚"
              dateTime={
                context?.queueContextUpdatedAt ?? new Date().toISOString()
              }
              state="fresh"
              label="队列水位"
            />
            <span className="text-xs text-muted-foreground" aria-live="polite">
              {context?.filterSummary ?? "仅我的"} · 待处理{" "}
              {context?.total ?? 0}
            </span>
          </div>
        }
      />

      <div className="flex flex-wrap items-center gap-3 rounded-2xl border border-border bg-card px-3 py-2 text-sm">
        <div className="flex items-center gap-2">
          <Label htmlFor="auto-next" className="text-muted-foreground">
            自动下一项
          </Label>
          <Switch
            id="auto-next"
            checked={autoNext}
            onCheckedChange={toggleAutoNext}
            aria-describedby="auto-next-hint"
          />
          <span id="auto-next-hint" className="sr-only">
            仅写入显式 URL 与当前会话，未配置 preferenceScope 时不持久化
          </span>
        </div>
        <Badge variant="outline" className="font-normal">
          偏好范围：未配置（会话临时）
        </Badge>
        <label className="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
          <input
            type="checkbox"
            className="size-3.5"
            checked={forceUnknownOnce}
            onChange={(e) => setForceUnknownOnce(e.target.checked)}
          />
          下次通过模拟结果未知
        </label>
      </div>

      {lastResult ? (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
          <FormalActionResult
            status={lastResult.status}
            title={lastResult.title}
            description={lastResult.description}
            reference={lastResult.reference}
            facts={buildResultFacts(lastResult.outcome, context)}
            actions={
              <ResultActions
                lastResult={lastResult}
                taskSalesOrderId={
                  lastResult.outcome &&
                  "salesOrderId" in lastResult.outcome
                    ? lastResult.outcome.salesOrderId
                    : task?.salesSubmission.salesOrderId
                }
                returnTo={returnTo}
                onNext={() => {
                  const next =
                    context?.nextWorkItemId ??
                    neighborId(1) ??
                    tasks[0]?.workItemId
                  goToWorkItem(next)
                }}
                onResolveUnknown={() => void handleResolveUnknown(false)}
                onSettleUnknown={() => void handleResolveUnknown(true)}
                resolving={resolveUnknownMutation.isPending}
              />
            }
          />
          {lastResult.outcome?.kind === "REJECTED_TO_SALES" ? (
            <ProcurementRejectionNextSteps
              salesOrderId={lastResult.outcome.salesOrderId}
              returnTo={returnTo}
              rejectedSubmissionId={lastResult.outcome.rejectedSubmissionId}
              rejectedSubjectHash={lastResult.outcome.rejectedSubjectHash}
            />
          ) : null}
        </div>
      ) : null}

      {actionError ? (
        <Alert variant="destructive" role="alert">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertTitle>操作未生效</AlertTitle>
          <AlertDescription>{actionError}</AlertDescription>
        </Alert>
      ) : null}

      {completed ? (
        <BusinessEmptyState
          kind="no-tasks"
          title="本筛选项已处理完"
          description="当前采购二次确认队列已经清空，可以返回工作台处理其它事项。"
          action={
            <Button render={<Link href="/workspace" />}>返回今日工作台</Button>
          }
        />
      ) : task ? (
        <>
          <SequentialProcessBar
            current={context?.position ?? currentIndex + 1}
            total={context?.total ?? tasks.length}
            leaseStatus={leaseStatus}
            leaseStatusLabel={leaseLabel}
            processLabel="确认通过"
            processNextLabel="通过并打开下一条"
            processDisabled={
              formalPending ||
              Boolean(lastResult?.status === "unknown") ||
              !allCovered
            }
            pending={formalPending}
            onBack={() => router.push("/workspace")}
            onProcess={() => {
              setAdvanceAfterConfirm(false)
              setConfirmOpen(true)
            }}
            onProcessNext={() => {
              setAdvanceAfterConfirm(true)
              setConfirmOpen(true)
            }}
            onReclaim={() => {
              void ensureLease().catch((error) => {
                setActionError(
                  error instanceof Error ? error.message : "领取失败"
                )
              })
            }}
          />

          {/* 1440 双栏：左销售提交+分行，右决策摘要 sticky */}
          <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,66fr)_minmax(17rem,34fr)]">
            <div className="min-w-0 space-y-4">
              {task.salesSubmission.resubmissionContext ? (
                <Alert variant="info">
                  <TriangleAlertIcon aria-hidden="true" />
                  <AlertTitle>
                    {task.salesSubmission.origin ===
                    "LOW_MARGIN_MANAGER_APPROVED"
                      ? "低毛利上级通过后重提 · 仍待采购确认"
                      : "改品/改价后新提交 · 须重新确认"}
                  </AlertTitle>
                  <AlertDescription>
                    新 submissionId{" "}
                    <span className="num font-mono">
                      {task.salesSubmission.submissionId}
                    </span>
                    ，指纹{" "}
                    <span className="num font-mono">
                      {task.salesSubmission.subjectHashSummary}
                    </span>
                    。上一驳回提交{" "}
                    <span className="num font-mono">
                      {
                        task.salesSubmission.resubmissionContext
                          .previousRejectedSubmissionId
                      }
                    </span>
                    。
                    {task.salesSubmission.resubmissionContext
                      .lowMarginManagerConfirmationEvidenceReference
                      ? ` 上级证据 ${task.salesSubmission.resubmissionContext.lowMarginManagerConfirmationEvidenceReference} 不能自动通过。`
                      : " 不得复用旧确认分行。"}
                  </AlertDescription>
                </Alert>
              ) : null}

              <Card className="min-w-0" size="sm">
                <CardHeader className="border-b">
                  <div className="flex flex-wrap items-center gap-2">
                    <CardTitle>
                      <h2
                        ref={headingRef}
                        tabIndex={-1}
                        className="outline-none"
                        aria-live="polite"
                      >
                        {task.salesSubmission.salesOrderNo} ·{" "}
                        {task.salesSubmission.customerSnapshot}
                      </h2>
                    </CardTitle>
                    <BusinessStatusBadge
                      context="list"
                      label={task.riskLabel}
                      tone={task.riskTone}
                    />
                    <Badge variant="secondary">行为/任务 · 无确认单号</Badge>
                  </div>
                  <CardDescription>
                    不可变提交 第 {task.salesSubmission.submissionNo} 次 ·{" "}
                    {task.salesSubmission.submittedAt} ·{" "}
                    {task.salesSubmission.submittedByLabel}
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-5">
                  <DocumentSummary
                    columns="two"
                    items={[
                      {
                        id: "submissionId",
                        label: "提交身份 submissionId",
                        value: (
                          <span className="num font-mono text-sm">
                            {task.salesSubmission.submissionId}
                          </span>
                        ),
                        emphasized: true,
                      },
                      {
                        id: "subjectHash",
                        label: "内容指纹 subjectHash",
                        value: (
                          <span className="num font-mono text-sm">
                            {task.salesSubmission.subjectHashSummary}
                          </span>
                        ),
                        description: shortHash(
                          task.salesSubmission.subjectHash
                        ),
                      },
                      {
                        id: "contract",
                        label: "合同（提交快照）",
                        value:
                          task.salesSubmission.contractSnapshot ?? "—",
                      },
                      {
                        id: "payment",
                        label: "客户付款条件",
                        value: task.salesSubmission.paymentTermLabel,
                      },
                      {
                        id: "gross",
                        label: "销售含税金额",
                        value: money.format(
                          Number(task.salesSubmission.grossAmount)
                        ),
                        numeric: true,
                      },
                      {
                        id: "impact",
                        label: "业务影响",
                        value: task.impactSummary,
                      },
                    ]}
                  />

                  <Alert
                    variant={
                      task.riskTone === "destructive"
                        ? "destructive"
                        : task.riskTone === "success"
                          ? "success"
                          : "warning"
                    }
                  >
                    {task.riskTone === "success" ? (
                      <CircleCheckIcon aria-hidden="true" />
                    ) : (
                      <TriangleAlertIcon aria-hidden="true" />
                    )}
                    <AlertTitle>{task.riskLabel}</AlertTitle>
                    <AlertDescription>
                      {task.riskDescription}
                    </AlertDescription>
                  </Alert>

                  <Separator />

                  <section aria-labelledby="confirm-lines-title">
                    <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                      <h3
                        id="confirm-lines-title"
                        className="text-sm font-semibold"
                      >
                        销售明细与采购确认分行
                      </h3>
                      <span className="text-xs text-muted-foreground">
                        至少展示两条确认分行；覆盖按明细独立核算
                      </span>
                    </div>

                    <div className="space-y-5">
                      {task.salesSubmission.lines.map((subLine) => {
                        const lines = lineDrafts.filter(
                          (l) =>
                            l.submissionLineId === subLine.submissionLineId
                        )
                        const cov = coverage.find(
                          (c) =>
                            c.submissionLineId === subLine.submissionLineId
                        )
                        return (
                          <div
                            key={subLine.submissionLineId}
                            id={`submission-line-${subLine.submissionLineId}`}
                            className="rounded-xl border border-border"
                            tabIndex={-1}
                          >
                            <div className="flex flex-wrap items-start justify-between gap-2 border-b border-border bg-muted/40 px-3 py-2">
                              <div>
                                <p className="text-sm font-medium">
                                  {subLine.itemName}{" "}
                                  <span className="num text-muted-foreground">
                                    {subLine.itemSku}
                                  </span>
                                </p>
                                <p className="text-xs text-muted-foreground">
                                  承诺{" "}
                                  <span className="num">
                                    {subLine.committedQuantity} {subLine.unit}
                                  </span>{" "}
                                  · 客户期望 {subLine.requestedDeliveryDate}
                                  {subLine.referenceSupplier
                                    ? ` · 参考 ${subLine.referenceSupplier} / ${subLine.referenceCost}`
                                    : null}
                                </p>
                              </div>
                              <div
                                className="text-right text-xs"
                                aria-live="polite"
                              >
                                <Badge
                                  variant={
                                    cov?.complete ? "secondary" : "destructive"
                                  }
                                >
                                  覆盖 {cov?.confirmed}/{cov?.required}
                                  {cov && !cov.complete
                                    ? ` · 缺口 ${cov.gap}`
                                    : " · 完整"}
                                </Badge>
                              </div>
                            </div>

                            <div className="overflow-x-auto">
                              <table className="w-full min-w-[40rem] text-sm">
                                <caption className="sr-only">
                                  {subLine.itemName} 确认分行
                                </caption>
                                <thead>
                                  <tr className="border-b border-border text-left text-xs text-muted-foreground">
                                    <th className="px-3 py-2 font-medium">
                                      供应商
                                    </th>
                                    <th className="px-3 py-2 font-medium num">
                                      确认数量
                                    </th>
                                    <th className="px-3 py-2 font-medium num">
                                      含税成本
                                    </th>
                                    <th className="px-3 py-2 font-medium num">
                                      进项税率
                                    </th>
                                    <th className="px-3 py-2 font-medium">
                                      预计交期
                                    </th>
                                    <th className="px-3 py-2 font-medium">
                                      履约方式
                                    </th>
                                    <th className="px-3 py-2 font-medium">
                                      资质
                                    </th>
                                    <th className="px-3 py-2 font-medium text-right">
                                      操作
                                    </th>
                                  </tr>
                                </thead>
                                <tbody>
                                  {lines.map((line) => (
                                    <tr
                                      key={line.lineKey}
                                      className="border-b border-border last:border-0"
                                    >
                                      <td className="px-3 py-2">
                                        <Input
                                          id={`line-supplier-${line.lineKey}`}
                                          value={line.supplierName}
                                          onChange={(e) =>
                                            updateLine(line.lineKey, {
                                              supplierName: e.target.value,
                                            })
                                          }
                                          disabled={formalPending}
                                          aria-label={`${subLine.itemName} 供应商`}
                                          className="min-w-[8rem]"
                                        />
                                        <p className="mt-0.5 text-[11px] text-muted-foreground">
                                          {line.capabilitySummary}
                                        </p>
                                      </td>
                                      <td className="px-3 py-2">
                                        <Input
                                          className="num w-20"
                                          inputMode="decimal"
                                          value={line.confirmedQuantity}
                                          onChange={(e) =>
                                            updateLine(line.lineKey, {
                                              confirmedQuantity: e.target.value,
                                            })
                                          }
                                          disabled={formalPending}
                                          aria-label={`${line.supplierName} 确认数量`}
                                        />
                                      </td>
                                      <td className="px-3 py-2">
                                        <Input
                                          className="num w-24"
                                          inputMode="decimal"
                                          value={line.latestCostGross}
                                          onChange={(e) =>
                                            updateLine(line.lineKey, {
                                              latestCostGross: e.target.value,
                                            })
                                          }
                                          disabled={formalPending}
                                          aria-label="最新含税成本"
                                        />
                                      </td>
                                      <td className="px-3 py-2">
                                        <Input
                                          className="num w-16"
                                          inputMode="decimal"
                                          value={line.inputTaxRate}
                                          onChange={(e) =>
                                            updateLine(line.lineKey, {
                                              inputTaxRate: e.target.value,
                                            })
                                          }
                                          disabled={formalPending}
                                          aria-label="进项税率"
                                        />
                                      </td>
                                      <td className="px-3 py-2">
                                        <Input
                                          type="date"
                                          className="w-[9.5rem]"
                                          value={line.expectedDeliveryDate}
                                          onChange={(e) =>
                                            updateLine(line.lineKey, {
                                              expectedDeliveryDate:
                                                e.target.value,
                                            })
                                          }
                                          disabled={formalPending}
                                          aria-label="预计交期"
                                        />
                                      </td>
                                      <td className="px-3 py-2">
                                        <Select
                                          value={line.fulfillmentMode}
                                          onValueChange={(value) => {
                                            if (!value) return
                                            updateLine(line.lineKey, {
                                              fulfillmentMode:
                                                value as FulfillmentMode,
                                            })
                                          }}
                                          disabled={formalPending}
                                        >
                                          <SelectTrigger
                                            size="sm"
                                            aria-label="履约方式"
                                          >
                                            <SelectValue />
                                          </SelectTrigger>
                                          <SelectContent>
                                            {(
                                              Object.keys(
                                                FULFILLMENT_MODE_LABEL
                                              ) as FulfillmentMode[]
                                            ).map((mode) => (
                                              <SelectItem
                                                key={mode}
                                                value={mode}
                                              >
                                                {FULFILLMENT_MODE_LABEL[mode]}
                                              </SelectItem>
                                            ))}
                                          </SelectContent>
                                        </Select>
                                      </td>
                                      <td className="px-3 py-2">
                                        <BusinessStatusBadge
                                          context="list"
                                          label={
                                            line.qualificationStatus ===
                                            "VALID"
                                              ? "有效"
                                              : line.qualificationStatus ===
                                                  "EXPIRING"
                                                ? "即将到期"
                                                : "失效"
                                          }
                                          tone={
                                            line.qualificationStatus ===
                                            "VALID"
                                              ? "success"
                                              : line.qualificationStatus ===
                                                  "EXPIRING"
                                                ? "warning"
                                                : "destructive"
                                          }
                                        />
                                      </td>
                                      <td className="px-3 py-2 text-right">
                                        <Button
                                          type="button"
                                          size="sm"
                                          variant="ghost"
                                          disabled={
                                            formalPending ||
                                            lines.length <= 1
                                          }
                                          onClick={() =>
                                            removeLine(line.lineKey)
                                          }
                                        >
                                          删除
                                        </Button>
                                      </td>
                                    </tr>
                                  ))}
                                </tbody>
                              </table>
                            </div>

                            <div className="border-t border-border px-3 py-2">
                              <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={formalPending}
                                onClick={() =>
                                  addSplitLine(subLine.submissionLineId)
                                }
                              >
                                <PlusIcon
                                  data-icon="inline-start"
                                  aria-hidden="true"
                                />
                                拆分供应商
                              </Button>
                            </div>
                          </div>
                        )
                      })}
                    </div>
                  </section>

                  {saveMessage ? (
                    <p className="text-xs text-muted-foreground" role="status">
                      {saveMessage}
                      {dirty ? " · 之后有未保存修改" : null}
                    </p>
                  ) : dirty ? (
                    <p className="text-xs text-amber-600 dark:text-amber-400" role="status">
                      有未保存的确认分行修改（⌘S 保存）
                    </p>
                  ) : null}
                </CardContent>
              </Card>
            </div>

            {/* 决策摘要：桌面 sticky */}
            <aside className="space-y-4 xl:sticky xl:top-4 xl:self-start">
              <Card size="sm">
                <CardHeader className="border-b">
                  <CardTitle>决策摘要</CardTitle>
                  <CardDescription>
                    数量覆盖按明细独立展示，不可跨行抵消。
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4">
                  <ul className="space-y-2" aria-label="逐明细数量覆盖">
                    {coverage.map((c) => (
                      <li
                        key={c.submissionLineId}
                        className="flex items-start justify-between gap-2 text-sm"
                      >
                        <span className="min-w-0 truncate">{c.itemName}</span>
                        <span
                          className={
                            c.complete
                              ? "num shrink-0 text-emerald-600 dark:text-emerald-400"
                              : "num shrink-0 text-destructive"
                          }
                        >
                          {c.confirmed}/{c.required}
                          {!c.complete ? ` 缺${c.gap}` : ""}
                        </span>
                      </li>
                    ))}
                  </ul>
                  <Separator />
                  <dl className="space-y-2 text-sm">
                    <div className="flex justify-between gap-2">
                      <dt className="text-muted-foreground">预计采购含税</dt>
                      <dd className="num font-medium">
                        {money.format(Number(estimatedPurchase))}
                      </dd>
                    </div>
                    <div className="flex justify-between gap-2">
                      <dt className="text-muted-foreground">销售含税</dt>
                      <dd className="num">
                        {money.format(
                          Number(task.salesSubmission.grossAmount)
                        )}
                      </dd>
                    </div>
                    <div className="flex justify-between gap-2">
                      <dt className="text-muted-foreground">供应商数</dt>
                      <dd className="num">
                        {
                          new Set(lineDrafts.map((l) => l.supplierId)).size
                        }{" "}
                        家
                      </dd>
                    </div>
                  </dl>
                  {clientBlocking.length > 0 ? (
                    <ValidationSummary
                      title="通过前须补齐"
                      issues={clientBlocking}
                    />
                  ) : (
                    <p className="flex items-center gap-2 text-sm text-emerald-600 dark:text-emerald-400">
                      <CircleCheckIcon
                        className="size-4"
                        aria-hidden="true"
                      />
                      当前编辑态覆盖完整（最终以服务端重验为准）
                    </p>
                  )}
                  {(task.decisionSummary.warnings.length > 0
                    ? task.decisionSummary.warnings
                    : []
                  ).map((w) => (
                    <p
                      key={w.code + (w.lineId ?? "")}
                      className="text-xs text-muted-foreground"
                    >
                      警告：{w.message}
                    </p>
                  ))}
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader className="border-b">
                  <CardTitle>打开销售单中心</CardTitle>
                  <CardDescription>
                    深挖后返回仍恢复队列位置、筛选与当前项。
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <Button
                    variant="outline"
                    className="w-full"
                    render={
                      <Link
                        href={w05Href(
                          task.salesSubmission.salesOrderId,
                          returnTo,
                          task.workItemId
                        )}
                      />
                    }
                  >
                    <FileSearchIcon
                      data-icon="inline-start"
                      aria-hidden="true"
                    />
                    打开 W05 · {task.salesSubmission.salesOrderNo}
                  </Button>
                </CardContent>
              </Card>
            </aside>
          </div>

          {/* 底栏正式动作 */}
          <div
            className="sticky bottom-0 z-10 -mx-4 flex flex-wrap items-center justify-end gap-2 border-t border-border bg-background/95 px-4 py-3 backdrop-blur md:-mx-5 md:px-5"
            role="region"
            aria-label="正式动作"
          >
            <Button
              type="button"
              variant="outline"
              onClick={() => void handleSave()}
              disabled={formalPending || !dirty}
            >
              保存确认数据
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => void handleDefer()}
              disabled={formalPending || lastResult?.status === "unknown"}
            >
              <PauseIcon data-icon="inline-start" aria-hidden="true" />
              暂挂
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={() => setRejectOpen(true)}
              disabled={formalPending || lastResult?.status === "unknown"}
            >
              <XIcon data-icon="inline-start" aria-hidden="true" />
              驳回
            </Button>
            <Button
              type="button"
              onClick={() => {
                setAdvanceAfterConfirm(autoNext)
                setConfirmOpen(true)
              }}
              disabled={
                formalPending ||
                !allCovered ||
                lastResult?.status === "unknown"
              }
            >
              <CheckIcon data-icon="inline-start" aria-hidden="true" />
              确认通过并使销售单生效
            </Button>
          </div>

          <FormalActionConfirmDialog
            open={confirmOpen}
            onOpenChange={setConfirmOpen}
            title="确认通过采购二次确认"
            actionLabel="通过并使销售单生效"
            confirmLabel={
              advanceAfterConfirm
                ? "确认通过并打开下一条"
                : "确认通过"
            }
            fromStatus={{ label: "待二次确认", tone: "warning" }}
            toStatus={{ label: "销售已生效", tone: "success" }}
            lockedFields={[
              `submissionId ${task.salesSubmission.submissionId}`,
              `subjectHash ${task.salesSubmission.subjectHashSummary}`,
              "确认分行供应商/数量/成本/交期",
            ]}
            effects={[
              "形成采购确认通过事实与确认分行",
              "销售提交原样形成正式版本并生效、形成应收",
              "完成当前 PROCUREMENT_CONFIRMATION 任务",
              "生成不可变采购创建依据（不创建采购建单任务）",
            ]}
            nextDepartment="采购建单（W08 读取创建依据）"
            pending={completeMutation.isPending}
            onConfirm={handleApprove}
          />

          <Dialog open={rejectOpen} onOpenChange={setRejectOpen}>
            <DialogContent className="sm:max-w-lg">
              <DialogHeader>
                <DialogTitle>驳回采购二次确认</DialogTitle>
                <DialogDescription>
                  将形成本次确认的正式 REJECTED 结论并完成当前任务；不创建采购单、变更单或后继任务。销售只能走固定三路。
                </DialogDescription>
              </DialogHeader>
              <form
                onSubmit={(event) => {
                  event.preventDefault()
                  void rejectForm.handleSubmit()
                }}
                className="space-y-4"
              >
                <div className="space-y-2">
                  <Label htmlFor="reject-reason-code">驳回原因</Label>
                  <rejectForm.AppField name="reasonCode">
                    {(field) => (
                      <Select
                        value={field.state.value}
                        onValueChange={(value) => {
                          if (value) field.handleChange(value as RejectReasonCode)
                        }}
                      >
                        <SelectTrigger id="reject-reason-code" className="w-full">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          {(
                            Object.keys(
                              REJECT_REASON_LABEL
                            ) as RejectReasonCode[]
                          ).map((code) => (
                            <SelectItem key={code} value={code}>
                              {REJECT_REASON_LABEL[code]}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    )}
                  </rejectForm.AppField>
                </div>
                <rejectForm.AppField name="comment">
                  {(field) => (
                    <field.TextareaField
                      label="补充说明"
                      placeholder="请说明无法履约、成本、交期或资质等问题"
                      rows={4}
                    />
                  )}
                </rejectForm.AppField>
                <div className="rounded-lg border border-border bg-muted/40 p-3 text-xs text-muted-foreground">
                  <p className="mb-2 font-medium text-foreground">
                    销售后续固定出路（驳回后只读展示）
                  </p>
                  <ol className="list-decimal space-y-1 pl-4">
                    {NEXT_SALES_RESOLUTION_COPY.map((item) => (
                      <li key={item.code}>{item.title}</li>
                    ))}
                  </ol>
                </div>
                <DialogFooter>
                  <DialogClose
                    render={<Button type="button" variant="outline" />}
                  >
                    取消
                  </DialogClose>
                  <rejectForm.AppForm>
                    <rejectForm.SubmitButton
                      label="确认驳回并完成任务"
                      pendingLabel="正在驳回"
                      variant="destructive"
                    />
                  </rejectForm.AppForm>
                </DialogFooter>
              </form>
            </DialogContent>
          </Dialog>
        </>
      ) : null}
    </div>
  )
}

function buildResultFacts(
  outcome: FormalOutcome | undefined,
  context:
    | {
        position: number
        total: number
      }
    | undefined
) {
  if (!outcome) {
    return [
      {
        label: "队列位置",
        value: context ? `第 ${context.position}/${context.total}` : "—",
      },
    ]
  }
  if (outcome.kind === "APPROVED_AND_SALES_EFFECTIVE") {
    return [
      { label: "销售单", value: outcome.salesOrderNo },
      {
        label: "submissionId",
        value: (
          <span className="num font-mono text-xs">{outcome.submissionId}</span>
        ),
      },
      {
        label: "subjectHash",
        value: (
          <span className="num font-mono text-xs">
            {shortHash(outcome.subjectHash)}
          </span>
        ),
      },
      {
        label: "采购创建依据",
        value: (
          <span className="num font-mono text-xs">
            {outcome.procurementCreationBasisId}
          </span>
        ),
      },
      {
        label: "销售正式版本",
        value: (
          <span className="num font-mono text-xs">
            {outcome.salesOrderRevisionId}
          </span>
        ),
      },
    ]
  }
  if (outcome.kind === "REJECTED_TO_SALES") {
    return [
      { label: "销售单", value: outcome.salesOrderNo },
      {
        label: "驳回提交",
        value: (
          <span className="num font-mono text-xs">
            {outcome.rejectedSubmissionId}
          </span>
        ),
      },
      {
        label: "驳回指纹",
        value: (
          <span className="num font-mono text-xs">
            {shortHash(outcome.rejectedSubjectHash)}
          </span>
        ),
      },
      {
        label: "后继任务",
        value: "无（本事务不创建任何 work_item）",
      },
      {
        label: "驳回原因",
        value: `${REJECT_REASON_LABEL[outcome.rejectReasonCode]} · ${outcome.comment}`,
      },
    ]
  }
  return [
    {
      label: "任务状态",
      value: outcome.workItemStatus,
    },
    {
      label: "租约处置",
      value: outcome.leaseDisposition === "RELEASED" ? "已释放" : "保留",
    },
  ]
}

function ResultActions({
  lastResult,
  taskSalesOrderId,
  returnTo,
  onNext,
  onResolveUnknown,
  onSettleUnknown,
  resolving,
}: {
  lastResult: NonNullable<ResultState>
  taskSalesOrderId?: string
  returnTo: string
  onNext: () => void
  onResolveUnknown: () => void
  onSettleUnknown: () => void
  resolving: boolean
}) {
  if (lastResult.status === "unknown") {
    return (
      <>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={resolving}
          onClick={onResolveUnknown}
        >
          查询最终结果
        </Button>
        <Button
          type="button"
          size="sm"
          disabled={resolving}
          onClick={onSettleUnknown}
        >
          同幂等键完成（演示）
        </Button>
      </>
    )
  }
  return (
    <>
      {taskSalesOrderId ? (
        <Button
          type="button"
          size="sm"
          variant="outline"
          render={
            <Link href={w05Href(taskSalesOrderId, returnTo)} />
          }
        >
          打开销售单中心
        </Button>
      ) : null}
      {lastResult.outcome?.kind === "APPROVED_AND_SALES_EFFECTIVE" ? (
        <Button
          type="button"
          size="sm"
          variant="outline"
          render={
            <Link
              href={`/procurement/orders?basisId=${encodeURIComponent(lastResult.outcome.procurementCreationBasisId)}`}
            />
          }
        >
          用创建依据建采购单
        </Button>
      ) : null}
      {lastResult.stayOnItem !== false || lastResult.status === "blocked" ? (
        <Button type="button" size="sm" onClick={onNext}>
          打开下一条
          <ArrowRightIcon data-icon="inline-end" aria-hidden="true" />
        </Button>
      ) : null}
    </>
  )
}

function ProcurementRejectionNextSteps({
  salesOrderId,
  returnTo,
  rejectedSubmissionId,
  rejectedSubjectHash,
}: {
  salesOrderId: string
  returnTo: string
  rejectedSubmissionId: string
  rejectedSubjectHash: string
}) {
  return (
    <Card size="sm" className="mt-3">
      <CardHeader className="border-b">
        <CardTitle>销售固定三条出路</CardTitle>
        <CardDescription>
          旧任务已 COMPLETED；W07 只读展示出路，不代销售选择。驳回提交{" "}
          <span className="num font-mono">{rejectedSubmissionId}</span> /{" "}
          <span className="num font-mono">
            {shortHash(rejectedSubjectHash)}
          </span>
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <ol className="list-decimal space-y-3 pl-5 text-sm">
          {NEXT_SALES_RESOLUTION_COPY.map((item) => (
            <li key={item.code}>
              <p className="font-medium">{item.title}</p>
              <p className="text-muted-foreground">{item.description}</p>
            </li>
          ))}
        </ol>
        <Button
          render={
            <Link
              href={w05Href(salesOrderId, returnTo)}
            />
          }
        >
          打开 W05 驳回处理
          <ArrowRightIcon data-icon="inline-end" aria-hidden="true" />
        </Button>
      </CardContent>
    </Card>
  )
}
