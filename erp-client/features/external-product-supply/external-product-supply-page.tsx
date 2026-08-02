"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ArrowRightIcon,
  CircleCheckIcon,
  ExternalLinkIcon,
  PauseIcon,
  ShieldAlertIcon,
  TriangleAlertIcon,
} from "lucide-react"
import { z } from "zod"

import {
  BusinessDiffPanel,
  BusinessEmptyState,
  BusinessStatusBadge,
  DataFreshness,
  DocumentSummary,
  FormalActionConfirmDialog,
  FormalActionResult,
  OptionCombobox,
  PageHeader,
  RevisionTimeline,
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
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import type {
  DemoRole,
  ExternalCatalogItemView,
  FormalOutcome,
} from "@/features/external-product-supply/types"
import {
  CHANGE_TYPE_LABEL,
  DEMO_ROLE_LABEL,
  HOLD_REASON_OPTIONS,
  RECOVERY_BLOCKER_MESSAGE,
  RETURN_REASON_OPTIONS,
} from "@/features/external-product-supply/types"
import {
  useClaimExternalCatalogMutation,
  useCompleteExternalCatalogMutation,
  useExternalCatalogActionMutation,
  useExternalCatalogQueueQuery,
  useResolveUnknownExternalCatalogMutation,
  useSaveExternalCatalogDraftMutation,
} from "@/features/external-product-supply/queries"
import { cn } from "@/lib/utils"
import { versionText } from "@/lib/ui-text"

type SessionLease = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expiresAt: string
}

type ResultState =
  | {
      status: "succeeded" | "blocked" | "rejected" | "unknown"
      title: string
      description: string
      reference?: string
      outcome?: FormalOutcome
      stayOnItem?: boolean
      pendingIdempotencyKey?: string
      terminal?: boolean
    }
  | null

type ConfirmMode =
  | { kind: "hold" }
  | { kind: "return" }
  | { kind: "confirm_error" }
  | { kind: "confirm_stop" }
  | null

const holdSchema = z.object({
  reasonCode: z.enum([
    "NEED_CLARIFICATION",
    "WAITING_SOURCE",
    "WAITING_MASTER_DATA",
    "OTHER",
  ]),
  comment: z.string(),
})

const returnSchema = z.object({
  reasonCode: z.enum([
    "SOURCE_DATA_ERROR",
    "PAYLOAD_INVALID",
    "SYNC_CORRUPT",
    "OTHER",
  ]),
  comment: z.string().trim().min(4, "请填写至少 4 个字的退回说明"),
})

const completeSchema = z.object({
  comment: z.string().trim().min(4, "请填写至少 4 个字的结论说明"),
})

function formatTime(iso?: string) {
  if (!iso) return "—"
  try {
    return new Date(iso).toLocaleString("zh-CN", { hour12: false })
  } catch {
    return iso
  }
}

function shortHash(hash: string) {
  if (hash.length <= 18) return hash
  return `${hash.slice(0, 10)}…${hash.slice(-4)}`
}

function isExceptionItem(
  item: ExternalCatalogItemView
): item is Extract<ExternalCatalogItemView, { changeType: "ERROR" | "STOPPED" }> {
  return item.changeType === "ERROR" || item.changeType === "STOPPED"
}

function changeTone(
  t: ExternalCatalogItemView["changeType"]
): "destructive" | "warning" | "info" | "neutral" {
  if (t === "STOPPED" || t === "ERROR") return "destructive"
  if (t === "CHANGED") return "warning"
  if (t === "NEW") return "info"
  return "neutral"
}

function buildQueueReturnHref(searchParams: URLSearchParams) {
  const qs = searchParams.toString()
  return qs ? `/supplier-api/catalog?${qs}` : "/supplier-api/catalog"
}

