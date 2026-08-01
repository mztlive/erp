"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  CircleCheckIcon,
  PauseIcon,
  ReceiptIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react"
import { z } from "zod"

import {
  AllocationWorkspace,
  BusinessDiffPanel,
  BusinessEmptyState,
  BusinessStatusBadge,
  DataFreshness,
  DocumentSummary,
  FormalActionConfirmDialog,
  FormalActionResult,
  MetricItem,
  MetricStrip,
  PageHeader,
  SequentialProcessBar,
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
  AllocationDraftLine,
  ApproveConclusion,
  CardFundsReviewDecision,
  FormalOutcome,
  RejectReasonCode,
  ReviewType,
} from "@/features/card-funds-review/types"
import {
  APPROVE_CONCLUSION_LABEL,
  REJECT_FOLLOW_UP_COLLABORATION,
  REJECT_REASON_LABEL,
  REVIEW_TYPE_LABEL,
  WORK_ITEM_TYPE_LABEL,
} from "@/features/card-funds-review/types"
import {
  useCardFundsReviewQueueQuery,
  useClaimCardFundsMutation,
  useCompleteCardFundsMutation,
  useDemoDriftHashMutation,
  useHoldCardFundsMutation,
  useRegisterInvoiceMutation,
  useRegisterReceiptMutation,
  useResolveUnknownCardFundsMutation,
  useSaveCardFundsEvidenceMutation,
} from "@/features/card-funds-review/queries"

const money = new Intl.NumberFormat("zh-CN", {
  style: "currency",
  currency: "CNY",
  minimumFractionDigits: 2,
})

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

type ConfirmMode =
  | { kind: "approve"; conclusion: ApproveConclusion; advance: boolean }
  | { kind: "zero"; advance: boolean }
  | { kind: "reject" }
  | { kind: "hold" }
  | null

function shortHash(hash: string) {
  if (hash.length <= 20) return hash
  return `${hash.slice(0, 12)}…${hash.slice(-6)}`
}

function formatMoney(value: string) {
  return money.format(Number(value) || 0)
}

const rejectSchema = z.object({
  reasonCode: z.enum([
    "EVIDENCE_INSUFFICIENT",
    "FACTS_MISMATCH",
    "COUNTERPARTY_UNCLEAR",
    "OTHER",
  ]),
  comment: z.string().trim().min(5, "请填写至少 5 个字的驳回说明"),
})

