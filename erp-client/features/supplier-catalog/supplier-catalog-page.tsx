"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  CircleCheckIcon,
  ExternalLinkIcon,
  ImageIcon,
  PauseIcon,
  SearchIcon,
  ShieldAlertIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react"
import { z } from "zod"

import {
  BusinessDiffPanel,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  DataFreshness,
  DocumentSummary,
  FilterChip,
  FormalActionConfirmDialog,
  FormalActionResult,
  ListToolbar,
  OptionCombobox,
  PageHeader,
  PageScaffold,
  RevisionTimeline,
  SequentialProcessBar,
  surfaceInsetClassName,
  surfacePanelClassName,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
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
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"

import type {
  SupplierCatalogItemView,
  SupplierCatalogQueueQuery,
  SupplierCatalogSourceType,
  FormalOutcome,
} from "@/features/supplier-catalog/types"
import {
  CHANGE_TYPE_LABEL,
  HOLD_REASON_OPTIONS,
  RETURN_REASON_OPTIONS,
} from "@/features/supplier-catalog/types"
import {
  PromoteSupplierProductDialog,
  RegisterSupplyForSkuDialog,
  SupplierCatalogIntakeDialog,
} from "@/features/supplier-catalog/catalog-write-dialogs"
import {
  useClaimSupplierCatalogMutation,
  useCompleteSupplierCatalogMutation,
  useSupplierCatalogActionMutation,
  useSupplierCatalogQueueQuery,
  useSaveSupplierCatalogDraftMutation,
} from "@/features/supplier-catalog/queries"
import {
  offeringStatusLabel,
  SupplyRelationshipListView,
} from "@/features/supplier-catalog/supply-relationship-list-view"
import { cn } from "@/lib/utils"
import { compareDecimal } from "@/lib/fixed-decimal"

type SessionLease = {
  workItemId: string
  claimedByLabel: string
}

type ResultState = SharedResultState<FormalOutcome>

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

const decimalString = (label: string, maxScale: number, positive = false) =>
  z
    .string()
    .trim()
    .regex(
      new RegExp(`^\\d+(?:\\.\\d{1,${maxScale}})?$`),
      `${label}最多保留 ${maxScale} 位小数`
    )
    .refine((value) => !positive || /[1-9]/.test(value), `${label}必须大于 0`)

function decimalAtMost(value: string, maximum: string, maxScale: number) {
  try {
    return compareDecimal(value, maximum, maxScale) <= 0
  } catch {
    return false
  }
}

const offeringDraftSchema = z.object({
  supplyPriceGross: decimalString("含税供货价", 4, true),
  floorPriceGross: decimalString("含税底价", 4),
  dropshipExpress: z.string(),
  inputTaxRate: z.string().refine(
    (value) =>
      !value.trim() ||
      (/^\d+(?:\.\d{1,6})?$/.test(value.trim()) &&
        decimalAtMost(value, "1", 6)),
    "进项税率必须为 0 到 1 的十进制数，或留空待补充来源"
  ),
  freightAmount: decimalString("运费", 2),
  serviceFeeAmount: decimalString("服务费", 2),
  minimumOrderQuantity: decimalString("最小起订量", 6, true),
  supplyRegionText: z.string().trim().min(1, "请填写可供区域"),
  productCapabilitiesText: z.string(),
  validFrom: z.string().min(1, "请选择生效日期"),
  validTo: z.string(),
  note: z.string(),
})

function decisionLabel(kind: string) {
  if (kind === "CONFIRM_ERROR_RESOLVED") return "供应商数据已恢复"
  if (kind === "CONFIRM_STOP_SUPPLY") return "停供信息已确认"
  return "处理完成"
}

function actionLabel(kind: string) {
  if (kind === "HOLD") return "稍后处理"
  if (kind === "RETURN_FOR_DATA_FIX") return "已退回修正"
  if (kind === "QUERY_ORIGINAL_RESULT") return "已查询处理结果"
  if (kind === "SAVE_EVIDENCE") return "处理说明已保存"
  return "处理已记录"
}

function availabilityLabel(value: string) {
  if (value === "AVAILABLE") return "可供"
  if (value === "UNAVAILABLE") return "暂不可供"
  if (value === "STOPPED") return "已停供"
  if (value === "STALE") return "信息待更新"
  return value
}

function safetyReasonLabel(value: string) {
  if (value === "STOPPED") return "供应商停止供货"
  if (value === "UNAVAILABLE") return "当前不可供"
  if (value === "ZERO_STOCK") return "可供数量为零"
  if (value === "ZERO_INVENTORY") return "可供数量为零"
  if (value === "STALE") return "供货信息待更新"
  if (value === "PRICE_CHANGED") return "供货价待确认"
  if (value === "COST_CHANGE_UNCONFIRMED") return "供货价待确认"
  return value
}

function publicationPauseStatusLabel(status: string) {
  if (status === "PAUSED") return "已暂停"
  if (status === "ACTIVE") return "已恢复"
  return "处理中"
}

function mappingHistoryStatusLabel(status: string) {
  if (status === "ACTIVE") return "已生效"
  if (status === "REVOKED" || status === "DISABLED") return "已失效"
  return status
}

function isExceptionItem(
  item: SupplierCatalogItemView
): item is Extract<SupplierCatalogItemView, { changeType: "ERROR" | "STOPPED" }> {
  return item.changeType === "ERROR" || item.changeType === "STOPPED"
}

function changeTone(
  t: SupplierCatalogItemView["changeType"]
): "destructive" | "warning" | "info" | "neutral" {
  if (t === "STOPPED" || t === "ERROR") return "destructive"
  if (t === "CHANGED") return "warning"
  if (t === "NEW") return "info"
  return "neutral"
}

function buildQueueReturnHref(searchParams: URLSearchParams) {
  const qs = searchParams.toString()
  return qs ? `/procurement/supplier-catalog?${qs}` : "/procurement/supplier-catalog"
}

export function SupplierCatalogPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const mode: "list" | "queue" =
    searchParams.get("mode") === "queue" ? "queue" : "list"
  const skuId = searchParams.get("skuId") ?? undefined
  const returnToParam = searchParams.get("returnTo")
  const safeReturnTo =
    returnToParam?.startsWith("/master-data/products/") ||
    returnToParam === "/master-data/products"
      ? returnToParam
      : undefined

  const changeTypeParam = searchParams.get("changeType")
  const changeType: SupplierCatalogQueueQueryChangeType =
    changeTypeParam === "NEW" ||
    changeTypeParam === "CHANGED" ||
    changeTypeParam === "STOPPED" ||
    changeTypeParam === "ERROR" ||
    changeTypeParam === "all"
      ? changeTypeParam
      : mode === "list"
        ? "all"
        : "actionable"

  const statusParam = searchParams.get("status")
  const status: "pending" | "held" =
    statusParam === "held" ? "held" : "pending"

  const q = searchParams.get("q") ?? undefined
  const sourceTypeParam = searchParams.get("sourceType")
  const sourceType: SupplierCatalogSourceType | "all" =
    sourceTypeParam === "API" ||
    sourceTypeParam === "EXCEL" ||
    sourceTypeParam === "MANUAL"
      ? sourceTypeParam
      : "all"
  const currentSupplierProductId =
    searchParams.get("currentSupplierProductId") ?? undefined
  const currentWorkItemId =
    searchParams.get("currentWorkItemId") ?? undefined
  const queueContextId =
    searchParams.get("queueContextId") ??
    `queue:W21:${changeType}:${skuId ?? "all"}`

  const autoNextExplicit = searchParams.get("autoNext")
  const [sessionAutoNext, setSessionAutoNext] = React.useState(true)
  const autoNext =
    autoNextExplicit === "0"
      ? false
      : autoNextExplicit === "1"
        ? true
        : sessionAutoNext

  const filters = React.useMemo<SupplierCatalogQueueQuery>(() => {
    if (mode === "list") {
      // D8：list 模式只传列表维度参数；changeType 固定 all（保留全量含「无变化」），
      // queue 专属参数（status/currentSupplierProductId/currentWorkItemId/queueContextId）
      // 不传给 queryFn，避免从 queue 切回 list 时 URL 残留被静默消费。
      return { mode, skuId, q, sourceType, changeType: "all" }
    }
    return {
      mode,
      skuId,
      changeType,
      status,
      q,
      sourceType,
      currentSupplierProductId,
      currentWorkItemId,
      queueContextId,
    }
  }, [
    mode,
    skuId,
    q,
    sourceType,
    changeType,
    status,
    currentSupplierProductId,
    currentWorkItemId,
    queueContextId,
  ])

  const queueQuery = useSupplierCatalogQueueQuery(filters)
  const claimMutation = useClaimSupplierCatalogMutation()
  const actionMutation = useSupplierCatalogActionMutation()
  const completeMutation = useCompleteSupplierCatalogMutation()
  const saveDraftMutation = useSaveSupplierCatalogDraftMutation()
  const [excelImportOpen, setExcelImportOpen] = React.useState(false)
  const [manualEntryOpen, setManualEntryOpen] = React.useState(false)
  const [promotionItem, setPromotionItem] =
    React.useState<SupplierCatalogItemView | undefined>()

  const view = queueQuery.data
  const items = React.useMemo(() => [...(view?.items ?? [])], [view?.items])
  const context = view?.context
  const item =
    items.find((i) => {
      if (currentWorkItemId && isExceptionItem(i)) {
        return i.workItem.workItemId === currentWorkItemId
      }
      if (currentSupplierProductId) {
        return i.supplierProduct.id === currentSupplierProductId
      }
      return false
    }) ??
    view?.current ??
    items[0]
  const currentExceptionWorkItemId =
    item && isExceptionItem(item) ? item.workItem.workItemId : null

  const currentIndex = item
    ? Math.max(
        0,
        items.findIndex((i) => i.supplierProduct.id === item.supplierProduct.id)
      )
    : 0
  const completed = Boolean(view) && items.length === 0

  const [confirmMode, setConfirmMode] = React.useState<ConfirmMode>(null)
  const [lastResult, setLastResult] = React.useState<ResultState>(null)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [selectedSkuId, setSelectedSkuId] = React.useState<string>("")
  const [substituteIds, setSubstituteIds] = React.useState<string[]>([])
  const [searchInput, setSearchInput] = React.useState(q ?? "")

  // 浏览器后退/URL 变化时回填搜索输入框
  React.useEffect(() => {
    setSearchInput(q ?? "")
  }, [q])

  // D8：进入 list 模式时清理 queue 残留参数，避免深链参数滞留且无控件；
  // queue 模式由「URL defaults」effect 负责补齐，此处互不干扰。
  React.useEffect(() => {
    if (mode !== "list") return
    const staleKeys = [
      "changeType",
      "status",
      "autoNext",
      "currentSupplierProductId",
      "currentWorkItemId",
      "queueContextId",
    ]
    if (!staleKeys.some((key) => searchParams.has(key))) return
    const params = new URLSearchParams(searchParams.toString())
    for (const key of staleKeys) params.delete(key)
    const qs = params.toString()
    router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
  }, [mode, pathname, router, searchParams])

  // P3：list 模式提供 / 聚焦搜索快捷键
  React.useEffect(() => {
    if (mode !== "list") return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey)
        return
      const target = event.target as HTMLElement | null
      const tag = target?.tagName
      if (
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        target?.isContentEditable
      ) {
        return
      }
      event.preventDefault()
      document
        .querySelector<HTMLInputElement>('[data-slot="catalog-list-search"]')
        ?.focus()
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [mode])

  const headingRef = React.useRef<HTMLHeadingElement>(null)
  const resultRef = React.useRef<HTMLDivElement>(null)
  const leaseRef = React.useRef<SessionLease | null>(null)
  const [activeLease, setActiveLease] = React.useState<SessionLease | null>(null)

  const offeringForm = useAppForm({
    defaultValues: {
      supplyPriceGross: "",
      floorPriceGross: "",
      dropshipExpress: "",
      inputTaxRate: "",
      freightAmount: "0.00",
      serviceFeeAmount: "0.00",
      minimumOrderQuantity: "",
      supplyRegionText: "",
      productCapabilitiesText: "",
      validFrom: "",
      validTo: "",
      note: "",
    },
    validators: { onChange: offeringDraftSchema },
    onSubmit: async ({ value }) => {
      if (!item?.offering?.proposedDefaults) return
      try {
        await saveDraftMutation.mutateAsync({
          supplierCatalogSkuId:
            item.supplierProduct.catalogSkus?.[0]?.id ??
            `${item.supplierProduct.id}_sku`,
          selectedSkuId: selectedSkuId || undefined,
          offeringDraft: {
            supplyPriceGross: value.supplyPriceGross,
            floorPriceGross: value.floorPriceGross,
            dropshipExpress:
              value.dropshipExpress.trim() || undefined,
            inputTaxRate: value.inputTaxRate.trim(),
            freightAmount: value.freightAmount,
            serviceFeeAmount: value.serviceFeeAmount,
            minimumOrderQuantity: value.minimumOrderQuantity,
            supplyRegion: value.supplyRegionText
              .split(/[，,]/)
              .map((entry) => entry.trim())
              .filter(Boolean),
            productCapabilities: value.productCapabilitiesText
              .split(/[，,]/)
              .map((entry) => entry.trim())
              .filter(Boolean),
            validFrom: value.validFrom,
            validTo: value.validTo || undefined,
            sessionDraftOnly: true,
          },
          substituteCandidateSkuIds: substituteIds,
          note: value.note.trim() || undefined,
        })
        setLastResult({
          status: "blocked",
          title: "草稿已保存",
          description:
            "草稿仅保存在当前页面，尚未改变 ERP 商品关联、供货条件或商城商品。",
          stayOnItem: true,
          terminal: false,
        })
      } catch (error) {
        setActionError(error instanceof Error ? error.message : "保存草稿失败")
      }
    },
  })

  // 切换队列项时重置会话草稿 UI（与 W13 等队列页同一模式）
   
  React.useEffect(() => {
    if (!item) return
    const proposed = item.offering?.proposedDefaults
    const contextSkuId =
      skuId &&
      (item.mapping?.skuId === skuId ||
        item.skuCandidates.some((candidate) => candidate.skuId === skuId))
        ? skuId
        : undefined
    setSelectedSkuId(
      contextSkuId ?? item.mapping?.skuId ?? item.skuCandidates[0]?.skuId ?? ""
    )
    offeringForm.reset({
      supplyPriceGross: proposed?.supplyPriceGross ?? "",
      floorPriceGross: proposed?.floorPriceGross ?? "",
      dropshipExpress: proposed?.dropshipExpress ?? "",
      inputTaxRate: proposed?.inputTaxRate ?? "",
      freightAmount: proposed?.freightAmount ?? "0.00",
      serviceFeeAmount: proposed?.serviceFeeAmount ?? "0.00",
      minimumOrderQuantity: proposed?.minimumOrderQuantity ?? "",
      supplyRegionText: proposed?.supplyRegion.join("、") ?? "",
      productCapabilitiesText: proposed?.productCapabilities.join("、") ?? "",
      validFrom: proposed?.validFrom ?? "",
      validTo: proposed?.validTo ?? "",
      note: "",
    })
    setSubstituteIds([])
    setActionError(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅按供应商商品身份重置草稿
  }, [item?.supplierProduct.id, skuId])
   

  // URL defaults
  React.useEffect(() => {
    if (mode === "list") return
    if (queueQuery.isPending || !view) return
    const hasChange = searchParams.has("changeType")
    const hasCtx = searchParams.has("queueContextId")
    const hasItem =
      searchParams.has("currentSupplierProductId") ||
      searchParams.has("currentWorkItemId")
    if (hasChange && hasCtx && (hasItem || items.length === 0)) return
    const params = new URLSearchParams(searchParams.toString())
    if (!hasChange) params.set("changeType", changeType)
    if (!hasCtx) params.set("queueContextId", queueContextId)
    if (!hasItem && item) {
      params.set("currentSupplierProductId", item.supplierProduct.id)
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
    mode,
  ])

  // Auto-claim registered exception tasks only
  React.useEffect(() => {
    if (mode === "list") return
    if (!item || !isExceptionItem(item)) return
    if (leaseRef.current?.workItemId === item.workItem.workItemId) return
    if (claimMutation.isPending) return
    let cancelled = false
    void claimMutation
      .mutateAsync(item.workItem.workItemId)
      .then((lease) => {
        if (cancelled) return
        const session: SessionLease = {
          workItemId: lease.workItemId,
          claimedByLabel: lease.claimedByLabel,
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
  }, [
    currentExceptionWorkItemId,
    mode,
  ])

  React.useEffect(() => {
    if (mode === "list") return
    if (lastResult) {
      resultRef.current?.focus()
    } else if (item) {
      headingRef.current?.focus()
    }
  }, [item, lastResult, mode])

  const replaceUrl = React.useCallback(
    (patch: Record<string, string | null | undefined>) => {
      const params = new URLSearchParams(searchParams.toString())
      for (const [key, value] of Object.entries(patch)) {
        if (value == null || value === "") params.delete(key)
        else params.set(key, value)
      }
      if (
        mode === "queue" &&
        !params.has("queueContextId") &&
        !Object.prototype.hasOwnProperty.call(patch, "queueContextId")
      ) {
        params.set("queueContextId", queueContextId)
      }
      const qs = params.toString()
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    },
    [mode, pathname, queueContextId, router, searchParams]
  )

  // P3：list 模式搜索 300ms 防抖即时写 URL（replace）；Enter 提交由视图兜底。
  // 定义在 replaceUrl 之后（依赖其引用）。
  React.useEffect(() => {
    if (mode !== "list") return
    const handle = globalThis.setTimeout(() => {
      const next = searchInput.trim()
      if (next === (q ?? "")) return
      replaceUrl({ q: next || null })
    }, 300)
    return () => globalThis.clearTimeout(handle)
  }, [mode, q, replaceUrl, searchInput])

  const goToItem = React.useCallback(
    (next: SupplierCatalogItemView | undefined | null) => {
      setLastResult(null)
      setActionError(null)
      if (!next) {
        replaceUrl({
          currentSupplierProductId: null,
          currentWorkItemId: null,
          queueContextId,
        })
        return
      }
      replaceUrl({
        currentSupplierProductId: next.supplierProduct.id,
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
    if (existing && existing.workItemId === wi.workItemId) {
      return existing
    }
    const lease = await claimMutation.mutateAsync(wi.workItemId)
    const session: SessionLease = {
      workItemId: lease.workItemId,
      claimedByLabel: lease.claimedByLabel,
    }
    leaseRef.current = session
    setActiveLease(session)
    return session
  }, [claimMutation, item])

  const exceptionWorkItem =
    item && isExceptionItem(item) ? item.workItem : null
  const workItemId = exceptionWorkItem?.workItemId ?? null
  const expectedRevision = item
    ? String(
        item.supplierProduct.incomingRevision?.revisionNo ??
          item.supplierProduct.currentRevision.revisionNo
      )
    : ""

  const formalPending =
    actionMutation.isPending || completeMutation.isPending

  const leaseActive =
    Boolean(workItemId) &&
    activeLease?.workItemId === workItemId

  const leaseStatus = !workItemId
    ? ("unclaimed" as const)
    : leaseActive
      ? ("active" as const)
      : ("unclaimed" as const)

  const canProcureWrite =
    Boolean(workItemId) && leaseActive

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
        await ensureLease()
        const result = await actionMutation.mutateAsync({
          workItemId: item.workItem.workItemId,
          action: {
            kind: "HOLD",
            reasonCode: value.reasonCode,
            comment: value.comment || undefined,
          },
        })
        setConfirmMode(null)
        if (result.status === "failed") {
          setActionError(result.message)
          return
        }
        if (result.outcome.kind !== "ACTION") return
        setLastResult({
          status: "blocked",
          title: "已设为稍后处理",
          description: result.outcome.resumeHint,
          reference: result.outcome.reference,
          outcome: result.outcome,
          stayOnItem: true,
          terminal: false,
        })
        // 不自动下一项
      } catch (e) {
        setActionError(e instanceof Error ? e.message : "稍后处理失败")
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
        await ensureLease()
        const result = await actionMutation.mutateAsync({
          workItemId: item.workItem.workItemId,
          action: {
            kind: "RETURN_FOR_DATA_FIX",
            reasonCode: value.reasonCode,
            comment: value.comment,
          },
        })
        setConfirmMode(null)
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
        await ensureLease()
        const result = await completeMutation.mutateAsync({
          workItemId: item.workItem.workItemId,
          decision:
            decisionKind === "CONFIRM_ERROR_RESOLVED"
              ? {
                  kind: "CONFIRM_ERROR_RESOLVED",
                  expectedSourceRevision: expectedRevision,
                  resolutionCode: "SOURCE_FIXED",
                  comment: value.comment,
                }
              : {
                  kind: "CONFIRM_STOP_SUPPLY",
                  expectedSourceRevision: expectedRevision,
                  expectedOfferingRevision: item.offering?.currentRevision
                    ? String(item.offering.currentRevision.revisionNo)
                    : undefined,
                  reasonCode: "SUPPLIER_STOPPED",
                  comment: value.comment,
                },
        })
        setConfirmMode(null)
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
              ? "供应商数据异常已解决"
              : "供应商停供信息已确认",
          description:
            "处理结果已记录。你可以继续下一项；本次没有选择替代供应商，也没有恢复商城销售。",
          reference:
            result.outcome.kind === "COMPLETED"
              ? result.outcome.business.reference
              : undefined,
          outcome: result.outcome,
          terminal: true,
        })
        if (autoNext) {
          // 「完成后显示下一项」开启：结果面板提供「下一项」按钮，用户点击后再跳
        }
      } catch (e) {
        setActionError(e instanceof Error ? e.message : "终结失败")
      }
    },
  })

  const w14Href = item
    ? `/master-data/products?from=W21&supplierProductId=${encodeURIComponent(item.supplierProduct.id)}&returnTo=${encodeURIComponent(buildQueueReturnHref(searchParams))}&queueContextId=${encodeURIComponent(queueContextId)}`
    : "/master-data"
  const w20Href = item?.supplierProduct.source.connection
    ? `/supplier-api/connections?connectionId=${encodeURIComponent(item.supplierProduct.source.connection.id)}&returnTo=${encodeURIComponent(buildQueueReturnHref(searchParams))}`
    : "/supplier-api/connections"
  const centerHref = item
    ? `/procurement/supplier-catalog/${item.supplierProduct.id}?section=overview&queueContextId=${encodeURIComponent(queueContextId)}&returnTo=${encodeURIComponent(buildQueueReturnHref(searchParams))}`
    : "#"

  const displayedSourceRevision = item
    ? item.supplierProduct.incomingRevision ??
      item.supplierProduct.currentRevision
    : undefined

  if (queueQuery.isPending) {
    return (
      <PageScaffold>
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-16 animate-pulse rounded-lg bg-muted" />
        <div className="grid gap-4 xl:grid-cols-[minmax(0,58fr)_minmax(16rem,42fr)]">
          <div className="h-80 animate-pulse rounded-lg bg-muted" />
          <div className="h-80 animate-pulse rounded-lg bg-muted" />
        </div>
      </PageScaffold>
    )
  }

  if (queueQuery.isError) {
    return (
      <PageScaffold>
        <PageHeader
          title={mode === "list" ? "供应商商品库" : "供应商商品待处理"}
          description="加载失败"
        />
        <BusinessFailureState
          kind="system"
          title="数据加载失败"
          description="未能加载供应商商品数据。请重试；若持续失败，可稍后再来。"
          onRetry={() => void queueQuery.refetch()}
          action={
            <Button
              variant="outline"
              size="sm"
              render={<Link href="/workspace" />}
            >
              返回今日工作台
            </Button>
          }
        />
      </PageScaffold>
    )
  }

  if (mode === "list") {
    return (
      <>
        <SupplyRelationshipListView
          items={items}
          skuId={skuId}
          skuContext={view?.skuContext}
          returnTo={safeReturnTo}
          returnHref={buildQueueReturnHref(searchParams)}
          updatedAt={context?.queueContextUpdatedAt}
          emptyReason={view?.emptyReason}
          sort={searchParams.get("sort") ?? undefined}
          onSortChange={(next) =>
            replaceUrl({
              sort: next || null,
              currentSupplierProductId: null,
            })
          }
          onOpenItem={(target) =>
            router.push(
              `/procurement/supplier-catalog/${target.supplierProduct.id}?section=overview&returnTo=${encodeURIComponent(buildQueueReturnHref(searchParams))}`
            )
          }
          searchInput={searchInput}
          onSearchInputChange={setSearchInput}
          onSearch={() =>
            replaceUrl({
              q: searchInput.trim() || null,
              currentSupplierProductId: null,
              currentWorkItemId: null,
            })
          }
          sourceType={sourceType}
          onSourceTypeChange={(next) =>
            replaceUrl({
              sourceType: next === "all" ? null : next,
              currentSupplierProductId: null,
            })
          }
          onClearFilters={() =>
            replaceUrl({
              q: null,
              sourceType: null,
              skuId: null,
              currentSupplierProductId: null,
              currentWorkItemId: null,
              queueContextId: null,
            })
          }
          onClearSku={() =>
            replaceUrl({
              skuId: null,
              currentSupplierProductId: null,
              currentWorkItemId: null,
              queueContextId: null,
            })
          }
          onOpenExcelImport={() => setExcelImportOpen(true)}
          onOpenManualEntry={() => setManualEntryOpen(true)}
          onPromote={setPromotionItem}
        />
        <SupplierCatalogIntakeDialog
          open={excelImportOpen}
          onOpenChange={setExcelImportOpen}
          sourceType="EXCEL"
        />
        {view?.skuContext ? (
          <RegisterSupplyForSkuDialog
            key={view.skuContext.skuId}
            open={manualEntryOpen}
            onOpenChange={setManualEntryOpen}
            fixedSku={{
              skuId: view.skuContext.skuId,
              skuCode: view.skuContext.skuCode,
              skuName: view.skuContext.productName,
              specification: view.skuContext.specification,
              baseUnit: view.skuContext.baseUnit,
              category: view.skuContext.category,
              brand: view.skuContext.brand,
              barcode: view.skuContext.barcode,
              description: view.skuContext.description,
              carouselImages: view.skuContext.carouselImages,
              detailImages: view.skuContext.detailImages,
              mainImage: view.skuContext.mainImage,
              salesVisiblePriceGross: view.skuContext.poolEntry?.salesVisiblePriceGross,
              hasPoolEntry: Boolean(view.skuContext.poolEntry),
            }}
          />
        ) : (
          <SupplierCatalogIntakeDialog
            open={manualEntryOpen}
            onOpenChange={setManualEntryOpen}
            sourceType="MANUAL"
          />
        )}
        <PromoteSupplierProductDialog
          key={promotionItem?.supplierProduct.id ?? "promote-supplier-product"}
          item={promotionItem}
          open={Boolean(promotionItem)}
          onOpenChange={(open) => {
            if (!open) setPromotionItem(undefined)
          }}
        />
      </>
    )
  }

  return (
    <PageScaffold>
      <PageHeader
        title="供应商商品变化待处理"
        description="处理 Excel、API 与手工来源中的新增、关键变化、停供和数据异常"
        breadcrumbs={[
          { id: "catalog", label: "供应商商品库", href: "/procurement/supplier-catalog" },
          { id: "queue", label: "来源变化待处理", current: true },
        ]}
        metadata={
          <DataFreshness
            state="fresh"
            label="供给信息更新时间"
            updatedAt={
              context?.filterSummary
                ? `${context.filterSummary} · ${formatDateTime(context.queueContextUpdatedAt, "default")}`
                : formatDateTime(context?.queueContextUpdatedAt, "default")
            }
            dateTime={context?.queueContextUpdatedAt}
          />
        }
        actions={
          safeReturnTo ? (
            <Button variant="ghost" size="sm" render={<Link href={safeReturnTo} />}>
              返回商品与 SKU
            </Button>
          ) : undefined
        }
      />

      <div
        className={cn(
          surfacePanelClassName,
          "sticky top-0 z-10 space-y-2.5 px-3 py-2.5 text-sm"
        )}
      >
        <div className="flex flex-wrap items-center gap-2">
          <div
            role="group"
            aria-label="队列范围"
            className="inline-flex rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
          >
            <button
              type="button"
              aria-pressed={status === "pending"}
              className={cn(
                "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all",
                status === "pending"
                  ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                  : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
              )}
              onClick={() =>
                replaceUrl({
                  status: null,
                  currentSupplierProductId: null,
                  currentWorkItemId: null,
                })
              }
            >
              待处理
            </button>
            <button
              type="button"
              aria-pressed={status === "held"}
              className={cn(
                "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all",
                status === "held"
                  ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                  : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
              )}
              onClick={() =>
                replaceUrl({
                  status: "held",
                  currentSupplierProductId: null,
                  currentWorkItemId: null,
                })
              }
            >
              稍后处理
            </button>
          </div>
          <OptionCombobox
            value={changeType}
            options={[
              { value: "actionable", label: "需处理" },
              { value: "STOPPED", label: CHANGE_TYPE_LABEL.STOPPED },
              { value: "ERROR", label: CHANGE_TYPE_LABEL.ERROR },
              { value: "NEW", label: CHANGE_TYPE_LABEL.NEW },
              { value: "CHANGED", label: CHANGE_TYPE_LABEL.CHANGED },
              { value: "all", label: "全部" },
            ]}
            allowClear={false}
            size="sm"
            aria-label="变化类型"
            inputClassName="w-[8.5rem]"
            onValueChange={(v) => {
              const next = (v as typeof changeType | null) ?? "actionable"
              replaceUrl({
                changeType: next === "actionable" ? "actionable" : next,
                currentSupplierProductId: null,
                currentWorkItemId: null,
              })
            }}
          />
        </div>
        <ListToolbar
          aria-label="商品库队列筛选"
          search={
            <form
              onSubmit={(e) => {
                e.preventDefault()
                replaceUrl({
                  q: searchInput.trim() || null,
                  currentSupplierProductId: null,
                  currentWorkItemId: null,
                })
              }}
            >
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  value={searchInput}
                  onChange={(e) => setSearchInput(e.target.value)}
                  placeholder="供应商商品编码 / SKU / 名称"
                  aria-label="搜索"
                />
              </InputGroup>
            </form>
          }
          secondary={
            skuId ? (
              <FilterChip
                label={
                  view?.skuContext
                    ? `${view.skuContext.productName}（${view.skuContext.skuCode}）`
                    : "商品规格筛选"
                }
                onClear={() =>
                  replaceUrl({
                    skuId: null,
                    currentSupplierProductId: null,
                    currentWorkItemId: null,
                    queueContextId: null,
                  })
                }
                clearLabel="清除 SKU 筛选"
              />
            ) : undefined
          }
          actions={
            <div className="flex items-center gap-2">
              <Label htmlFor="w21-auto-next" className="text-muted-foreground">
                完成后显示下一项
              </Label>
              <Switch
                id="w21-auto-next"
                checked={autoNext}
                onCheckedChange={(on) => {
                  setSessionAutoNext(on)
                  replaceUrl({ autoNext: on ? "1" : "0" })
                }}
                aria-describedby="w21-auto-next-hint"
              />
              <span id="w21-auto-next-hint" className="sr-only">
                开启后，处理完成的结果面板会提供「下一项」按钮
              </span>
            </div>
          }
        />
      </div>

      {lastResult ? (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
          <FormalActionResult
            status={lastResult.status === "failed" ? "blocked" : lastResult.status}
            title={lastResult.title}
            description={lastResult.description}
            reference={lastResult.reference}
            facts={
              lastResult.outcome?.kind === "COMPLETED"
                ? [
                    {
                      label: "处理结论",
                      value: decisionLabel(
                        lastResult.outcome.business.decisionKind
                      ),
                    },
                    {
                      label: "记录编号",
                      value: lastResult.outcome.business.reference,
                    },
                    {
                      label: "完成时间",
                      value: formatDateTime(lastResult.outcome.business.completedAt, "default"),
                    },
                  ]
                : lastResult.outcome?.kind === "ACTION"
                  ? [
                      {
                        label: "动作",
                        value: actionLabel(lastResult.outcome.actionKind),
                      },
                      {
                        label: "任务状态",
                        value:
                          lastResult.outcome.workItemStatus === "PENDING"
                            ? "待处理"
                            : "处理中",
                      },
                      {
                        label: "下一步",
                        value: "继续处理当前供应商商品",
                      },
                    ]
                  : undefined
            }
            actions={
              <div className="flex flex-wrap gap-2">
                {lastResult.terminal || autoNext ? (
                  <Button
                    type="button"
                    onClick={() => {
                      const next =
                        neighbor(1) ??
                        items.find(
                          (i) =>
                            i.supplierProduct.id !==
                            item?.supplierProduct.id
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
        view?.emptyReason === "FILTER_NO_RESULT" ? (
          <BusinessEmptyState
            kind="filter"
            title="当前筛选没有结果"
            description="没有符合变化类型、待处理范围或搜索条件的供应商商品，可调整筛选后重试。"
            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
            action={
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="secondary"
                  className="rounded-lg shadow-none"
                  onClick={() =>
                    replaceUrl({
                      changeType: "actionable",
                      status: null,
                      q: null,
                      currentSupplierProductId: null,
                      currentWorkItemId: null,
                    })
                  }
                >
                  清除筛选
                </Button>
                <Button
                  variant="secondary"
                  className="rounded-lg shadow-none"
                  render={<Link href="/workspace" />}
                >
                  返回今日工作台
                </Button>
              </div>
            }
          />
        ) : view?.emptyReason === "NO_DATA_SCOPE" ? (
          <BusinessEmptyState
            kind="no-scope"
            title="当前角色无数据范围"
            description="你可以进入此页面，但当前角色范围内没有可查看的供应商商品。"
            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
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
            title="当前队列已处理完"
            description="可切换变化类型或稍后处理范围，也可以返回工作台。"
            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
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
        )
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
                : "无需领取 · 可查看资料并准备草稿"
            }
            processLabel={
              isRegistered
                ? item.changeType === "ERROR"
                  ? "确认异常已解决"
                  : "确认停供记录"
                : "确认处理（暂未开放）"
            }
            // 没有独立的「并准备下一项」路径：两个 handler 同义
            showProcessNext={false}
            backLabel="返回工作台"
            processDisabled={
              formalPending ||
              !isRegistered ||
              !canProcureWrite
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
              {item.supplierProduct.supplier.name}
            </span>
          </div>

          {/* 身份与风险条 */}
          <Card size="sm" className={surfacePanelClassName}>
            <CardContent className="flex flex-wrap items-start gap-3 py-3">
              <div className="min-w-0 flex-1 space-y-1">
                <div className="flex flex-wrap items-center gap-2">
                  <h2
                    ref={headingRef}
                    tabIndex={-1}
                    className="font-heading text-lg font-semibold outline-none"
                  >
                    {item.supplierProduct.currentRevision.name}
                  </h2>
                  <BusinessStatusBadge
                    context="list"
                    label={CHANGE_TYPE_LABEL[item.changeType]}
                    tone={changeTone(item.changeType)}
                  />
                  {isExceptionItem(item) && item.workItem.held ? (
                    <BusinessStatusBadge
                      context="list"
                      label="稍后处理"
                      tone="warning"
                    />
                  ) : null}
                </div>
                <p className="text-sm text-muted-foreground">
                  供应商商品编码 {item.supplierProduct.supplierSpuCode ?? "未提供 SPU"}
                  {item.supplierProduct.supplierSkuCode
                    ? ` / ${item.supplierProduct.supplierSkuCode}`
                    : ""}{" "}
                  · 供应商 {item.supplierProduct.supplier.name} · 最近更新{" "}
                  {formatDateTime(item.sourceContext.receivedAt, "default")}
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
              <AlertTitle>相关商城商品已暂停销售</AlertTitle>
              <AlertDescription className="space-y-1">
                <p>{item.publicationImpact.note}</p>
                <p>
                  原因：
                  {item.publicationImpact.safetyPauseReasons
                    .map(safetyReasonLabel)
                    .join("、")} ·
                  已暂停发布 {item.publicationImpact.pausedPublicationCount} ·
                  历史已支付{" "}
                  {item.publicationImpact.historicalPaidOrderCount} 笔历史记录保留
                </p>
                {item.publicationImpact.recoveryBlocker ? (
                  <p className="font-medium">
                    尚未确定恢复销售的负责人，当前只能查看影响和准备替代方案，暂不能恢复销售。
                  </p>
                ) : null}
              </AlertDescription>
            </Alert>
          ) : null}

          {hasRegistrationBlocker && item.registrationBlocker ? (
            <Alert>
              <TriangleAlertIcon aria-hidden="true" />
              <AlertTitle>当前暂不能提交确认</AlertTitle>
              <AlertDescription>
                {item.registrationBlocker.businessProcess === "MAPPING"
                  ? "商品关联功能正在配置。你可以查看供应商商品、比较 ERP SKU，并保存草稿。"
                  : "供货条件确认功能正在配置。你可以查看变化并保存草稿，暂不能提交生效。"}
              </AlertDescription>
            </Alert>
          ) : null}

          {/* 双栏：来源 diff ~58% | 映射与供给决策 ~42% */}
          <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,58fr)_minmax(17rem,42fr)]">
            <div className="min-w-0 space-y-4">
              <BusinessDiffPanel
                title="供应商更新内容"
                caption="对比原数据与供应商最新数据；成本字段按权限显示"
                changes={item.sourceDiff.map((c) => ({
                  id: c.id,
                  field: c.field,
                  before:
                    c.field === "可供状态"
                      ? availabilityLabel(c.before)
                      : c.before,
                  after:
                    c.field === "可供状态"
                      ? availabilityLabel(c.after)
                      : c.after,
                  note: c.note,
                }))}
              />

              <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30 py-3">
                  <CardTitle className="text-base">供应商商品资料</CardTitle>
                  <CardDescription>
                    供应商更新不会直接修改 ERP 商品或商城商品，确认后才会生效
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
                          item.supplierProduct.incomingRevision?.name ??
                          item.supplierProduct.currentRevision.name,
                        emphasized: true,
                      },
                      {
                        id: "spec",
                        label: "规格",
                        value:
                          item.supplierProduct.incomingRevision
                            ?.specification ||
                          item.supplierProduct.currentRevision.specification ||
                          "—",
                      },
                      {
                        id: "brand",
                        label: "来源品牌",
                        value: displayedSourceRevision?.brand || "—",
                      },
                      {
                        id: "unit",
                        label: "来源单位",
                        value: displayedSourceRevision?.baseUnit || "—",
                      },
                      {
                        id: "barcode",
                        label: "商品条码",
                        value: displayedSourceRevision?.barcode || "—",
                      },
                      {
                        id: "attributes",
                        label: "规格属性",
                        value:
                          displayedSourceRevision?.attributes
                            ?.map((attribute) => `${attribute.name}：${attribute.value}`)
                            .join(" / ") || "—",
                      },
                      {
                        id: "dropship",
                        label: "一件代发底价（含税运）",
                        value:
                          item.supplierProduct.incomingRevision
                            ?.dropshipFloorPriceGross ??
                          item.supplierProduct.currentRevision
                            .dropshipFloorPriceGross ??
                          "—",
                        numeric: true,
                      },
                      {
                        id: "bulk",
                        label: "集采底价（含税）",
                        value:
                          item.supplierProduct.incomingRevision
                            ?.bulkFloorPriceGross ??
                          item.supplierProduct.currentRevision
                            .bulkFloorPriceGross ??
                          "—",
                        numeric: true,
                      },
                      {
                        id: "moq",
                        label: "集采起订量",
                        value:
                          item.supplierProduct.incomingRevision
                            ?.bulkMinimumOrderQuantity ??
                          item.supplierProduct.currentRevision
                            .bulkMinimumOrderQuantity ??
                          "—",
                        numeric: true,
                      },
                      {
                        id: "avail",
                        label: "可供状态 / 数量",
                        value: `${availabilityLabel(item.supplierProduct.incomingRevision?.availabilityStatus ?? item.supplierProduct.currentRevision.availabilityStatus)} / ${item.supplierProduct.incomingRevision?.availableQuantity ?? item.supplierProduct.currentRevision.availableQuantity}`,
                      },
                    ]}
                  />
                  <div className="mt-4 border-t pt-4">
                    <div className="flex items-center gap-2 text-sm font-medium">
                      <ImageIcon className="size-4" aria-hidden />
                      来源图文
                    </div>
                    {displayedSourceRevision?.media?.length ? (
                      <div className="mt-2 flex flex-wrap gap-2">
                        {displayedSourceRevision.media.map((media) => (
                          <Badge key={media.id} variant="outline">
                            {media.usage === "SKU_MAIN"
                              ? "SKU 主图"
                              : media.usage === "SPU_CAROUSEL"
                                ? "轮播图"
                                : "详情图"}
                            ：{media.fileName}
                            {media.archiveStatus !== "ARCHIVED"
                              ? "（待归档）"
                              : ""}
                          </Badge>
                        ))}
                      </div>
                    ) : (
                      <p className="mt-2 text-sm text-muted-foreground">
                        来源未提供图片；首次创建公司商品时必须补齐启用 SKU 主图。
                      </p>
                    )}
                  </div>
                </CardContent>
              </Card>

              {item.offering && item.offering.revisionHistory.length > 0 ? (
                <Card size="sm" className={surfacePanelClassName}>
                  <CardHeader className="border-b border-border/30 py-3">
                    <CardTitle className="text-base">
                      供货条件历史
                    </CardTitle>
                    <CardDescription>
                      价格或供货条件变化会保留历史版本，不会覆盖原记录
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="pt-4">
                    <RevisionTimeline
                      revisions={item.offering.revisionHistory.map((r, idx, arr) => ({
                        id: `off-r${r.revisionNo}`,
                        version: r.revisionNo,
                        source: "mall-sync" as const,
                        actor: "系统记录",
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
                        reason: `含税价 ${r.supplyPriceGross ?? "—"} · 最小起订量 ${r.minimumOrderQuantity} · ${r.supplyRegion.join("、")}`,
                        effectiveAt: {
                          dateTime: r.validFrom,
                          label: r.validFrom,
                        },
                      }))}
                    />
                    <p className="mt-3 text-xs text-muted-foreground">
                      最小起订量是供应商的供货要求，不会复制为商城最小购买量；供货价变化也不会自动修改商城销售价。
                    </p>
                  </CardContent>
                </Card>
              ) : null}

              <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30 py-3">
                  <CardTitle className="text-base">发布影响</CardTitle>
                  <CardDescription>
                    每个商城商品版本使用一条确定的供给，系统不会自动切换供应商
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
                      className={cn(surfaceInsetClassName, "px-3 py-2 text-xs")}
                    >
                      {safetyReasonLabel(p.reason)} ·{" "}
                      {publicationPauseStatusLabel(p.status)}
                    </div>
                  ))}
                  <div className="space-y-1">
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      className="mt-2"
                      disabled
                    >
                      恢复商城销售
                    </Button>
                    <p className="text-xs text-muted-foreground">
                      恢复销售的负责人尚未确定，当前只能准备候选方案。
                    </p>
                  </div>
                </CardContent>
              </Card>
            </div>

            {/* 决策侧栏 */}
            <div className="min-w-0 space-y-4">
              <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30 py-3">
                  <CardTitle className="text-base">关联 ERP 商品规格</CardTitle>
                  <CardDescription>
                    一个供应商商品只能关联一个 ERP SKU；同一 SKU 可以有多个供应商
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                  {item.mapping?.mappingStatus === "ACTIVE" ? (
                    <div className={cn(surfaceInsetClassName, "px-3 py-2 text-sm")}>
                      <div className="font-medium">
                        当前有效：{item.mapping.skuCode} ·{" "}
                        {item.mapping.skuName}
                      </div>
                      <p className="text-muted-foreground">
                        {item.mapping.specification} ·{" "}
                        {item.mapping.baseUnit}
                        {item.mapping.approvedAt
                          ? ` · ${formatDateTime(item.mapping.approvedAt, "default")} 确认`
                          : ""}
                      </p>
                    </div>
                  ) : (
                    <p className="text-sm text-muted-foreground">
                      尚无有效映射（待审核草稿不影响基础资料）
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
                          {h.at} · {h.skuCode} ·{" "}
                          {mappingHistoryStatusLabel(h.status)} · {h.note}
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
                      前往商品资料
                    </Button>
                    {/* 正式确认映射：未登记时禁用且不可聚焦为可用 */}
                    <div className="space-y-1">
                      <Button
                        type="button"
                        size="sm"
                        disabled
                        tabIndex={-1}
                        aria-disabled="true"
                      >
                        确认关联
                      </Button>
                      <p className="text-xs text-muted-foreground">
                        商品关联确认功能暂未开放，可先查看资料并保存草稿。
                      </p>
                    </div>
                  </div>
                </CardContent>
              </Card>

              <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30 py-3">
                  <CardTitle className="text-base">供货条件</CardTitle>
                  <CardDescription>
                    可准备价格、起订量、区域和有效期草稿；确认功能暂未开放
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                  {item.offering?.currentRevision ? (
                    <DocumentSummary
                      columns="one"
                      items={[
                        {
                          id: "cur",
                          label: "当前供货条件",
                          value: `第 ${item.offering.currentRevision.revisionNo} 版 · ${offeringStatusLabel(item.offering.currentRevision.status)}`,
                        },
                        {
                          id: "price",
                          label: "含税价 / 税率",
                          value: `${item.offering.currentRevision.supplyPriceGross ?? "—"} / ${item.offering.currentRevision.inputTaxRate ?? "—"}`,
                          numeric: true,
                        },
                        {
                          id: "floor",
                          label: "含税底价",
                          value: item.offering.currentRevision.floorPriceGross ?? "—",
                          numeric: true,
                        },
                        {
                          id: "mode",
                          label: "件代发价 / 集采价 / 快递",
                          value: `${item.offering.currentRevision.dropshipSupplyPriceGross ?? "—"} / ${item.offering.currentRevision.bulkSupplyPriceGross ?? "—"} / ${item.offering.currentRevision.dropshipExpress ?? "—"}`,
                        },
                        {
                          id: "moq",
                          label: "最小起订量",
                          value: item.offering.currentRevision.minimumOrderQuantity,
                          numeric: true,
                        },
                      ]}
                    />
                  ) : (
                    <p className="text-sm text-muted-foreground">
                      尚未设置供货条件
                    </p>
                  )}

                  {item.offering?.proposedDefaults ? (
                    <form
                      className="space-y-3 rounded-lg border border-dashed px-3 py-3"
                      onSubmit={(event) => {
                        event.preventDefault()
                        void offeringForm.handleSubmit()
                      }}
                    >
                      <p className="text-xs font-medium">
                        供货条件草稿 · 保存后不会立即生效
                      </p>
                      <div className="grid gap-3 sm:grid-cols-2">
                        <offeringForm.AppField name="supplyPriceGross">
                          {(field) => <field.TextField label="含税供货价" />}
                        </offeringForm.AppField>
                        <offeringForm.AppField name="floorPriceGross">
                          {(field) => <field.TextField label="含税底价（控价参考）" />}
                        </offeringForm.AppField>
                        <offeringForm.AppField name="inputTaxRate">
                          {(field) => (
                            <field.TextField
                              label="进项税率"
                              description="无可靠来源时可留空；提交时会要求补充来源，建议先向供应商确认"
                            />
                          )}
                        </offeringForm.AppField>
                        <offeringForm.AppField name="freightAmount">
                          {(field) => <field.TextField label="运费" />}
                        </offeringForm.AppField>
                        <offeringForm.AppField name="serviceFeeAmount">
                          {(field) => <field.TextField label="服务费" />}
                        </offeringForm.AppField>
                        <offeringForm.AppField name="minimumOrderQuantity">
                          {(field) => <field.TextField label="最小起订量（基础单位）" />}
                        </offeringForm.AppField>
                        <offeringForm.AppField name="dropshipExpress">
                          {(field) => <field.TextField label="一件代发快递" />}
                        </offeringForm.AppField>
                        <offeringForm.AppField name="supplyRegionText">
                          {(field) => <field.TextField label="可供区域（逗号分隔）" />}
                        </offeringForm.AppField>
                        <offeringForm.AppField name="productCapabilitiesText">
                          {(field) => <field.TextField label="商品能力（逗号分隔）" />}
                        </offeringForm.AppField>
                        <offeringForm.AppField name="validFrom">
                          {(field) => <field.TextField label="生效日期" />}
                        </offeringForm.AppField>
                        <offeringForm.AppField name="validTo">
                          {(field) => <field.TextField label="失效日期（可空）" />}
                        </offeringForm.AppField>
                      </div>
                      <offeringForm.AppField name="note">
                        {(field) => <field.TextareaField label="草稿备注" rows={2} />}
                      </offeringForm.AppField>
                      <div className="flex flex-wrap gap-2">
                        <offeringForm.AppForm>
                          <offeringForm.SubmitButton label="保存草稿" />
                        </offeringForm.AppForm>
                        <div className="space-y-1">
                          <Button
                            type="button"
                            size="sm"
                            disabled
                            tabIndex={-1}
                            aria-disabled="true"
                          >
                            确认供货条件
                          </Button>
                          <p className="text-xs text-muted-foreground">
                            供货条件确认功能暂未开放，可先保存草稿。
                          </p>
                        </div>
                      </div>
                    </form>
                  ) : null}
                </CardContent>
              </Card>

              {item.changeType === "STOPPED" ? (
                <Card size="sm" className={surfacePanelClassName}>
                  <CardHeader className="border-b border-border/30 py-3">
                    <CardTitle className="text-base">
                      替代供应商候选
                    </CardTitle>
                    <CardDescription>
                      当前仅可准备候选证据，暂不能选定替代供给
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
                    <div className="space-y-1">
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled
                        tabIndex={-1}
                        aria-disabled="true"
                      >
                        选择替代供应商
                      </Button>
                      <p className="text-xs text-muted-foreground">
                        尚未确定由谁选择替代供应商，当前只能准备候选方案。
                      </p>
                    </div>
                  </CardContent>
                </Card>
              ) : null}

              <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30 py-3">
                  <CardTitle className="text-base">处理动作</CardTitle>
                  <CardDescription>
                    {isRegistered
                      ? "处理当前供应商商品的停供或数据异常"
                      : "当前可查看资料并准备草稿，确认功能暂未开放"}
                  </CardDescription>
                </CardHeader>
                <CardContent className="flex flex-wrap gap-2 pt-4">
                  {isRegistered ? (
                    <>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={!leaseActive || formalPending}
                        onClick={() => setConfirmMode({ kind: "hold" })}
                      >
                        <PauseIcon className="size-3.5" />
                        稍后处理
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
                  {w20Href ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      render={<Link href={w20Href} />}
                    >
                      查看来源 API 连接
                    </Button>
                  ) : null}
                </CardContent>
              </Card>
            </div>
          </div>
        </>
      ) : null}

      {/* 稍后处理 */}
      <Dialog
        open={confirmMode?.kind === "hold"}
        onOpenChange={(open) => {
          if (!open) setConfirmMode(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>稍后处理当前事项</DialogTitle>
            <DialogDescription>
              保存后仍可在待处理列表中继续跟进，不会自动进入下一项
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
                    aria-label="稍后处理原因"
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
                <holdForm.SubmitButton label="确认稍后处理" />
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
              当前供应商数据需要修正，退回后仍可在待处理列表中继续跟进。
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
            ? "相关商城商品已经暂停销售。本次只确认供应商停供信息，不会自动选择替代供应商或恢复销售。"
            : "请确认供应商数据已经修正。本次不会更改 ERP 商品关联或现有供货条件。"
        }
        actionLabel="确认处理结果"
        confirmLabel="确认提交"
        fromStatus={{ label: "待处理", tone: "warning" }}
        toStatus={{ label: "处理完成", tone: "success" }}
        lockedFields={[
          item
            ? `供应商商品：${item.supplierProduct.currentRevision.name}`
            : "供应商商品",
          item
            ? `供应商：${item.supplierProduct.supplier.name}`
            : "供应商",
        ]}
        effects={
          confirmMode?.kind === "confirm_stop"
            ? [
                "记录本次停供确认",
                "完成当前待处理事项",
                "保持相关商城商品暂停销售",
              ]
            : [
                "记录供应商数据已经恢复",
                "完成当前待处理事项",
                "保持现有商品关联和供货条件不变",
              ]
        }
        irreversibleEffects={["提交后将记录在操作历史中"]}
        pending={completeMutation.isPending}
        onConfirm={async () => {
          await completeForm.handleSubmit()
        }}
      />
    </PageScaffold>
  )
}

type SupplierCatalogQueueQueryChangeType =
  | "actionable"
  | "NEW"
  | "CHANGED"
  | "STOPPED"
  | "ERROR"
  | "all"