export function ExternalProductSupplyPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const changeTypeParam = searchParams.get("changeType")
  const changeType: ExternalCatalogQueueQueryChangeType =
    changeTypeParam === "NEW" ||
    changeTypeParam === "CHANGED" ||
    changeTypeParam === "STOPPED" ||
    changeTypeParam === "ERROR" ||
    changeTypeParam === "all"
      ? changeTypeParam
      : "actionable"

  const statusParam = searchParams.get("status")
  const status: "pending" | "held" =
    statusParam === "held" ? "held" : "pending"

  const demoRoleParam = searchParams.get("demoRole")
  const demoRole: DemoRole =
    demoRoleParam === "operations" ||
    demoRoleParam === "admin" ||
    demoRoleParam === "ops_tech"
      ? demoRoleParam
      : "procurement"

  const maskCost = searchParams.get("maskCost") === "1"
  const q = searchParams.get("q") ?? undefined
  const currentExternalProductId =
    searchParams.get("currentExternalProductId") ?? undefined
  const currentWorkItemId =
    searchParams.get("currentWorkItemId") ?? undefined
  const queueContextId =
    searchParams.get("queueContextId") ??
    `queue:W21:${demoRole}:${changeType}`

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
      changeType,
      status,
      demoRole,
      maskCost,
      q,
      currentExternalProductId,
      currentWorkItemId,
      queueContextId,
    }),
    [
      changeType,
      status,
      demoRole,
      maskCost,
      q,
      currentExternalProductId,
      currentWorkItemId,
      queueContextId,
    ]
  )

  const queueQuery = useExternalCatalogQueueQuery(filters)
  const claimMutation = useClaimExternalCatalogMutation()
  const actionMutation = useExternalCatalogActionMutation()
  const completeMutation = useCompleteExternalCatalogMutation()
  const saveDraftMutation = useSaveExternalCatalogDraftMutation()
  const resolveUnknownMutation = useResolveUnknownExternalCatalogMutation()

  const view = queueQuery.data
  const items = view?.items ?? []
  const context = view?.context
  const item =
    items.find((i) => {
      if (currentWorkItemId && isExceptionItem(i)) {
        return i.workItem.workItemId === currentWorkItemId
      }
      if (currentExternalProductId) {
        return i.externalProduct.id === currentExternalProductId
      }
      return false
    }) ??
    view?.current ??
    items[0]

  const currentIndex = item
    ? Math.max(
        0,
        items.findIndex((i) => i.externalProduct.id === item.externalProduct.id)
      )
    : 0
  const completed = Boolean(view) && items.length === 0

  const [confirmMode, setConfirmMode] = React.useState<ConfirmMode>(null)
  const [lastResult, setLastResult] = React.useState<ResultState>(null)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [forceUnknownOnce, setForceUnknownOnce] = React.useState(false)
  const [selectedSkuId, setSelectedSkuId] = React.useState<string>("")
  const [draftNote, setDraftNote] = React.useState("")
  const [substituteIds, setSubstituteIds] = React.useState<string[]>([])
  const [moqDraft, setMoqDraft] = React.useState("")
  const [searchInput, setSearchInput] = React.useState(q ?? "")

  const headingRef = React.useRef<HTMLHeadingElement>(null)
  const resultRef = React.useRef<HTMLDivElement>(null)
  const leaseRef = React.useRef<SessionLease | null>(null)
  const [activeLease, setActiveLease] = React.useState<SessionLease | null>(null)
  const idempotencyRef = React.useRef<Record<string, string>>({})

  // 切换队列项时重置会话草稿 UI（与 W13 等队列页同一模式）
   
  React.useEffect(() => {
    if (!item) return
    const proposed = item.offering?.proposedDefaults
    setSelectedSkuId(
      item.mapping?.skuId ?? item.skuCandidates[0]?.skuId ?? ""
    )
    setMoqDraft(proposed?.minimumOrderQuantity ?? "")
    setDraftNote("")
    setSubstituteIds([])
    setActionError(null)
    idempotencyRef.current = {}
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅按外部商品身份重置草稿
  }, [item?.externalProduct.id])
   

  // URL defaults
  React.useEffect(() => {
    if (queueQuery.isPending || !view) return
    const hasChange = searchParams.has("changeType")
    const hasCtx = searchParams.has("queueContextId")
    const hasItem =
      searchParams.has("currentExternalProductId") ||
      searchParams.has("currentWorkItemId")
    if (hasChange && hasCtx && (hasItem || items.length === 0)) return
    const params = new URLSearchParams(searchParams.toString())
    if (!hasChange) params.set("changeType", changeType)
    if (!hasCtx) params.set("queueContextId", queueContextId)
    if (!hasItem && item) {
      params.set("currentExternalProductId", item.externalProduct.id)
      if (isExceptionItem(item)) {
        params.set("currentWorkItemId", item.workItem.workItemId)
      }
    }
    const qs = params.toString()
    router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
  }, [
    queueQuery.isPending,
    view,
    searchParams,
    changeType,
    queueContextId,
    item,
    items.length,
    pathname,
    router,
  ])

  // Auto-claim registered exception tasks only
  React.useEffect(() => {
    if (!item || !isExceptionItem(item)) return
    if (demoRole === "operations") return
    if (leaseRef.current?.workItemId === item.workItem.workItemId) return
    if (claimMutation.isPending) return
    let cancelled = false
    void claimMutation
      .mutateAsync(item.workItem.workItemId)
      .then((lease) => {
        if (cancelled) return
        const session: SessionLease = {
          workItemId: lease.workItemId,
          claimToken: lease.claimToken,
          leaseVersion: lease.leaseVersion,
          expiresAt: lease.expiresAt,
        }
        leaseRef.current = session
        setActiveLease(session)
      })
      .catch(() => {
        /* keep unclaimed */
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅任务切换时领取
  }, [item && isExceptionItem(item) ? item.workItem.workItemId : null, demoRole])

  React.useEffect(() => {
    if (lastResult) {
      resultRef.current?.focus()
    } else if (item) {
      headingRef.current?.focus()
    }
  }, [item?.externalProduct.id, lastResult?.status])

  const replaceUrl = React.useCallback(
    (patch: Record<string, string | null | undefined>) => {
      const params = new URLSearchParams(searchParams.toString())
      for (const [key, value] of Object.entries(patch)) {
        if (value == null || value === "") params.delete(key)
        else params.set(key, value)
      }
      if (!params.has("queueContextId")) {
        params.set("queueContextId", queueContextId)
      }
      const qs = params.toString()
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    },
    [pathname, queueContextId, router, searchParams]
  )

  const goToItem = React.useCallback(
    (next: ExternalCatalogItemView | undefined | null) => {
      setLastResult(null)
      setActionError(null)
      if (!next) {
        replaceUrl({
          currentExternalProductId: null,
          currentWorkItemId: null,
          queueContextId,
        })
        return
      }
      replaceUrl({
        currentExternalProductId: next.externalProduct.id,
        currentWorkItemId: isExceptionItem(next)
          ? next.workItem.workItemId
          : null,
        queueContextId,
      })
    },
    [queueContextId, replaceUrl]
  )

  const neighbor = React.useCallback(
    (delta: number) => {
      const idx = currentIndex + delta
      if (idx < 0 || idx >= items.length) return null
      return items[idx] ?? null
    },
    [currentIndex, items]
  )

  const ensureLease = React.useCallback(async (): Promise<SessionLease> => {
    if (!item || !isExceptionItem(item)) {
      throw new Error("当前项无已注册异常任务，无需领取")
    }
    const wi = item.workItem
    const existing = leaseRef.current
    if (
      existing &&
      existing.workItemId === wi.workItemId &&
      existing.claimToken
    ) {
      return existing
    }
    const lease = await claimMutation.mutateAsync(wi.workItemId)
    const session: SessionLease = {
      workItemId: lease.workItemId,
      claimToken: lease.claimToken,
      leaseVersion: lease.leaseVersion,
      expiresAt: lease.expiresAt,
    }
    leaseRef.current = session
    setActiveLease(session)
    return session
  }, [claimMutation, item])

  const exceptionWorkItem =
    item && isExceptionItem(item) ? item.workItem : null
  const workItemId = exceptionWorkItem?.workItemId ?? null
  const subjectHash = exceptionWorkItem?.subjectHash ?? ""
  const expectedRevision = item
    ? String(
        item.externalProduct.incomingRevision?.revisionNo ??
          item.externalProduct.currentRevision.revisionNo
      )
    : ""

  const formalPending =
    actionMutation.isPending || completeMutation.isPending

  const leaseActive =
    Boolean(workItemId) &&
    activeLease?.workItemId === workItemId &&
    Boolean(activeLease.claimToken)

  const leaseStatus = !workItemId
    ? ("unclaimed" as const)
    : leaseActive
      ? ("active" as const)
      : ("unclaimed" as const)

  const canProcureWrite =
    demoRole === "procurement" && Boolean(workItemId) && leaseActive

  const isRegistered = item ? isExceptionItem(item) : false
  const hasRegistrationBlocker =
    item && (item.changeType === "NEW" || item.changeType === "CHANGED")

  const holdForm = useAppForm({
    defaultValues: {
      reasonCode: "NEED_CLARIFICATION" as
        | "NEED_CLARIFICATION"
        | "WAITING_SOURCE"
        | "WAITING_MASTER_DATA"
        | "OTHER",
      comment: "",
    },
    validators: { onChange: holdSchema },
    onSubmit: async ({ value }) => {
      if (!item || !isExceptionItem(item)) return
      try {
        const lease = await ensureLease()
        const key =
          idempotencyRef.current.hold ??
          `hold_${item.workItem.workItemId}_${Date.now()}`
        idempotencyRef.current.hold = key
        const result = await actionMutation.mutateAsync({
          workItemId: item.workItem.workItemId,
          claimToken: lease.claimToken,
          leaseVersion: lease.leaseVersion,
          expectedSubjectHash: item.workItem.subjectHash,
          action: {
            kind: "HOLD",
            reasonCode: value.reasonCode,
            comment: value.comment || undefined,
          },
          idempotencyKey: key,
          simulateTimeout: forceUnknownOnce,
        })
        setForceUnknownOnce(false)
        setConfirmMode(null)
        if (result.status === "unknown") {
          setLastResult({
            status: "unknown",
            title: "暂挂结果不确定",
            description: result.message,
            pendingIdempotencyKey: result.idempotencyKey,
            stayOnItem: true,
          })
          return
        }
        if (result.status === "failed") {
          setActionError(result.message)
          return
        }
        if (result.outcome.kind !== "ACTION") return
        setLastResult({
          status: "blocked",
          title: "已暂挂 · 仍在有效队列",
          description: result.outcome.resumeHint,
          reference: result.outcome.reference,
          outcome: result.outcome,
          stayOnItem: true,
          terminal: false,
        })
        // 不自动下一项
      } catch (e) {
        setActionError(e instanceof Error ? e.message : "暂挂失败")
      }
    },
  })

  const returnForm = useAppForm({
    defaultValues: {
      reasonCode: "SOURCE_DATA_ERROR" as
        | "SOURCE_DATA_ERROR"
        | "PAYLOAD_INVALID"
        | "SYNC_CORRUPT"
        | "OTHER",
      comment: "",
    },
    validators: { onChange: returnSchema },
    onSubmit: async ({ value }) => {
      if (!item || !isExceptionItem(item) || item.changeType !== "ERROR") return
      try {
        const lease = await ensureLease()
        const key =
          idempotencyRef.current.return ??
          `return_${item.workItem.workItemId}_${Date.now()}`
        idempotencyRef.current.return = key
        const result = await actionMutation.mutateAsync({
          workItemId: item.workItem.workItemId,
          claimToken: lease.claimToken,
          leaseVersion: lease.leaseVersion,
          expectedSubjectHash: item.workItem.subjectHash,
          action: {
            kind: "RETURN_FOR_DATA_FIX",
            reasonCode: value.reasonCode,
            comment: value.comment,
          },
          idempotencyKey: key,
          simulateTimeout: forceUnknownOnce,
        })
        setForceUnknownOnce(false)
        setConfirmMode(null)
        if (result.status === "unknown") {
          setLastResult({
            status: "unknown",
            title: "退回结果不确定",
            description: result.message,
            pendingIdempotencyKey: result.idempotencyKey,
            stayOnItem: true,
          })
          return
        }
        if (result.status === "failed") {
          setActionError(result.message)
          return
        }
        if (result.outcome.kind !== "ACTION") return
        setLastResult({
          status: "blocked",
          title: "已退回数据修复",
          description: result.outcome.resumeHint,
          reference: result.outcome.reference,
          outcome: result.outcome,
          stayOnItem: true,
          terminal: false,
        })
      } catch (e) {
        setActionError(e instanceof Error ? e.message : "退回失败")
      }
    },
  })

  const completeForm = useAppForm({
    defaultValues: { comment: "" },
    validators: { onChange: completeSchema },
    onSubmit: async ({ value }) => {
      if (!item || !isExceptionItem(item) || !confirmMode) return
      const decisionKind =
        confirmMode.kind === "confirm_error"
          ? ("CONFIRM_ERROR_RESOLVED" as const)
          : confirmMode.kind === "confirm_stop"
            ? ("CONFIRM_STOP_SUPPLY" as const)
            : null
      if (!decisionKind) return
      try {
        const lease = await ensureLease()
        const key =
          idempotencyRef.current.complete ??
          `complete_${item.workItem.workItemId}_${Date.now()}`
        idempotencyRef.current.complete = key
        const result = await completeMutation.mutateAsync({
          workItemId: item.workItem.workItemId,
          claimToken: lease.claimToken,
          leaseVersion: lease.leaseVersion,
          expectedSubjectHash: subjectHash,
          decision:
            decisionKind === "CONFIRM_ERROR_RESOLVED"
              ? {
                  kind: "CONFIRM_ERROR_RESOLVED",
                  expectedExternalRevision: expectedRevision,
                  resolutionCode: "SOURCE_FIXED",
                  comment: value.comment,
                }
              : {
                  kind: "CONFIRM_STOP_SUPPLY",
                  expectedExternalRevision: expectedRevision,
                  expectedOfferingRevision: item.offering?.currentRevision
                    ? String(item.offering.currentRevision.revisionNo)
                    : undefined,
                  reasonCode: "SUPPLIER_STOPPED",
                  comment: value.comment,
                },
          idempotencyKey: key,
          simulateTimeout: forceUnknownOnce,
        })
        setForceUnknownOnce(false)
        setConfirmMode(null)
        if (result.status === "unknown") {
          setLastResult({
            status: "unknown",
            title: "终结结果不确定",
            description: result.message,
            pendingIdempotencyKey: result.idempotencyKey,
            stayOnItem: true,
          })
          return
        }
        if (result.status === "failed") {
          setActionError(result.message)
          return
        }
        leaseRef.current = null
        setActiveLease(null)
        setLastResult({
          status: "succeeded",
          title:
            decisionKind === "CONFIRM_ERROR_RESOLVED"
              ? "异常已解决 · 任务已终结"
              : "停供记录已确认 · 任务已终结",
          description:
            "FormalActionResult 固定展示后，可按自动下一项偏好或手动继续。不包含替代供给选定或恢复发布。",
          reference:
            result.outcome.kind === "COMPLETED"
              ? result.outcome.business.reference
              : undefined,
          outcome: result.outcome,
          terminal: true,
        })
        if (autoNext) {
          // 终结后先展示结果；用户点「下一项」再跳，符合 stay FormalActionResult 优先
        }
      } catch (e) {
        setActionError(e instanceof Error ? e.message : "终结失败")
      }
    },
  })

  const onSaveDraft = async () => {
    if (!item) return
    const proposed = item.offering?.proposedDefaults
    try {
      await saveDraftMutation.mutateAsync({
        externalProductId: item.externalProduct.id,
        selectedSkuId: selectedSkuId || undefined,
        offeringDraft: proposed
          ? {
              ...proposed,
              minimumOrderQuantity: moqDraft || proposed.minimumOrderQuantity,
              sessionDraftOnly: true,
            }
          : undefined,
        substituteCandidateSkuIds: substituteIds,
        note: draftNote || undefined,
      })
      setLastResult({
        status: "blocked",
        title: "会话草稿已保存",
        description:
          "草稿仅存于当前会话：未经审核不写 ERP SKU / 商城商品 / 供给修订。映射确认与供给类型登记前无写入口。",
        stayOnItem: true,
        terminal: false,
      })
    } catch (e) {
      setActionError(e instanceof Error ? e.message : "保存草稿失败")
    }
  }

  const w14Href = item
    ? `/master-data/sku?from=W21&externalProductId=${encodeURIComponent(item.externalProduct.id)}&returnTo=${encodeURIComponent(buildQueueReturnHref(searchParams))}&queueContextId=${encodeURIComponent(queueContextId)}`
    : "/master-data"
  const w22Href = item?.mapping?.skuId
    ? `/commerce/publications?from=W21&skuId=${encodeURIComponent(item.mapping.skuId)}&returnTo=${encodeURIComponent(buildQueueReturnHref(searchParams))}&queueContextId=${encodeURIComponent(queueContextId)}`
    : "/commerce/publications"
  const w20Href = item
    ? `/supplier-api/connections?connectionId=${encodeURIComponent(item.externalProduct.connection.id)}&returnTo=${encodeURIComponent(buildQueueReturnHref(searchParams))}`
    : "/supplier-api/connections"
  const w29Href = item
    ? `/governance/integration-errors?from=W21&externalProductId=${encodeURIComponent(item.externalProduct.id)}&returnTo=${encodeURIComponent(buildQueueReturnHref(searchParams))}`
    : "/governance/integration-errors"
  const centerHref = item
    ? `/supplier-api/catalog/${item.externalProduct.id}?section=overview&queueContextId=${encodeURIComponent(queueContextId)}&returnTo=${encodeURIComponent(buildQueueReturnHref(searchParams))}`
    : "#"

  const costMasked = view?.costFieldVisibility === "masked" || maskCost

  if (queueQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-16 animate-pulse rounded-2xl bg-muted" />
        <div className="grid gap-4 xl:grid-cols-[minmax(0,58fr)_minmax(16rem,42fr)]">
          <div className="h-80 animate-pulse rounded-2xl bg-muted" />
          <div className="h-80 animate-pulse rounded-2xl bg-muted" />
        </div>
      </div>
    )
  }

  if (queueQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="外部商品映射与供给"
          description="加载失败"
        />
        <Button type="button" onClick={() => void queueQuery.refetch()}>
          重试
        </Button>
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="外部商品映射与供给"
        breadcrumbs={[
          { id: "api", label: "供应商 API", href: "/supplier-api/catalog" },
          { id: "cat", label: "外部商品供给", current: true },
        ]}
        metadata={
          <DataFreshness
            state="fresh"
            label="目录观察更新时间"
            updatedAt={
              context?.filterSummary
                ? `${context.filterSummary} · ${formatTime(context.queueContextUpdatedAt)}`
                : formatTime(context?.queueContextUpdatedAt)
            }
            dateTime={context?.queueContextUpdatedAt}
          />
        }
      />

      <div className="flex flex-wrap items-center gap-2">
        <ToggleGroup
          value={[changeType]}
          onValueChange={(v) => {
            const next = (v[0] as typeof changeType | undefined) ?? "actionable"
            replaceUrl({
              changeType: next === "actionable" ? "actionable" : next,
              currentExternalProductId: null,
              currentWorkItemId: null,
            })
          }}
          variant="outline"
          size="sm"
          spacing={0}
          aria-label="变化类型"
        >
          <ToggleGroupItem value="actionable">需处理</ToggleGroupItem>
          <ToggleGroupItem value="STOPPED">停止供应</ToggleGroupItem>
          <ToggleGroupItem value="ERROR">异常</ToggleGroupItem>
          <ToggleGroupItem value="NEW">新商品</ToggleGroupItem>
          <ToggleGroupItem value="CHANGED">关键变化</ToggleGroupItem>
          <ToggleGroupItem value="all">全部</ToggleGroupItem>
        </ToggleGroup>
        <ToggleGroup
          value={[status]}
          onValueChange={(v) => {
            const next = (v[0] as typeof status | undefined) ?? "pending"
            replaceUrl({
              status: next === "pending" ? null : next,
              currentExternalProductId: null,
              currentWorkItemId: null,
            })
          }}
          variant="outline"
          size="sm"
          spacing={0}
          aria-label="队列范围"
        >
          <ToggleGroupItem value="pending">待处理</ToggleGroupItem>
          <ToggleGroupItem value="held">已暂挂</ToggleGroupItem>
        </ToggleGroup>
        <OptionCombobox
          value={demoRole}
          onValueChange={(v) => {
            if (!v) return
            replaceUrl({
              demoRole: v === "procurement" ? null : v,
              currentExternalProductId: null,
              currentWorkItemId: null,
            })
          }}
          options={(Object.keys(DEMO_ROLE_LABEL) as DemoRole[]).map((r) => ({
            value: r,
            label: DEMO_ROLE_LABEL[r],
          }))}
          className="w-[9rem]"
          size="sm"
          allowClear={false}
          aria-label="演示角色"
          placeholder="角色"
        />
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <input
            type="checkbox"
            className="size-3.5"
            checked={maskCost}
            onChange={(e) =>
              replaceUrl({ maskCost: e.target.checked ? "1" : null })
            }
          />
          模拟无成本字段权
        </label>
        <div className="flex items-center gap-2">
          <Label htmlFor="w21-auto-next" className="text-muted-foreground">
            自动下一项
          </Label>
          <Switch
            id="w21-auto-next"
            checked={autoNext}
            onCheckedChange={(on) => {
              setSessionAutoNext(on)
              replaceUrl({ autoNext: on ? "1" : "0" })
            }}
          />
        </div>
        <form
          className="ml-auto flex items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault()
            replaceUrl({
              q: searchInput.trim() || null,
              currentExternalProductId: null,
              currentWorkItemId: null,
            })
          }}
        >
          <Input
            value={searchInput}
            onChange={(e) => setSearchInput(e.target.value)}
            placeholder="外部商品 ID / SKU / 名称"
            className="h-8 w-48"
            aria-label="搜索"
          />
          <Button type="submit" size="sm" variant="secondary">
            搜索
          </Button>
        </form>
      </div>

      <label className="flex items-center gap-2 text-xs text-muted-foreground">
        <input
          type="checkbox"
          className="size-3.5"
          checked={forceUnknownOnce}
          onChange={(e) => setForceUnknownOnce(e.target.checked)}
        />
        下次处理动作模拟结果未知
      </label>

      {lastResult ? (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
          <FormalActionResult
            status={lastResult.status}
            title={lastResult.title}
            description={lastResult.description}
            reference={lastResult.reference}
            facts={
              lastResult.outcome?.kind === "COMPLETED"
                ? [
                    {
                      label: "决策",
                      value: lastResult.outcome.business.decisionKind,
                    },
                    {
                      label: "审计号",
                      value: lastResult.outcome.business.auditEventId,
                    },
                    {
                      label: versionText.dataVersion,
                      value: shortHash(lastResult.outcome.business.subjectHash),
                    },
                    {
                      label: "完成时间",
                      value: formatTime(lastResult.outcome.business.completedAt),
                    },
                  ]
                : lastResult.outcome?.kind === "ACTION"
                  ? [
                      {
                        label: "动作",
                        value: lastResult.outcome.actionKind,
                      },
                      {
                        label: "任务状态",
                        value: lastResult.outcome.workItemStatus,
                      },
                      {
                        label: "停留当前项",
                        value: "是（非终结）",
                      },
                    ]
                  : undefined
            }
            actions={
              <div className="flex flex-wrap gap-2">
                {lastResult.status === "unknown" ? (
                  <Button
                    type="button"
                    variant="secondary"
                    disabled={resolveUnknownMutation.isPending}
                    onClick={() => {
                      if (!lastResult.pendingIdempotencyKey) return
                      void resolveUnknownMutation
                        .mutateAsync({
                          idempotencyKey: lastResult.pendingIdempotencyKey,
                        })
                        .then((r) => {
                          if (r.status === "succeeded") {
                            setLastResult({
                              status:
                                r.outcome.kind === "COMPLETED"
                                  ? "succeeded"
                                  : "blocked",
                              title: "查询到处理结果",
                              description:
                                r.outcome.kind === "COMPLETED"
                                  ? "终结决策已确认。"
                                  : r.outcome.resumeHint,
                              reference:
                                r.outcome.kind === "COMPLETED"
                                  ? r.outcome.business.reference
                                  : r.outcome.reference,
                              outcome: r.outcome,
                              terminal: r.outcome.kind === "COMPLETED",
                              stayOnItem: r.outcome.kind !== "COMPLETED",
                            })
                          } else if (r.status === "unknown") {
                            setActionError(r.message)
                          } else {
                            setActionError(r.message)
                          }
                        })
                    }}
                  >
                    查询最终结果
                  </Button>
                ) : (
                  <>
                    {lastResult.terminal || autoNext ? (
                      <Button
                        type="button"
                        onClick={() => {
                          const next =
                            neighbor(1) ??
                            items.find(
                              (i) =>
                                i.externalProduct.id !==
                                item?.externalProduct.id
                            )
                          goToItem(next)
                        }}
                      >
                        下一项
                      </Button>
                    ) : null}
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() => setLastResult(null)}
                    >
                      继续当前项
                    </Button>
                  </>
                )}
              </div>
            }
          />
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
          description="可切换变化类型/暂挂范围，或返回工作台。"
          action={
            <Button render={<Link href="/workspace" />}>返回今日工作台</Button>
          }
        />
      ) : item ? (
        <>
          <SequentialProcessBar
            current={context?.position ?? currentIndex + 1}
            total={context?.total ?? items.length}
            leaseStatus={isRegistered ? leaseStatus : "unclaimed"}
            leaseStatusLabel={
              isRegistered
                ? leaseActive
                  ? "异常任务处理中"
                  : "异常任务待领取"
                : "正常类型未登记 · 无处理任务"
            }
            processLabel={
              isRegistered
                ? item.changeType === "ERROR"
                  ? "确认异常已解决"
                  : "确认停供记录"
                : "确认（不可用）"
            }
            // 没有独立的「并准备下一项」路径：两个 handler 同义
            showProcessNext={false}
            processDisabled={
              formalPending ||
              Boolean(lastResult?.status === "unknown") ||
              !isRegistered ||
              !canProcureWrite ||
              demoRole !== "procurement"
            }
            pending={formalPending}
            onBack={() => router.push("/workspace")}
            onProcess={() => {
              if (!isRegistered || !canProcureWrite) return
              setConfirmMode(
                item.changeType === "ERROR"
                  ? { kind: "confirm_error" }
                  : { kind: "confirm_stop" }
              )
            }}
            onProcessNext={() => {
              if (!isRegistered || !canProcureWrite) return
              setConfirmMode(
                item.changeType === "ERROR"
                  ? { kind: "confirm_error" }
                  : { kind: "confirm_stop" }
              )
            }}
            onReclaim={() => {
              if (!isRegistered) return
              void ensureLease().catch((error) => {
                setActionError(
                  error instanceof Error ? error.message : "领取失败"
                )
              })
            }}
          />

          <div className="flex flex-wrap items-center gap-2 text-sm">
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!neighbor(-1)}
              onClick={() => goToItem(neighbor(-1))}
            >
              上一项
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!neighbor(1)}
              onClick={() => goToItem(neighbor(1))}
            >
              下一项
            </Button>
            <span className="text-muted-foreground">
              {CHANGE_TYPE_LABEL[item.changeType]} ·{" "}
              {item.externalProduct.supplier.name}
            </span>
          </div>

          {/* 身份与风险条 */}
          <Card size="sm">
            <CardContent className="flex flex-wrap items-start gap-3 py-3">
              <div className="min-w-0 flex-1 space-y-1">
                <div className="flex flex-wrap items-center gap-2">
                  <h2
                    ref={headingRef}
                    tabIndex={-1}
                    className="font-heading text-lg font-semibold outline-none"
                  >
                    {item.externalProduct.currentRevision.name}
                  </h2>
                  <BusinessStatusBadge
                    context="list"
                    label={CHANGE_TYPE_LABEL[item.changeType]}
                    tone={changeTone(item.changeType)}
                  />
                  {isExceptionItem(item) ? (
                    <Badge variant="secondary">
                      BUSINESS_EXCEPTION · {item.workItem.workItemId}
                    </Badge>
                  ) : null}
                  {isExceptionItem(item) && item.workItem.held ? (
                    <BusinessStatusBadge
                      context="list"
                      label="已暂挂 · 仍在有效队列"
                      tone="warning"
                    />
                  ) : null}
                  {costMasked ? (
                    <Badge variant="outline">成本字段已掩码</Badge>
                  ) : null}
                </div>
                <p className="text-sm text-muted-foreground">
                  外部商品 {item.externalProduct.externalProductId}
                  {item.externalProduct.externalSkuId
                    ? ` / ${item.externalProduct.externalSkuId}`
                    : ""}{" "}
                  · 连接 {item.externalProduct.connection.code} · 来源修订 r
                  {item.externalProduct.incomingRevision?.revisionNo ??
                    item.externalProduct.currentRevision.revisionNo}{" "}
                  · 接收 {formatTime(item.syncContext.receivedAt)}
                </p>
              </div>
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={<Link href={centerHref} />}
              >
                查看详情
                <ExternalLinkIcon className="size-3.5" />
              </Button>
            </CardContent>
          </Card>

          {item.publicationImpact.safetyPauseTriggered ? (
            <Alert variant="destructive">
              <ShieldAlertIcon aria-hidden="true" />
              <AlertTitle>安全暂停已触发（不等待人工）</AlertTitle>
              <AlertDescription className="space-y-1">
                <p>{item.publicationImpact.note}</p>
                <p>
                  原因：{item.publicationImpact.safetyPauseReasons.join("、")} ·
                  已暂停发布 {item.publicationImpact.pausedPublicationCount} ·
                  历史已支付{" "}
                  {item.publicationImpact.historicalPaidOrderCount} 笔历史记录保留
                </p>
                {item.publicationImpact.recoveryBlocker ? (
                  <p className="font-medium">
                    {item.publicationImpact.recoveryBlocker.code}：
                    {item.publicationImpact.recoveryBlocker.message}
                  </p>
                ) : null}
              </AlertDescription>
            </Alert>
          ) : null}

          {hasRegistrationBlocker && item.registrationBlocker ? (
            <Alert variant="destructive">
              <TriangleAlertIcon aria-hidden="true" />
              <AlertTitle>
                {item.registrationBlocker.code} ·{" "}
                {item.registrationBlocker.businessProcess === "MAPPING"
                  ? "映射"
                  : "供给复核"}
              </AlertTitle>
              <AlertDescription>
                {item.registrationBlocker.message}
              </AlertDescription>
            </Alert>
          ) : null}

          {/* 双栏：来源 diff ~58% | 映射与供给决策 ~42% */}
          <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,58fr)_minmax(17rem,42fr)]">
            <div className="min-w-0 space-y-4">
              <BusinessDiffPanel
                title="来源版本差异（当前 vs 新修订）"
                caption="白名单业务字段对比；成本字段按权限掩码"
                changes={item.sourceDiff.map((c) => ({
                  id: c.id,
                  field: c.field,
                  before: c.before,
                  after: c.after,
                  note: c.note,
                }))}
              />

              <Card size="sm">
                <CardHeader className="border-b py-3">
                  <CardTitle className="text-base">外部修订暂存</CardTitle>
                  <CardDescription>
                    先进入 supplier_external_product 不可变修订区；未经映射与审核不修改
                    ERP SKU 或商城商品
                  </CardDescription>
                </CardHeader>
                <CardContent className="pt-4">
                  <DocumentSummary
                    columns="two"
                    items={[
                      {
                        id: "name",
                        label: "名称",
                        value:
                          item.externalProduct.incomingRevision?.name ??
                          item.externalProduct.currentRevision.name,
                        emphasized: true,
                      },
                      {
                        id: "spec",
                        label: "规格",
                        value:
                          item.externalProduct.incomingRevision
                            ?.specification ||
                          item.externalProduct.currentRevision.specification ||
                          "—",
                      },
                      {
                        id: "price",
                        label: "含税供货价",
                        value:
                          item.externalProduct.incomingRevision
                            ?.supplyPriceGross ??
                          item.externalProduct.currentRevision
                            .supplyPriceGross ??
                          "—",
                        numeric: true,
                      },
                      {
                        id: "tax",
                        label: "进项税率",
                        value:
                          item.externalProduct.incomingRevision?.inputTaxRate ??
                          item.externalProduct.currentRevision.inputTaxRate ??
                          "—",
                        numeric: true,
                      },
                      {
                        id: "region",
                        label: "可供区域",
                        value: (
                          item.externalProduct.incomingRevision?.supplyRegion ??
                          item.externalProduct.currentRevision.supplyRegion
                        ).join("、") || "—",
                      },
                      {
                        id: "avail",
                        label: "可供状态 / 数量",
                        value: `${item.externalProduct.incomingRevision?.availabilityStatus ?? item.externalProduct.currentRevision.availabilityStatus} / ${item.externalProduct.incomingRevision?.availableQuantity ?? item.externalProduct.currentRevision.availableQuantity}`,
                      },
                      {
                        id: "fp",
                        label: "数据版本（短）",
                        value:
                          item.externalProduct.incomingRevision
                            ?.contentFingerprintShort ??
                          item.externalProduct.currentRevision
                            .contentFingerprintShort ??
                          "—",
                      },
                      {
                        id: "sync",
                        label: "同步批次",
                        value: item.syncContext.sourceBatchIdentity,
                      },
                    ]}
                  />
                </CardContent>
              </Card>

              {item.offering && item.offering.revisionHistory.length > 0 ? (
                <Card size="sm">
                  <CardHeader className="border-b py-3">
                    <CardTitle className="text-base">
                      供给修订时间线（不可变）
                    </CardTitle>
                    <CardDescription>
                      供货价/关键供给变化形成新修订，不覆盖旧版；不自动改商城销售价
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="pt-4">
                    <RevisionTimeline
                      revisions={item.offering.revisionHistory.map((r, idx, arr) => ({
                        id: `off-r${r.revisionNo}`,
                        version: r.revisionNo,
                        source: "mall-sync" as const,
                        actor: "系统 · 供给修订",
                        isCurrent: idx === arr.length - 1,
                        status: {
                          label:
                            r.status === "ACTIVE"
                              ? "启用"
                              : r.status === "PAUSED"
                                ? "暂停"
                                : r.status === "STOPPED"
                                  ? "停止"
                                  : "待确认",
                          tone:
                            r.status === "ACTIVE"
                              ? ("success" as const)
                              : r.status === "STOPPED"
                                ? ("destructive" as const)
                                : ("warning" as const),
                        },
                        reason: `含税 ${r.supplyPriceGross ?? "—"} · MOQ ${r.minimumOrderQuantity} · ${r.supplyRegion.join("、")}`,
                        effectiveAt: {
                          dateTime: r.validFrom,
                          label: r.validFrom,
                        },
                      }))}
                    />
                    <p className="mt-3 text-xs text-muted-foreground">
                      供给 MOQ 是供应商约束，不等于商城最小购买量（
                      {String(item.publicationImpact.moqCopiedToMallMinPurchase)}{" "}
                      自动复制）。商城销售价自动更新：
                      {String(item.publicationImpact.mallSalePriceAutoUpdate)}。
                    </p>
                  </CardContent>
                </Card>
              ) : null}

              <Card size="sm">
                <CardHeader className="border-b py-3">
                  <CardTitle className="text-base">发布影响</CardTitle>
                  <CardDescription>
                    每个发布版本只绑定一条确定供给修订；无动态供应商路由
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-2 pt-4 text-sm">
                  <ul className="list-inside list-disc space-y-1 text-muted-foreground">
                    <li>
                      在售 {item.publicationImpact.activePublicationCount} ·
                      已暂停 {item.publicationImpact.pausedPublicationCount}
                    </li>
                    <li>
                      历史已支付订单{" "}
                      {item.publicationImpact.historicalPaidOrderCount}{" "}
                      （记录只读）
                    </li>
                  </ul>
                  {item.publicationImpact.pauseSubResults.map((p) => (
                    <div
                      key={p.id}
                      className="rounded-lg border px-3 py-2 text-xs"
                    >
                      {p.publicationId} · {p.reason} · outbox {p.outboxId} ·{" "}
                      {p.status}
                    </div>
                  ))}
                  {demoRole === "operations" ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      className="mt-2"
                      render={<Link href={w22Href} />}
                      disabled={
                        item.publicationImpact.safetyPauseTriggered ||
                        !item.mapping?.skuId
                      }
                    >
                      去商品发布
                      <ArrowRightIcon className="size-3.5" />
                    </Button>
                  ) : null}
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    className="mt-2"
                    disabled
                    aria-disabled="true"
                    title={RECOVERY_BLOCKER_MESSAGE}
                  >
                    发起商品发布恢复（阻断）
                  </Button>
                </CardContent>
              </Card>
            </div>

            {/* 决策侧栏 */}
            <div className="min-w-0 space-y-4">
              <Card size="sm">
                <CardHeader className="border-b py-3">
                  <CardTitle className="text-base">ERP SKU 映射</CardTitle>
                  <CardDescription>
                    同一外部商品同时点仅一个有效映射；一 SKU 可有多外部供给
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                  {item.mapping?.mappingStatus === "ACTIVE" ? (
                    <div className="rounded-lg border bg-muted/40 px-3 py-2 text-sm">
                      <div className="font-medium">
                        当前有效：{item.mapping.skuCode} ·{" "}
                        {item.mapping.skuName}
                      </div>
                      <p className="text-muted-foreground">
                        {item.mapping.specification} ·{" "}
                        {item.mapping.baseUnit} · v
                        {item.mapping.mappingVersion}
                      </p>
                    </div>
                  ) : (
                    <p className="text-sm text-muted-foreground">
                      尚无有效映射（待审核草稿不影响主数据）
                    </p>
                  )}

                  {item.skuCandidates.length > 0 ? (
                    <div className="space-y-2" role="radiogroup" aria-label="SKU 候选">
                      {item.skuCandidates.map((c) => (
                        <label
                          key={c.skuId}
                          className={cn(
                            "flex cursor-pointer gap-2 rounded-lg border px-3 py-2 text-sm",
                            selectedSkuId === c.skuId &&
                              "border-primary bg-primary/5"
                          )}
                        >
                          <input
                            type="radio"
                            name="sku-candidate"
                            className="mt-1"
                            checked={selectedSkuId === c.skuId}
                            onChange={() => setSelectedSkuId(c.skuId)}
                          />
                          <span>
                            <span className="font-medium">
                              {c.skuCode} · {c.skuName}
                            </span>
                            <span className="block text-muted-foreground">
                              {c.specification} · {c.similarityLabel}
                            </span>
                          </span>
                        </label>
                      ))}
                    </div>
                  ) : (
                    <p className="text-sm text-muted-foreground">
                      没有合适的现有 SKU
                    </p>
                  )}

                  {item.mapping?.history && item.mapping.history.length > 0 ? (
                    <div className="space-y-1 text-xs text-muted-foreground">
                      <p className="font-medium text-foreground">映射历史</p>
                      {item.mapping.history.map((h) => (
                        <p key={h.id}>
                          {h.at} · {h.skuCode} · {h.status} · {h.note}
                        </p>
                      ))}
                    </div>
                  ) : null}

                  <div className="flex flex-wrap gap-2">
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      render={<Link href={w14Href} />}
                    >
                      打开主数据新建/修订 SKU
                    </Button>
                    {/* 正式确认映射：未登记时禁用且不可聚焦为可用 */}
                    <Button
                      type="button"
                      size="sm"
                      disabled
                      tabIndex={-1}
                      aria-disabled="true"
                      title="WORK_ITEM_TYPE_UNREGISTERED：无写入入口"
                    >
                      确认映射（不可用）
                    </Button>
                  </div>
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader className="border-b py-3">
                  <CardTitle className="text-base">供给摘要 / 会话草稿</CardTitle>
                  <CardDescription>
                    仅会话草稿；供给修订类型登记前不可提交
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                  {item.offering?.currentRevision ? (
                    <DocumentSummary
                      columns="one"
                      items={[
                        {
                          id: "cur",
                          label: "当前供给修订",
                          value: `r${item.offering.currentRevision.revisionNo} · ${item.offering.currentRevision.status}`,
                        },
                        {
                          id: "price",
                          label: "含税价 / 税率",
                          value: `${item.offering.currentRevision.supplyPriceGross ?? "—"} / ${item.offering.currentRevision.inputTaxRate ?? "—"}`,
                          numeric: true,
                        },
                        {
                          id: "moq",
                          label: "MOQ（供给）",
                          value: item.offering.currentRevision.minimumOrderQuantity,
                          numeric: true,
                        },
                      ]}
                    />
                  ) : (
                    <p className="text-sm text-muted-foreground">
                      尚无供给修订
                    </p>
                  )}

                  {item.offering?.proposedDefaults ? (
                    <div className="space-y-2 rounded-lg border border-dashed px-3 py-2">
                      <p className="text-xs font-medium">
                        拟生效草稿（会话）· 含税{" "}
                        {item.offering.proposedDefaults.supplyPriceGross}
                      </p>
                      <div className="space-y-1">
                        <Label htmlFor="moq-draft">草稿 MOQ</Label>
                        <Input
                          id="moq-draft"
                          value={moqDraft}
                          onChange={(e) => setMoqDraft(e.target.value)}
                          className="h-8"
                          disabled={demoRole !== "procurement"}
                        />
                      </div>
                      <div className="space-y-1">
                        <Label htmlFor="draft-note">草稿备注</Label>
                        <Textarea
                          id="draft-note"
                          value={draftNote}
                          onChange={(e) => setDraftNote(e.target.value)}
                          rows={2}
                          disabled={demoRole !== "procurement"}
                        />
                      </div>
                    </div>
                  ) : null}

                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    disabled={
                      demoRole !== "procurement" || saveDraftMutation.isPending
                    }
                    onClick={() => void onSaveDraft()}
                  >
                    保存会话草稿
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    className="ml-2"
                    disabled
                    tabIndex={-1}
                    aria-disabled="true"
                    title="WORK_ITEM_TYPE_UNREGISTERED"
                  >
                    确认供给版本（不可用）
                  </Button>
                </CardContent>
              </Card>

              {item.changeType === "STOPPED" ? (
                <Card size="sm">
                  <CardHeader className="border-b py-3">
                    <CardTitle className="text-base">
                      替代候选（会话内）
                    </CardTitle>
                    <CardDescription>
                      仅证据准备；选定被 RECOVERY_RESPONSIBILITY_UNCONFIRMED
                      阻断
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-2 pt-4">
                    {item.skuCandidates
                      .filter((c) => c.skuId !== item.mapping?.skuId)
                      .map((c) => (
                        <label
                          key={c.skuId}
                          className="flex items-center gap-2 text-sm"
                        >
                          <input
                            type="checkbox"
                            checked={substituteIds.includes(c.skuId)}
                            onChange={(e) => {
                              setSubstituteIds((prev) =>
                                e.target.checked
                                  ? [...prev, c.skuId]
                                  : prev.filter((id) => id !== c.skuId)
                              )
                            }}
                          />
                          {c.skuCode} · {c.skuName}
                        </label>
                      ))}
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled
                      tabIndex={-1}
                      aria-disabled="true"
                      title={RECOVERY_BLOCKER_MESSAGE}
                    >
                      选定替代供给（阻断）
                    </Button>
                  </CardContent>
                </Card>
              ) : null}

              <Card size="sm">
                <CardHeader className="border-b py-3">
                  <CardTitle className="text-base">决策动作</CardTitle>
                  <CardDescription>
                    角色：{DEMO_ROLE_LABEL[demoRole]}
                    {isRegistered
                      ? " · 已注册异常可用任务内动作/终结"
                      : " · 仅浏览与草稿"}
                  </CardDescription>
                </CardHeader>
                <CardContent className="flex flex-wrap gap-2 pt-4">
                  {isRegistered && demoRole === "procurement" ? (
                    <>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={!leaseActive || formalPending}
                        onClick={() => setConfirmMode({ kind: "hold" })}
                      >
                        <PauseIcon className="size-3.5" />
                        暂挂
                      </Button>
                      {item.changeType === "ERROR" ? (
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          disabled={!leaseActive || formalPending}
                          onClick={() => setConfirmMode({ kind: "return" })}
                        >
                          退回数据修复
                        </Button>
                      ) : null}
                      <Button
                        type="button"
                        size="sm"
                        disabled={!leaseActive || formalPending}
                        onClick={() =>
                          setConfirmMode(
                            item.changeType === "ERROR"
                              ? { kind: "confirm_error" }
                              : { kind: "confirm_stop" }
                          )
                        }
                      >
                        <CircleCheckIcon className="size-3.5" />
                        {item.changeType === "ERROR"
                          ? "确认异常已解决"
                          : "确认停供记录"}
                      </Button>
                    </>
                  ) : null}
                  {(demoRole === "admin" || demoRole === "ops_tech") &&
                  item.changeType === "ERROR" ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      render={<Link href={w29Href} />}
                    >
                      进入接口错误中心（技术异常）
                    </Button>
                  ) : null}
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    render={<Link href={w20Href} />}
                  >
                    查看来源 API 连接
                  </Button>
                  {item.actionBlockers.slice(0, 4).map((b) => (
                    <p
                      key={`${b.action}-${b.code}`}
                      className="w-full text-xs text-muted-foreground"
                    >
                      <span className="font-medium text-foreground">
                        {b.code}
                      </span>
                      ：{b.message}
                    </p>
                  ))}
                </CardContent>
              </Card>
            </div>
          </div>
        </>
      ) : null}

      {/* 暂挂 */}
      <Dialog
        open={confirmMode?.kind === "hold"}
        onOpenChange={(open) => {
          if (!open) setConfirmMode(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>暂挂当前异常任务</DialogTitle>
            <DialogDescription>
              使用 WorkItemActionEnvelope；成功后任务仍为 PENDING/IN_PROGRESS，不自动下一项。
            </DialogDescription>
          </DialogHeader>
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault()
              void holdForm.handleSubmit()
            }}
          >
            <holdForm.AppField name="reasonCode">
              {(field) => (
                <div className="space-y-1">
                  <Label>原因</Label>
                  <OptionCombobox
                    value={field.state.value}
                    onValueChange={(v) =>
                      field.handleChange(
                        (v ?? field.state.value) as typeof field.state.value
                      )
                    }
                    options={HOLD_REASON_OPTIONS}
                    allowClear={false}
                    aria-label="暂挂原因"
                    placeholder="请选择原因"
                  />
                </div>
              )}
            </holdForm.AppField>
            <holdForm.AppField name="comment">
              {(field) => <field.TextareaField label="备注（可选）" />}
            </holdForm.AppField>
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                取消
              </DialogClose>
              <holdForm.AppForm>
                <holdForm.SubmitButton label="确认暂挂" />
              </holdForm.AppForm>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* 退回 */}
      <Dialog
        open={confirmMode?.kind === "return"}
        onOpenChange={(open) => {
          if (!open) setConfirmMode(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>退回数据修复</DialogTitle>
            <DialogDescription>
              不能通过业务确认修复来源错误；任务不终结。
            </DialogDescription>
          </DialogHeader>
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault()
              void returnForm.handleSubmit()
            }}
          >
            <returnForm.AppField name="reasonCode">
              {(field) => (
                <div className="space-y-1">
                  <Label>原因</Label>
                  <OptionCombobox
                    value={field.state.value}
                    onValueChange={(v) =>
                      field.handleChange(
                        (v ?? field.state.value) as typeof field.state.value
                      )
                    }
                    options={RETURN_REASON_OPTIONS}
                    allowClear={false}
                    aria-label="退回原因"
                    placeholder="请选择原因"
                  />
                </div>
              )}
            </returnForm.AppField>
            <returnForm.AppField name="comment">
              {(field) => <field.TextareaField label="说明" />}
            </returnForm.AppField>
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                取消
              </DialogClose>
              <returnForm.AppForm>
                <returnForm.SubmitButton label="确认退回" />
              </returnForm.AppForm>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <FormalActionConfirmDialog
        open={
          confirmMode?.kind === "confirm_error" ||
          confirmMode?.kind === "confirm_stop"
        }
        onOpenChange={(open) => {
          if (!open) setConfirmMode(null)
        }}
        title={
          confirmMode?.kind === "confirm_error"
            ? "确认异常已解决"
            : "确认停止供应记录"
        }
        description={
          confirmMode?.kind === "confirm_stop"
            ? "安全暂停已先发生。本动作仅确认停供记录与任务终态，不包含替代供给选定或恢复发布。"
            : "须存在数据修复证据边界；不写正常映射或供给修订。"
        }
        actionLabel="提交终结决策"
        confirmLabel="确认提交"
        fromStatus={{ label: "异常处理中", tone: "warning" }}
        toStatus={{ label: "任务已终结", tone: "success" }}
        lockedFields={[
          item ? `外部商品 ${item.externalProduct.externalProductId}` : "外部商品",
          `期望修订 r${expectedRevision}`,
          `数据版本 ${shortHash(subjectHash)}`,
        ]}
        effects={
          confirmMode?.kind === "confirm_stop"
            ? [
                "写入停供记录结论与审计",
                "完成 BUSINESS_EXCEPTION 任务",
                "不选定替代供给、不恢复上架",
              ]
            : [
                "写入异常已解决结论与审计",
                "完成 BUSINESS_EXCEPTION 任务",
                "不写正常映射或供给修订",
              ]
        }
        irreversibleEffects={["任务终态不可撤销（演示会话内）"]}
        pending={completeMutation.isPending}
        onConfirm={async () => {
          await completeForm.handleSubmit()
        }}
      />
    </div>
  )
}

type ExternalCatalogQueueQueryChangeType =
  | "actionable"
  | "NEW"
  | "CHANGED"
  | "STOPPED"
  | "ERROR"
  | "all"