export function CardFundsReviewPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const scope: "mine" | "role_pool" =
    searchParams.get("scope") === "role_pool" ? "role_pool" : "mine"
  const typeParam = searchParams.get("type")
  const type: "all" | "opening" | "delta" =
    typeParam === "opening" || typeParam === "delta" ? typeParam : "all"
  const statusParam = searchParams.get("status")
  const status: "pending" | "held" =
    statusParam === "held" ? "held" : "pending"
  const dueParam = searchParams.get("due")
  const due: "all" | "today" | "overdue" =
    dueParam === "today" || dueParam === "overdue" ? dueParam : "all"
  const q = searchParams.get("q") ?? undefined
  const currentWorkItemId =
    searchParams.get("currentWorkItemId") ?? undefined
  const queueContextId =
    searchParams.get("queueContextId") ??
    `queue:card-funds-review:${scope}`

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
      type,
      status,
      due,
      q,
      currentWorkItemId,
      queueContextId,
    }),
    [scope, type, status, due, q, currentWorkItemId, queueContextId]
  )

  const queueQuery = useCardFundsReviewQueueQuery(filters)
  const claimMutation = useClaimCardFundsMutation()
  const completeMutation = useCompleteCardFundsMutation()
  const holdMutation = useHoldCardFundsMutation()
  const registerReceiptMutation = useRegisterReceiptMutation()
  const registerInvoiceMutation = useRegisterInvoiceMutation()
  const saveEvidenceMutation = useSaveCardFundsEvidenceMutation()
  const resolveUnknownMutation = useResolveUnknownCardFundsMutation()
  const driftMutation = useDemoDriftHashMutation()

  const view = queueQuery.data
  const tasks = view?.tasks ?? []
  const context = view?.context
  const task =
    tasks.find((t) => t.workItem.workItemId === currentWorkItemId) ??
    view?.current ??
    tasks[0]
  const currentIndex = task
    ? Math.max(
        0,
        tasks.findIndex((t) => t.workItem.workItemId === task.workItem.workItemId)
      )
    : 0
  const completed = Boolean(view) && tasks.length === 0

  const [confirmMode, setConfirmMode] = React.useState<ConfirmMode>(null)
  const [lastResult, setLastResult] = React.useState<ResultState>(null)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [forceUnknownOnce, setForceUnknownOnce] = React.useState(false)
  const [simulateHashDrift, setSimulateHashDrift] = React.useState(false)
  const [allocationMode, setAllocationMode] = React.useState<
    null | "receipt" | "invoice"
  >(null)
  const [evidenceRef, setEvidenceRef] = React.useState("")
  const [evidenceDocId, setEvidenceDocId] = React.useState("")
  const [comment, setComment] = React.useState("")
  const [receiptForm, setReceiptForm] = React.useState({
    receiptNo: "",
    receivedAt: "2026-07-01",
    grossAmount: "",
  })
  const [invoiceForm, setInvoiceForm] = React.useState({
    invoiceNo: "",
    issuedAt: "2026-07-01",
    grossAmount: "",
    netAmount: "",
    taxAmount: "",
  })
  const [allocLines, setAllocLines] = React.useState<AllocationDraftLine[]>([])

  const headingRef = React.useRef<HTMLHeadingElement>(null)
  const resultRef = React.useRef<HTMLDivElement>(null)
  const leaseRef = React.useRef<SessionLease | null>(null)
  const [leaseEpoch, setLeaseEpoch] = React.useState(0)
  const idempotencyRef = React.useRef<{
    approve?: string
    reject?: string
    hold?: string
    zero?: string
  }>({})

  React.useEffect(() => {
    if (!task) return
    setEvidenceRef(task.currentEvidence.evidenceReferences[0] ?? "")
    setEvidenceDocId(task.currentEvidence.evidenceDocumentIds[0] ?? "")
    setComment(task.currentEvidence.comment ?? "")
    setActionError(null)
    setAllocationMode(null)
    idempotencyRef.current = {}
  }, [task?.workItem.workItemId, task?.fundsFactVersion])

  // URL 默认：保留 queueContextId / type / scope / currentWorkItemId
  React.useEffect(() => {
    if (queueQuery.isPending || !view) return
    const hasScope = searchParams.has("scope")
    const hasType = searchParams.has("type")
    const hasItem = searchParams.has("currentWorkItemId")
    const hasCtx = searchParams.has("queueContextId")
    if (hasScope && hasType && hasCtx && (hasItem || tasks.length === 0)) return
    const params = new URLSearchParams(searchParams.toString())
    if (!hasScope) params.set("scope", scope)
    if (!hasType) params.set("type", type)
    if (!hasCtx) params.set("queueContextId", queueContextId)
    if (!hasItem && task) {
      params.set("currentWorkItemId", task.workItem.workItemId)
    }
    const qs = params.toString()
    router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
  }, [
    queueQuery.isPending,
    view,
    searchParams,
    scope,
    type,
    queueContextId,
    task,
    tasks.length,
    pathname,
    router,
  ])

  // 自动领取
  React.useEffect(() => {
    if (!task) return
    if (leaseRef.current?.workItemId === task.workItem.workItemId) return
    if (claimMutation.isPending) return
    let cancelled = false
    void claimMutation
      .mutateAsync(task.workItem.workItemId)
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
        /* 保持未领取 */
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅任务切换时领取
  }, [task?.workItem.workItemId])

  // 焦点：结果区 / 对象标题；位置播报由 SequentialProcessBar aria-live
  React.useEffect(() => {
    if (lastResult) {
      resultRef.current?.focus()
    } else if (task) {
      headingRef.current?.focus()
    }
  }, [task?.workItem.workItemId, lastResult?.status])

  const replaceUrl = React.useCallback(
    (patch: Record<string, string | null | undefined>) => {
      const params = new URLSearchParams(searchParams.toString())
      for (const [key, value] of Object.entries(patch)) {
        if (value == null || value === "") params.delete(key)
        else params.set(key, value)
      }
      // 跨 W05/W11 返回时不丢 queueContextId
      if (!params.has("queueContextId")) {
        params.set("queueContextId", queueContextId)
      }
      const qs = params.toString()
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    },
    [pathname, queueContextId, router, searchParams]
  )

  const goToWorkItem = React.useCallback(
    (workItemId: string | undefined | null) => {
      setLastResult(null)
      setActionError(null)
      replaceUrl({
        currentWorkItemId: workItemId ?? null,
        queueContextId,
      })
    },
    [queueContextId, replaceUrl]
  )

  const neighborId = React.useCallback(
    (delta: number) => {
      const idx = currentIndex + delta
      return tasks[idx]?.workItem.workItemId
    },
    [currentIndex, tasks]
  )

  const activeLease =
    leaseRef.current?.workItemId === task?.workItem.workItemId
      ? leaseRef.current
      : null
  void leaseEpoch

  const leaseStatus: "active" | "unclaimed" | "lost" | "expiring" = activeLease
    ? "active"
    : "unclaimed"
  const leaseLabel = activeLease
    ? `已领取 · 至 ${new Date(activeLease.expiresAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}`
    : "未领取"

  const ensureLease = React.useCallback(async () => {
    if (!task) throw new Error("无当前任务")
    if (
      leaseRef.current?.workItemId === task.workItem.workItemId &&
      leaseRef.current.claimToken
    ) {
      return leaseRef.current
    }
    const lease = await claimMutation.mutateAsync(task.workItem.workItemId)
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

  const buildDecisionBase = React.useCallback(
    (reviewResult: "APPROVED" | "REJECTED") => {
      if (!task) throw new Error("无当前任务")
      const evidenceDocumentIds = evidenceDocId.trim()
        ? [evidenceDocId.trim()]
        : []
      const evidenceReferences = evidenceRef.trim() ? [evidenceRef.trim()] : []
      return {
        receivableAccountId: task.account.id,
        expectedAccountSeq: task.account.accountSeq,
        expectedAccountDomainVersion: task.account.domainVersion,
        expectedReviewChainTailId: task.reviewChain.tailReviewId,
        expectedReviewChainVersion: task.reviewChain.chainVersion,
        expectedNextReviewNo: task.reviewChain.nextReviewNo,
        expectedSalesOrderRevisionId: task.currentSalesOrderRevisionId,
        expectedFundsFactVersion: task.fundsFactVersion,
        reviewType: task.reviewType as ReviewType,
        evidenceDocumentIds,
        evidenceReferences,
        comment: comment.trim() || undefined,
        expectedSubjectHash: task.workItem.subjectHash,
        reviewResult,
      }
    },
    [comment, evidenceDocId, evidenceRef, task]
  )

  const advanceIfNeeded = React.useCallback(
    (shouldAdvance: boolean) => {
      if (!shouldAdvance) return
      const nextId =
        context?.nextWorkItemId ??
        neighborId(1) ??
        tasks.find((t) => t.workItem.workItemId !== task?.workItem.workItemId)
          ?.workItem.workItemId
      leaseRef.current = null
      setLeaseEpoch((n) => n + 1)
      if (nextId) goToWorkItem(nextId)
      else replaceUrl({ currentWorkItemId: null, queueContextId })
    },
    [
      context?.nextWorkItemId,
      goToWorkItem,
      neighborId,
      queueContextId,
      replaceUrl,
      task?.workItem.workItemId,
      tasks,
    ]
  )

  const runApprove = React.useCallback(
    async (conclusion: ApproveConclusion, advance: boolean) => {
      if (!task) return
      setActionError(null)
      try {
        const lease = await ensureLease()
        const keyField = conclusion === "NO_HISTORY_FROM_ZERO" ? "zero" : "approve"
        if (!idempotencyRef.current[keyField]) {
          idempotencyRef.current[keyField] =
            `${keyField}_${task.workItem.workItemId}_${crypto.randomUUID()}`
        }
        const base = buildDecisionBase("APPROVED")
        const decision: CardFundsReviewDecision = {
          ...base,
          reviewResult: "APPROVED",
          conclusion,
        }
        const response = await completeMutation.mutateAsync({
          workItemId: task.workItem.workItemId,
          claimToken: lease.claimToken,
          leaseVersion: lease.leaseVersion,
          expectedSubjectHash: task.workItem.subjectHash,
          expectedSubjectVersion: task.workItem.subjectVersion,
          idempotencyKey: idempotencyRef.current[keyField]!,
          forceUnknown: forceUnknownOnce,
          simulateHashDrift,
          decision,
        })
        setForceUnknownOnce(false)
        setSimulateHashDrift(false)
        setConfirmMode(null)

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
        if (response.outcome.kind !== "APPROVED") return
        const biz = response.outcome.business
        leaseRef.current = null
        setLeaseEpoch((n) => n + 1)
        setLastResult({
          status: "succeeded",
          title: `复核通过 · 复核号 ${biz.reviewNo}`,
          description: `${APPROVE_CONCLUSION_LABEL[biz.conclusion as ApproveConclusion]} · workflow ${biz.workflowActionId} · 先固定展示本结果再${advance && autoNext ? "自动下一项" : "手动继续"}`,
          reference: biz.reference,
          outcome: response.outcome,
          stayOnItem: !(advance && autoNext),
        })
        // 成功先展示固定复核号；若 autoNext 则短暂停留后前进
        if (advance && autoNext) {
          window.setTimeout(() => advanceIfNeeded(true), 600)
        }
      } catch (error) {
        setActionError(error instanceof Error ? error.message : "完成失败")
      }
    },
    [
      advanceIfNeeded,
      autoNext,
      buildDecisionBase,
      completeMutation,
      ensureLease,
      forceUnknownOnce,
      simulateHashDrift,
      task,
    ]
  )

  const rejectForm = useAppForm({
    defaultValues: {
      reasonCode: "EVIDENCE_INSUFFICIENT" as RejectReasonCode,
      comment: "",
    },
    validators: { onChange: rejectSchema },
    onSubmit: async ({ value }) => {
      if (!task) return
      setActionError(null)
      try {
        const lease = await ensureLease()
        if (!idempotencyRef.current.reject) {
          idempotencyRef.current.reject = `reject_${task.workItem.workItemId}_${crypto.randomUUID()}`
        }
        const base = buildDecisionBase("REJECTED")
        const decision: CardFundsReviewDecision = {
          ...base,
          reviewResult: "REJECTED",
          conclusion: "REJECTED",
          reasonCode: value.reasonCode as RejectReasonCode,
          comment: value.comment.trim(),
          evidenceDocumentIds:
            base.evidenceDocumentIds.length > 0
              ? base.evidenceDocumentIds
              : ["doc_reject_note"],
          evidenceReferences:
            base.evidenceReferences.length > 0
              ? base.evidenceReferences
              : [`驳回说明:${value.comment.trim().slice(0, 40)}`],
        }
        const response = await completeMutation.mutateAsync({
          workItemId: task.workItem.workItemId,
          claimToken: lease.claimToken,
          leaseVersion: lease.leaseVersion,
          expectedSubjectHash: task.workItem.subjectHash,
          expectedSubjectVersion: task.workItem.subjectVersion,
          idempotencyKey: idempotencyRef.current.reject,
          decision,
        })
        setConfirmMode(null)
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
        if (response.outcome.kind !== "REJECTED") return
        const biz = response.outcome.business
        leaseRef.current = null
        setLeaseEpoch((n) => n + 1)
        setLastResult({
          status: "rejected",
          title: `已驳回 · 复核号 ${biz.reviewNo}`,
          description: `${REJECT_FOLLOW_UP_COLLABORATION}`,
          reference: biz.reference,
          outcome: response.outcome,
          stayOnItem: !autoNext,
        })
        if (autoNext) {
          window.setTimeout(() => advanceIfNeeded(true), 800)
        }
      } catch (error) {
        setActionError(error instanceof Error ? error.message : "驳回失败")
      }
    },
  })

  const handleHold = React.useCallback(async () => {
    if (!task) return
    setActionError(null)
    try {
      const lease = await ensureLease()
      if (!idempotencyRef.current.hold) {
        idempotencyRef.current.hold = `hold_${task.workItem.workItemId}_${crypto.randomUUID()}`
      }
      const nextId = neighborId(1)
      const response = await holdMutation.mutateAsync({
        workItemId: task.workItem.workItemId,
        claimToken: lease.claimToken,
        leaseVersion: lease.leaseVersion,
        reasonCode: "NEED_MORE_EVIDENCE",
        note: comment.trim() || "暂挂：待补充票款证据",
        idempotencyKey: idempotencyRef.current.hold,
        nextWorkItemId: nextId,
      })
      setConfirmMode(null)
      if (response.status !== "succeeded" || response.outcome.kind !== "HELD") {
        if (response.status === "failed") setActionError(response.message)
        return
      }
      leaseRef.current = null
      setLeaseEpoch((n) => n + 1)
      setLastResult({
        status: "blocked",
        title: "当前项已暂挂 · 仍在有效队列",
        description: response.outcome.resumeHint,
        reference: response.outcome.reference,
        outcome: response.outcome,
      })
      // 暂挂：手动浏览下一项，不冒充任务完成；不自动「完成成功」语义
      if (nextId) goToWorkItem(nextId)
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "暂挂失败")
    }
  }, [comment, ensureLease, goToWorkItem, holdMutation, neighborId, task])

  const openAllocation = React.useCallback(
    (mode: "receipt" | "invoice") => {
      if (!task) return
      setAllocationMode(mode)
      setAllocLines([
        {
          lineId: "al_1",
          targetAccountId: task.account.id,
          targetLabel: `${task.salesOrder.orderNo} · ${task.account.customerName}`,
          amount: mode === "receipt" ? receiptForm.grossAmount || "0.00" : invoiceForm.grossAmount || "0.00",
        },
      ])
    },
    [invoiceForm.grossAmount, receiptForm.grossAmount, task]
  )

  const submitReceipt = React.useCallback(async () => {
    if (!task) return
    setActionError(null)
    try {
      const lease = await ensureLease()
      const result = await registerReceiptMutation.mutateAsync({
        workItemId: task.workItem.workItemId,
        claimToken: lease.claimToken,
        leaseVersion: lease.leaseVersion,
        receiptNo: receiptForm.receiptNo.trim() || `SK-W13-${Date.now().toString(36)}`,
        receivedAt: receiptForm.receivedAt,
        grossAmount: receiptForm.grossAmount,
        allocations: allocLines,
        evidenceReference: evidenceRef.trim() || "银行回单-会话登记",
        idempotencyKey: `rcpt_${task.workItem.workItemId}_${crypto.randomUUID()}`,
      })
      // 登记后停留当前项，刷新金额/指纹（invalidate 后 query 更新）
      setAllocationMode(null)
      setLastResult({
        status: "succeeded",
        title: "历史回款已登记",
        description: `已形成正式回款与多对多分配；指纹 ${shortHash(result.subjectHash)}，净已收 ${formatMoney(result.settledTotal)}。未完成复核前指标仍可能不可靠。`,
        reference: result.fundsFactVersion,
        stayOnItem: true,
      })
      // 租约仍有效但 subject 已变：刷新 lease 展示
      leaseRef.current = {
        ...lease,
      }
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "登记回款失败")
    }
  }, [
    allocLines,
    ensureLease,
    evidenceRef,
    receiptForm,
    registerReceiptMutation,
    task,
  ])

  const submitInvoice = React.useCallback(async () => {
    if (!task) return
    setActionError(null)
    try {
      const lease = await ensureLease()
      const gross = invoiceForm.grossAmount
      const net =
        invoiceForm.netAmount ||
        moneyStrSafe(Number(gross) / 1.13)
      const tax =
        invoiceForm.taxAmount ||
        moneyStrSafe(Number(gross) - Number(net))
      const result = await registerInvoiceMutation.mutateAsync({
        workItemId: task.workItem.workItemId,
        claimToken: lease.claimToken,
        leaseVersion: lease.leaseVersion,
        invoiceNo: invoiceForm.invoiceNo.trim() || `FP-W13-${Date.now().toString(36)}`,
        issuedAt: invoiceForm.issuedAt,
        grossAmount: gross,
        netAmount: net,
        taxAmount: tax,
        allocations: allocLines,
        evidenceReference: evidenceRef.trim() || "发票扫描件-会话登记",
        idempotencyKey: `inv_${task.workItem.workItemId}_${crypto.randomUUID()}`,
      })
      setAllocationMode(null)
      setLastResult({
        status: "succeeded",
        title: "历史发票已登记",
        description: `已形成正式发票与分配；指纹 ${shortHash(result.subjectHash)}，净已开票 ${formatMoney(result.invoicedTotal)}。`,
        reference: result.fundsFactVersion,
        stayOnItem: true,
      })
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "登记发票失败")
    }
  }, [
    allocLines,
    ensureLease,
    evidenceRef,
    invoiceForm,
    registerInvoiceMutation,
    task,
  ])

  const saveEvidence = React.useCallback(async () => {
    if (!task) return
    try {
      const lease = await ensureLease()
      await saveEvidenceMutation.mutateAsync({
        workItemId: task.workItem.workItemId,
        claimToken: lease.claimToken,
        leaseVersion: lease.leaseVersion,
        evidenceDocumentIds: evidenceDocId.trim() ? [evidenceDocId.trim()] : [],
        evidenceReferences: evidenceRef.trim() ? [evidenceRef.trim()] : [],
        comment: comment.trim() || undefined,
        idempotencyKey: `ev_${task.workItem.workItemId}_${Date.now()}`,
      })
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "保存证据失败")
    }
  }, [
    comment,
    ensureLease,
    evidenceDocId,
    evidenceRef,
    saveEvidenceMutation,
    task,
  ])

  // 键盘：j/k 导航；⌘↵ 打开正式确认
  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      const tag = target?.tagName
      const inField =
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        target?.isContentEditable

      if (
        (event.metaKey || event.ctrlKey) &&
        event.key === "Enter" &&
        !inField
      ) {
        event.preventDefault()
        if (activeLease && task) {
          const zeroOk =
            task.reviewType === "OPENING" &&
            Number(task.account.settledTotal) === 0 &&
            Number(task.account.invoicedTotal) === 0
          setConfirmMode(
            zeroOk
              ? { kind: "zero", advance: autoNext }
              : {
                  kind: "approve",
                  conclusion: "RECORDED_FACTS_RECONCILED",
                  advance: autoNext,
                }
          )
        }
        return
      }
      if (inField) return
      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault()
        const next = neighborId(1)
        if (next) goToWorkItem(next)
      }
      if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault()
        const prev = neighborId(-1)
        if (prev) goToWorkItem(prev)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [activeLease, autoNext, goToWorkItem, neighborId, task])

  const formalPending =
    completeMutation.isPending ||
    holdMutation.isPending ||
    claimMutation.isPending

  const canConfirmZero =
    task?.reviewType === "OPENING" &&
    Number(task.account.settledTotal) === 0 &&
    Number(task.account.invoicedTotal) === 0

  const w05Href = task
    ? `/sales/orders/${task.salesOrder.id}?from=W13&returnTo=${encodeURIComponent(`${pathname}?${searchParams.toString()}`)}&sourceWorkItemId=${task.workItem.workItemId}`
    : "#"
  const w11Href = task
    ? `/finance/customer-accounts?customer=${task.account.customerId}&from=W13&returnTo=${encodeURIComponent(`${pathname}?queueContextId=${queueContextId}&currentWorkItemId=${task.workItem.workItemId}&type=${type}&scope=${scope}`)}`
    : "/finance/customer-accounts"

  const allocatedSum = allocLines.reduce((s, l) => s + (Number(l.amount) || 0), 0)
  const allocTarget =
    allocationMode === "receipt"
      ? Number(receiptForm.grossAmount) || 0
      : Number(invoiceForm.grossAmount) || 0

  if (queueQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
        <div className="h-24 animate-pulse rounded-2xl bg-muted" />
        <div className="grid gap-4 xl:grid-cols-[minmax(0,64fr)_minmax(16rem,36fr)]">
          <div className="h-80 animate-pulse rounded-2xl bg-muted" />
          <div className="h-64 animate-pulse rounded-2xl bg-muted" />
        </div>
      </div>
    )
  }

  if (queueQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="卡券票款复核" description="队列加载失败" />
        <Button type="button" onClick={() => void queueQuery.refetch()}>
          重试
        </Button>
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="卡券票款复核"
        breadcrumbs={[
          {
            id: "fin",
            label: "财务",
            href: "/finance/card-funds-review",
          },
          { id: "card", label: "卡券票款复核", current: true },
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
              {context?.filterSummary ?? "仅我的"} · 第{" "}
              {context?.position ?? 0}/{context?.total ?? 0} 项
            </span>
          </div>
        }
      />

      <div className="flex flex-wrap items-center gap-3 rounded-2xl border border-border bg-card px-3 py-2 text-sm">
        <ToggleGroup
          value={[type]}
          onValueChange={(v) => {
            const next = (v[0] as typeof type | undefined) ?? "all"
            replaceUrl({ type: next, currentWorkItemId: null })
          }}
          variant="outline"
          size="sm"
          spacing={0}
          aria-label="任务类型"
        >
          <ToggleGroupItem value="all">全部类型</ToggleGroupItem>
          <ToggleGroupItem value="opening">期初 OPENING</ToggleGroupItem>
          <ToggleGroupItem value="delta">差额 SYNC_DELTA</ToggleGroupItem>
        </ToggleGroup>
        <ToggleGroup
          value={[status]}
          onValueChange={(v) => {
            const next = (v[0] as typeof status | undefined) ?? "pending"
            replaceUrl({ status: next === "pending" ? null : next, currentWorkItemId: null })
          }}
          variant="outline"
          size="sm"
          spacing={0}
          aria-label="队列范围"
        >
          <ToggleGroupItem value="pending">待处理</ToggleGroupItem>
          <ToggleGroupItem value="held">已暂挂</ToggleGroupItem>
        </ToggleGroup>
        <div className="flex items-center gap-2">
          <Label htmlFor="auto-next" className="text-muted-foreground">
            自动下一项
          </Label>
          <Switch
            id="auto-next"
            checked={autoNext}
            onCheckedChange={(on) => {
              setSessionAutoNext(on)
              replaceUrl({ autoNext: on ? "1" : "0" })
            }}
          />
        </div>
        <label className="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
          <input
            type="checkbox"
            className="size-3.5"
            checked={forceUnknownOnce}
            onChange={(e) => setForceUnknownOnce(e.target.checked)}
          />
          下次完成模拟结果未知
        </label>
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <input
            type="checkbox"
            className="size-3.5"
            checked={simulateHashDrift}
            onChange={(e) => setSimulateHashDrift(e.target.checked)}
          />
          完成前模拟指纹变化阻断
        </label>
      </div>

      {lastResult ? (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
          <FormalActionResult
            status={lastResult.status}
            title={lastResult.title}
            description={lastResult.description}
            reference={lastResult.reference}
            facts={buildResultFacts(lastResult.outcome)}
            actions={
              <div className="flex flex-wrap gap-2">
                {lastResult.status === "unknown" ? (
                  <>
                    <Button
                      type="button"
                      variant="secondary"
                      disabled={resolveUnknownMutation.isPending}
                      onClick={() => {
                        if (!lastResult.pendingIdempotencyKey) return
                        void resolveUnknownMutation
                          .mutateAsync({
                            idempotencyKey: lastResult.pendingIdempotencyKey,
                            settle: false,
                          })
                          .then((r) => {
                            if (r.status === "succeeded") {
                              setLastResult({
                                status:
                                  r.outcome.kind === "REJECTED"
                                    ? "rejected"
                                    : "succeeded",
                                title: "查询到正式终态",
                                description: "已确认正式结果，可继续下一项。",
                                reference:
                                  r.outcome.kind === "HELD"
                                    ? r.outcome.reference
                                    : r.outcome.business.reference,
                                outcome: r.outcome,
                              })
                            } else if (r.status === "unknown") {
                              setActionError(r.message)
                            }
                          })
                      }}
                    >
                      查询最终结果
                    </Button>
                  </>
                ) : (
                  <Button
                    type="button"
                    onClick={() => {
                      const next =
                        context?.nextWorkItemId ??
                        neighborId(1) ??
                        tasks[0]?.workItem.workItemId
                      goToWorkItem(next)
                    }}
                  >
                    下一项
                  </Button>
                )}
                {task ? (
                  <Button
                    type="button"
                    variant="outline"
                    render={<Link href={w05Href} />}
                  >
                    打开销售单
                  </Button>
                ) : null}
              </div>
            }
          />
          {lastResult.outcome?.kind === "REJECTED" &&
          lastResult.outcome.business.followUpConfiguration ? (
            <Alert className="mt-3" variant="destructive">
              <TriangleAlertIcon aria-hidden="true" />
              <AlertTitle>
                {
                  lastResult.outcome.business.followUpConfiguration
                    .blockerCode
                }
              </AlertTitle>
              <AlertDescription>
                {
                  lastResult.outcome.business.followUpConfiguration
                    .collaborationMessage
                }
              </AlertDescription>
            </Alert>
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
          title="当前筛选项已处理完"
          description="卡券票款复核有效队列已清空。可切换类型/暂挂范围，或返回工作台。"
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
            processLabel="复核通过"
            processNextLabel="通过并打开下一条"
            processDisabled={
              formalPending || Boolean(lastResult?.status === "unknown")
            }
            pending={formalPending}
            onBack={() => router.push("/workspace")}
            onProcess={() => {
              setConfirmMode({
                kind: "approve",
                conclusion: "RECORDED_FACTS_RECONCILED",
                advance: false,
              })
            }}
            onProcessNext={() => {
              setConfirmMode({
                kind: "approve",
                conclusion: "RECORDED_FACTS_RECONCILED",
                advance: true,
              })
            }}
            onReclaim={() => {
              void ensureLease().catch((error) => {
                setActionError(
                  error instanceof Error ? error.message : "领取失败"
                )
              })
            }}
          />

          <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,64fr)_minmax(17rem,36fr)]">
            <div className="min-w-0 space-y-4">
              <Card size="sm">
                <CardHeader className="border-b">
                  <div className="flex flex-wrap items-center gap-2">
                    <CardTitle>
                      <h2
                        ref={headingRef}
                        tabIndex={-1}
                        className="outline-none"
                        aria-live="polite"
                      >
                        {task.salesOrder.orderNo} · {task.account.customerName}
                      </h2>
                    </CardTitle>
                    <BusinessStatusBadge
                      context="list"
                      label={REVIEW_TYPE_LABEL[task.reviewType]}
                      tone={
                        task.reviewType === "OPENING" ? "info" : "warning"
                      }
                    />
                    <Badge variant="secondary">
                      {WORK_ITEM_TYPE_LABEL[task.workItem.workItemType]}
                    </Badge>
                    {task.workItem.held ? (
                      <BusinessStatusBadge
                        context="list"
                        label="已暂挂 · 仍在有效队列"
                        tone="warning"
                      />
                    ) : null}
                  </div>
                  <CardDescription>
                    同步版本 r{task.salesOrder.revisionNo} ·{" "}
                    {task.salesOrder.snapshotAt} ·{" "}
                    {task.account.mallName} · 往来{" "}
                    {task.account.counterpartyPartyName}
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4">
                  <DocumentSummary
                    columns="two"
                    items={[
                      {
                        id: "order",
                        label: "卡券销售单",
                        value: task.salesOrder.orderNo,
                        emphasized: true,
                      },
                      {
                        id: "hash",
                        label: "当前 subject_hash",
                        value: (
                          <span className="num font-mono text-sm">
                            {shortHash(task.workItem.subjectHash)}
                          </span>
                        ),
                        description: task.workItem.subjectHash,
                      },
                      {
                        id: "counterparty",
                        label: "收款/开票往来主体",
                        value: task.account.counterpartyPartyName,
                      },
                      {
                        id: "reason",
                        label: "任务原因",
                        value: task.workItem.reason,
                      },
                    ]}
                  />

                  <MetricStrip columns={5} aria-label="票款事实指标">
                    <MetricItem
                      label="同步成交额"
                      value={formatMoney(task.account.syncedGrossAmount)}
                      detail="商城当前版本"
                    />
                    <MetricItem
                      label="当前应收"
                      value={formatMoney(task.account.grossTotal)}
                      detail={`开放 ${formatMoney(task.account.openTotal)}`}
                    />
                    <MetricItem
                      label="净已收"
                      value={formatMoney(task.account.settledTotal)}
                      detail="APPLY−REVERSE"
                    />
                    <MetricItem
                      label="净已开票"
                      value={formatMoney(task.account.invoicedTotal)}
                      detail={`可开 ${formatMoney(task.account.openInvoiceableTotal)}`}
                    />
                    <MetricItem
                      label="指纹状态"
                      value={task.fingerprintStatus.label}
                      detail={task.fingerprintStatus.detail}
                      status={{
                        label: task.fingerprintStatus.label,
                        tone: task.fingerprintStatus.tone,
                      }}
                    />
                  </MetricStrip>

                  <Alert
                    variant={
                      task.account.fundsReliability === "VERIFIED"
                        ? "default"
                        : "destructive"
                    }
                  >
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>
                      {task.account.fundsReliability ===
                      "UNRELIABLE_PENDING_REVIEW"
                        ? "票款指标不可靠（复核未完成）"
                        : task.account.fundsReliability === "STALE_FINGERPRINT"
                          ? "旧指纹失效 · 指标不可靠"
                          : "可靠性"}
                    </AlertTitle>
                    <AlertDescription>
                      {task.account.reliabilityNote}
                      不以 0 值冒充已核实事实。W11/W15 应展示同等标识。
                    </AlertDescription>
                  </Alert>

                  {task.reviewType === "SYNC_DELTA" && task.difference ? (
                    <BusinessDiffPanel
                      title={task.difference.title}
                      caption="上一有效复核与当前事实对比（服务端投影）"
                      changes={task.difference.changes.map((c) => ({
                        id: c.id,
                        field: c.field,
                        before: c.before,
                        after: c.after,
                        note: [c.note, c.sourceObject, c.occurredAt]
                          .filter(Boolean)
                          .join(" · "),
                      }))}
                    />
                  ) : null}

                  <Card size="sm">
                    <CardHeader className="border-b py-3">
                      <CardTitle className="text-base">
                        正式回款与发票明细
                      </CardTitle>
                      <CardDescription>
                        仅展示 W11 正式事实；登记走多对多分配，禁止累计覆盖字段
                      </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-3 pt-4">
                      {task.receiptFacts.length === 0 &&
                      task.invoiceFacts.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                          尚无正式回款/发票。可登记历史事实，或在期初且净额为 0
                          时确认「从 0 起」（不创建 0 元单据）。
                        </p>
                      ) : null}
                      {task.receiptFacts.map((r) => (
                        <div
                          key={r.receiptId}
                          className="rounded-lg border border-border px-3 py-2 text-sm"
                        >
                          <div className="flex flex-wrap gap-2 font-medium">
                            <ReceiptIcon className="size-4 text-muted-foreground" />
                            回款 {r.receiptNo}
                            {r.reversed ? (
                              <Badge variant="destructive">已冲正</Badge>
                            ) : null}
                          </div>
                          <p className="mt-1 text-muted-foreground">
                            {r.receivedAt} · 含税 {formatMoney(r.grossAmount)} ·
                            分配本应收 {formatMoney(r.allocatedToAccount)}
                            {r.otherAllocationSummary
                              ? ` · ${r.otherAllocationSummary}`
                              : ""}
                          </p>
                        </div>
                      ))}
                      {task.invoiceFacts.map((inv) => (
                        <div
                          key={inv.invoiceId}
                          className="rounded-lg border border-border px-3 py-2 text-sm"
                        >
                          <div className="flex flex-wrap gap-2 font-medium">
                            发票 {inv.invoiceNo}
                            <Badge variant="outline">
                              {inv.direction === "BLUE" ? "蓝字" : "红字"}
                            </Badge>
                            {inv.reversed ? (
                              <Badge variant="destructive">已红冲</Badge>
                            ) : null}
                          </div>
                          <p className="mt-1 text-muted-foreground">
                            {inv.issuedAt} · 含税 {formatMoney(inv.grossAmount)}{" "}
                            · 分配本子账{" "}
                            {formatMoney(inv.allocatedToAccount)}
                          </p>
                        </div>
                      ))}
                      <div className="flex flex-wrap gap-2 pt-1">
                        <Button
                          type="button"
                          variant="secondary"
                          size="sm"
                          onClick={() => openAllocation("receipt")}
                        >
                          登记历史回款
                        </Button>
                        <Button
                          type="button"
                          variant="secondary"
                          size="sm"
                          onClick={() => openAllocation("invoice")}
                        >
                          登记历史发票
                        </Button>
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          render={<Link href={w11Href} />}
                        >
                          打开客户往来 W11
                        </Button>
                      </div>
                    </CardContent>
                  </Card>

                  {allocationMode ? (
                    <div className="space-y-3">
                      <Card size="sm">
                        <CardHeader className="border-b py-3">
                          <CardTitle className="text-base">
                            {allocationMode === "receipt"
                              ? "登记历史回款（W11 内核）"
                              : "登记历史发票（W11 内核）"}
                          </CardTitle>
                          <CardDescription>
                            内嵌 AllocationWorkspace；不写累计已收/已开覆盖字段；禁止 0
                            元单据
                          </CardDescription>
                        </CardHeader>
                        <CardContent className="grid gap-3 pt-4 sm:grid-cols-2">
                          {allocationMode === "receipt" ? (
                            <>
                              <div className="space-y-1.5">
                                <Label htmlFor="rcpt-no">回款单号</Label>
                                <Input
                                  id="rcpt-no"
                                  value={receiptForm.receiptNo}
                                  onChange={(e) =>
                                    setReceiptForm((f) => ({
                                      ...f,
                                      receiptNo: e.target.value,
                                    }))
                                  }
                                  placeholder="可空则系统生成"
                                />
                              </div>
                              <div className="space-y-1.5">
                                <Label htmlFor="rcpt-amt">含税金额</Label>
                                <Input
                                  id="rcpt-amt"
                                  className="num"
                                  value={receiptForm.grossAmount}
                                  onChange={(e) => {
                                    const grossAmount = e.target.value
                                    setReceiptForm((f) => ({
                                      ...f,
                                      grossAmount,
                                    }))
                                    setAllocLines((lines) =>
                                      lines.map((l, i) =>
                                        i === 0
                                          ? { ...l, amount: grossAmount || "0.00" }
                                          : l
                                      )
                                    )
                                  }}
                                  placeholder="须 > 0"
                                />
                              </div>
                              <div className="space-y-1.5">
                                <Label htmlFor="rcpt-at">到账日期</Label>
                                <Input
                                  id="rcpt-at"
                                  type="date"
                                  value={receiptForm.receivedAt}
                                  onChange={(e) =>
                                    setReceiptForm((f) => ({
                                      ...f,
                                      receivedAt: e.target.value,
                                    }))
                                  }
                                />
                              </div>
                            </>
                          ) : (
                            <>
                              <div className="space-y-1.5">
                                <Label htmlFor="inv-no">发票号码</Label>
                                <Input
                                  id="inv-no"
                                  value={invoiceForm.invoiceNo}
                                  onChange={(e) =>
                                    setInvoiceForm((f) => ({
                                      ...f,
                                      invoiceNo: e.target.value,
                                    }))
                                  }
                                />
                              </div>
                              <div className="space-y-1.5">
                                <Label htmlFor="inv-amt">含税金额</Label>
                                <Input
                                  id="inv-amt"
                                  className="num"
                                  value={invoiceForm.grossAmount}
                                  onChange={(e) => {
                                    const grossAmount = e.target.value
                                    setInvoiceForm((f) => ({
                                      ...f,
                                      grossAmount,
                                    }))
                                    setAllocLines((lines) =>
                                      lines.map((l, i) =>
                                        i === 0
                                          ? { ...l, amount: grossAmount || "0.00" }
                                          : l
                                      )
                                    )
                                  }}
                                  placeholder="须 > 0"
                                />
                              </div>
                              <div className="space-y-1.5">
                                <Label htmlFor="inv-at">开票日期</Label>
                                <Input
                                  id="inv-at"
                                  type="date"
                                  value={invoiceForm.issuedAt}
                                  onChange={(e) =>
                                    setInvoiceForm((f) => ({
                                      ...f,
                                      issuedAt: e.target.value,
                                    }))
                                  }
                                />
                              </div>
                            </>
                          )}
                        </CardContent>
                      </Card>

                      <AllocationWorkspace
                        title="多对多分配"
                        description="分配对象与金额由本页受控；差额由调用方展示，组件不重算业务。"
                        summary={{
                          totalToAllocate: formatMoney(
                            moneyStrSafe(allocTarget)
                          ),
                          allocated: formatMoney(moneyStrSafe(allocatedSum)),
                          difference: formatMoney(
                            moneyStrSafe(allocTarget - allocatedSum)
                          ),
                        }}
                        allocations={allocLines}
                        getRowId={(row) => row.lineId}
                        columns={[
                          {
                            id: "target",
                            header: "分配对象",
                            renderValue: ({ item }) => item.targetLabel,
                            renderEditor: ({ item }) => (
                              <span className="text-sm">{item.targetLabel}</span>
                            ),
                          },
                          {
                            id: "amount",
                            header: "分配金额",
                            numeric: true,
                            align: "end",
                            renderValue: ({ item }) => formatMoney(item.amount),
                            renderEditor: ({ item, rowIndex }) => (
                              <Input
                                className="num"
                                value={item.amount}
                                onChange={(e) => {
                                  const amount = e.target.value
                                  setAllocLines((lines) =>
                                    lines.map((l, i) =>
                                      i === rowIndex ? { ...l, amount } : l
                                    )
                                  )
                                }}
                              />
                            ),
                          },
                        ]}
                        onAddAllocation={() => {
                          if (!task) return
                          setAllocLines((lines) => [
                            ...lines,
                            {
                              lineId: `al_${Date.now().toString(36)}`,
                              targetAccountId: task.account.id,
                              targetLabel: `${task.salesOrder.orderNo} · 本应收`,
                              amount: "0.00",
                            },
                          ])
                        }}
                        onRemoveAllocation={(_row, _id, rowIndex) => {
                          setAllocLines((lines) =>
                            lines.length <= 1
                              ? lines
                              : lines.filter((_, i) => i !== rowIndex)
                          )
                        }}
                        actions={
                          <>
                            <Button
                              type="button"
                              variant="outline"
                              onClick={() => setAllocationMode(null)}
                            >
                              取消
                            </Button>
                            <Button
                              type="button"
                              disabled={
                                registerReceiptMutation.isPending ||
                                registerInvoiceMutation.isPending
                              }
                              onClick={() => {
                                if (allocationMode === "receipt") {
                                  void submitReceipt()
                                } else {
                                  void submitInvoice()
                                }
                              }}
                            >
                              提交分配
                            </Button>
                          </>
                        }
                      />
                    </div>
                  ) : null}
                </CardContent>
              </Card>

              {/* sticky 决策区 */}
              <Card
                size="sm"
                className="sticky bottom-2 z-10 border-primary/20 shadow-md"
              >
                <CardHeader className="border-b py-3">
                  <CardTitle className="text-base">结论区</CardTitle>
                  <CardDescription>
                    CompleteWorkItemEnvelope
                    &lt;CardFundsReviewDecision&gt;；账户/链尾/版本均在 decision
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                  <div className="grid gap-3 sm:grid-cols-2">
                    <div className="space-y-1.5">
                      <Label htmlFor="ev-doc">证据文档 ID</Label>
                      <Input
                        id="ev-doc"
                        value={evidenceDocId}
                        onChange={(e) => setEvidenceDocId(e.target.value)}
                        placeholder="doc_bank_slip_…"
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="ev-ref">证据引用</Label>
                      <Input
                        id="ev-ref"
                        value={evidenceRef}
                        onChange={(e) => setEvidenceRef(e.target.value)}
                        placeholder="银行回单号 / 受控引用"
                      />
                    </div>
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="ev-comment">备注</Label>
                    <Textarea
                      id="ev-comment"
                      value={comment}
                      onChange={(e) => setComment(e.target.value)}
                      rows={2}
                    />
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => void saveEvidence()}
                    >
                      保存证据
                    </Button>
                    {canConfirmZero ? (
                      <Button
                        type="button"
                        variant="secondary"
                        onClick={() =>
                          setConfirmMode({ kind: "zero", advance: autoNext })
                        }
                      >
                        <CircleCheckIcon data-icon="inline-start" />
                        无历史票款，从 0 起
                      </Button>
                    ) : null}
                    <Button
                      type="button"
                      onClick={() =>
                        setConfirmMode({
                          kind: "approve",
                          conclusion: "RECORDED_FACTS_RECONCILED",
                          advance: autoNext,
                        })
                      }
                    >
                      复核通过
                    </Button>
                    <Button
                      type="button"
                      variant="destructive"
                      onClick={() => setConfirmMode({ kind: "reject" })}
                    >
                      <XIcon data-icon="inline-start" />
                      驳回
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() => setConfirmMode({ kind: "hold" })}
                    >
                      <PauseIcon data-icon="inline-start" />
                      暂挂
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="text-xs text-muted-foreground"
                      onClick={() => {
                        if (!task) return
                        void driftMutation.mutateAsync(task.workItem.workItemId)
                      }}
                    >
                      演示：外部指纹漂移
                    </Button>
                  </div>
                </CardContent>
              </Card>
            </div>

            <aside className="min-w-0 space-y-4 xl:sticky xl:top-4 xl:self-start">
              <Card size="sm">
                <CardHeader className="border-b py-3">
                  <CardTitle className="text-base">复核链（只读）</CardTitle>
                  <CardDescription>
                    追加式链 · 旧记录不可编辑删除 · 下一号{" "}
                    {task.reviewChain.nextReviewNo}
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                  {task.reviewChain.items.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      尚无历史复核。本次通过/驳回将写入链首。
                    </p>
                  ) : (
                    task.reviewChain.items.map((item) => (
                      <div
                        key={item.reviewId}
                        className="rounded-lg border border-border px-3 py-2 text-sm"
                      >
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-medium">
                            复核号 {item.reviewNo}
                          </span>
                          <Badge variant="outline">
                            {REVIEW_TYPE_LABEL[item.reviewType]}
                          </Badge>
                          <BusinessStatusBadge
                            context="list"
                            label={
                              item.reviewResult === "APPROVED"
                                ? "通过"
                                : "驳回"
                            }
                            tone={
                              item.reviewResult === "APPROVED"
                                ? "success"
                                : "destructive"
                            }
                          />
                          <Badge variant="secondary">只读</Badge>
                        </div>
                        <p className="mt-1 text-muted-foreground">
                          {item.reviewerLabel} · {item.completedAt}
                        </p>
                        <p className="mt-0.5 font-mono text-xs text-muted-foreground">
                          指纹 {shortHash(item.subjectHashAtReview)}
                          {item.predecessorReviewId
                            ? ` · 前驱 ${item.predecessorReviewId}`
                            : " · 链首"}
                        </p>
                      </div>
                    ))
                  )}
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader className="border-b py-3">
                  <CardTitle className="text-base">证据与导航</CardTitle>
                </CardHeader>
                <CardContent className="space-y-3 pt-4 text-sm">
                  <p className="text-muted-foreground">{task.workItem.impact}</p>
                  <Separator />
                  <div className="flex flex-col gap-2">
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      render={<Link href={w05Href} />}
                    >
                      打开销售单（保留 queueContextId）
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      render={<Link href={w11Href} />}
                    >
                      打开客户往来
                    </Button>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    快捷键：j/k 上下项 · ⌘↵ 确认 · Esc 不关闭页签
                  </p>
                </CardContent>
              </Card>
            </aside>
          </div>
        </>
      ) : (
        <BusinessEmptyState
          kind="filter"
          title="筛选无结果"
          description="当前类型/范围没有任务，可清除筛选。"
          action={
            <Button
              type="button"
              onClick={() =>
                replaceUrl({
                  type: "all",
                  status: null,
                  q: null,
                  currentWorkItemId: null,
                })
              }
            >
              清除筛选
            </Button>
          }
        />
      )}

      {/* 从 0 起 / 通过 强确认 */}
      <FormalActionConfirmDialog
        open={confirmMode?.kind === "approve" || confirmMode?.kind === "zero"}
        onOpenChange={(open) => {
          if (!open) setConfirmMode(null)
        }}
        title={
          confirmMode?.kind === "zero"
            ? "确认无历史票款，从 0 起"
            : "确认复核通过"
        }
        description={
          confirmMode?.kind === "zero"
            ? `将提交 NO_HISTORY_FROM_ZERO 结论：销售单 ${task?.salesOrder.orderNo ?? ""}、应收 ${task?.account.id ?? ""}。不创建 0 元回款/发票。须证据完整；完成时三方校验 subject_hash。`
            : `将以 CompleteCardFundsReviewCommand 提交 APPROVED / RECORDED_FACTS_RECONCILED。复核类型 ${task ? REVIEW_TYPE_LABEL[task.reviewType] : ""}，当前指纹 ${task ? shortHash(task.workItem.subjectHash) : ""}。`
        }
        actionLabel={
          confirmMode?.kind === "zero" ? "从 0 起并完成" : "复核通过"
        }
        confirmLabel={
          confirmMode?.kind === "zero" ? "确认从 0 起并完成" : "确认通过"
        }
        fromStatus={{ label: "待复核", tone: "warning" }}
        toStatus={
          confirmMode?.kind === "zero"
            ? { label: "从 0 起已通过", tone: "success" }
            : { label: "复核已通过", tone: "success" }
        }
        lockedFields={
          task
            ? [
                `销售单 ${task.salesOrder.orderNo}`,
                `应收账户 ${task.account.id}`,
                `subject_hash ${shortHash(task.workItem.subjectHash)}`,
                `复核类型 ${REVIEW_TYPE_LABEL[task.reviewType]}`,
                `票款版本 ${task.fundsFactVersion}`,
              ]
            : []
        }
        effects={
          confirmMode?.kind === "zero"
            ? [
                "追加 OPENING 通过链尾，结论 NO_HISTORY_FROM_ZERO",
                "不创建 0 元回款单或 0 元发票",
                "同事务 workflow_action + 完成任务",
              ]
            : [
                "追加复核链尾与 workflow_action",
                "三方校验 subject_hash（阻断静默通过）",
                "同事务完成当前任务",
              ]
        }
        pending={completeMutation.isPending}
        onConfirm={() => {
          if (confirmMode?.kind === "zero") {
            void runApprove("NO_HISTORY_FROM_ZERO", confirmMode.advance)
          } else if (confirmMode?.kind === "approve") {
            void runApprove(confirmMode.conclusion, confirmMode.advance)
          }
        }}
      />

      <FormalActionConfirmDialog
        open={confirmMode?.kind === "hold"}
        onOpenChange={(open) => {
          if (!open) setConfirmMode(null)
        }}
        title="暂挂当前复核任务"
        description="暂挂后任务仍为 PENDING/IN_PROGRESS，保留在有效队列与「已暂挂」范围；不形成复核事实、不自动视为完成。可手动浏览下一项。"
        actionLabel="暂挂"
        confirmLabel="确认暂挂"
        fromStatus={{ label: "处理中", tone: "info" }}
        toStatus={{ label: "已暂挂（仍在队列）", tone: "warning" }}
        effects={[
          "任务保持 PENDING/IN_PROGRESS",
          "不写 receivable_funds_review",
          "不自动下一项成功语义",
        ]}
        pending={holdMutation.isPending}
        onConfirm={() => void handleHold()}
      />

      <Dialog
        open={confirmMode?.kind === "reject"}
        onOpenChange={(open) => {
          if (!open) setConfirmMode(null)
        }}
      >
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>驳回复核</DialogTitle>
            <DialogDescription>
              仅形成本次 REJECTED 复核事实并完成当前任务。Q5 未决期间不创建后继任务；结果固定显示配置 blocker 与协作说明。
            </DialogDescription>
          </DialogHeader>
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault()
              void rejectForm.handleSubmit()
            }}
          >
            <rejectForm.AppField
              name="reasonCode"
              children={(field) => (
                <div className="space-y-1.5">
                  <Label>驳回原因</Label>
                  <Select
                    value={field.state.value}
                    onValueChange={(v) =>
                      field.handleChange(v as RejectReasonCode)
                    }
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {(
                        Object.keys(REJECT_REASON_LABEL) as RejectReasonCode[]
                      ).map((code) => (
                        <SelectItem key={code} value={code}>
                          {REJECT_REASON_LABEL[code]}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
            />
            <rejectForm.AppField
              name="comment"
              children={(field) => (
                <div className="space-y-1.5">
                  <Label htmlFor="reject-comment">补充说明</Label>
                  <Textarea
                    id="reject-comment"
                    value={field.state.value}
                    onChange={(e) => field.handleChange(e.target.value)}
                    onBlur={field.handleBlur}
                    rows={3}
                  />
                  {field.state.meta.errors?.[0] ? (
                    <p className="text-xs text-destructive">
                      {String(field.state.meta.errors[0])}
                    </p>
                  ) : null}
                </div>
              )}
            />
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                取消
              </DialogClose>
              <Button type="submit" variant="destructive" disabled={completeMutation.isPending}>
                确认驳回
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function moneyStrSafe(n: number): string {
  if (!Number.isFinite(n)) return "0.00"
  return n.toFixed(2)
}

function buildResultFacts(
  outcome?: FormalOutcome
): { label: string; value: React.ReactNode }[] {
  if (!outcome) return []
  if (outcome.kind === "HELD") {
    return [
      { label: "任务状态", value: outcome.workItemStatus },
      { label: "暂挂时间", value: outcome.heldAt },
      { label: "恢复提示", value: outcome.resumeHint },
    ]
  }
  const biz = outcome.business
  const facts = [
    { label: "复核号", value: String(biz.reviewNo) },
    {
      label: "结论",
      value:
        biz.conclusion === "REJECTED"
          ? "驳回"
          : APPROVE_CONCLUSION_LABEL[biz.conclusion as ApproveConclusion],
    },
    { label: "workflowActionId", value: biz.workflowActionId },
    { label: "操作号", value: biz.operationId },
    {
      label: "subject_hash",
      value: (
        <span className="font-mono text-xs">{shortHash(biz.subjectHash)}</span>
      ),
    },
  ]
  if (biz.followUpConfiguration) {
    facts.push({
      label: "后继配置",
      value: biz.followUpConfiguration.blockerCode,
    })
  }
  return facts
}
