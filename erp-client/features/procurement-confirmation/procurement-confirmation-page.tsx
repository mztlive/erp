"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ArrowRightIcon,
  CheckIcon,
  CircleCheckIcon,
  FileSearchIcon,
  PlusIcon,
  SearchIcon,
  TriangleAlertIcon,
} from "lucide-react"
import { z } from "zod"

import {
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  DataFreshness,
  FormalActionResult,
  ListToolbar,
  OptionCombobox,
  PageHeader,
  PageScaffold,
  surfaceInsetClassName,
  surfacePanelClassName,
  ValidationSummary,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { cn } from "@/lib/utils"
import { useSupplierOptionsQuery } from "@/hooks/use-options"
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
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import type {
  ConfirmationLineDraft,
  FormalOutcome,
  FulfillmentMode,
  RejectReasonCode,
} from "@/features/procurement-confirmation/types"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
import { ContractPaperDialog } from "@/features/contracts/contract-paper-dialog"
import { useContractCenterQuery } from "@/features/contracts/queries"
import { ProcurementSalesDocument } from "@/features/procurement-confirmation/procurement-sales-document"
import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
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
  useProcurementRecommendationQuery,
  useProcurementSupplyOptionsQuery,
  useSaveProcurementConfirmationMutation,
} from "@/features/procurement-confirmation/queries"
import { freshnessText } from "@/lib/ui-text"

const rejectSchema = z.object({
  reasonCode: z.string().min(1, "请选择驳回原因"),
  comment: z.string().trim().min(5, "请填写至少 5 个字的补充说明"),
})

const money = new Intl.NumberFormat("zh-CN", {
  style: "currency",
  currency: "CNY",
  minimumFractionDigits: 2,
})

function capabilityCodeForMode(mode: FulfillmentMode) {
  if (mode === "ELECTRONIC") return "virtual"
  if (mode === "SERVICE") return "offline_service"
  return "physical"
}

function supplyCostForQuantity(
  option: ProcurementSupplyOption,
  quantity: string
) {
  return Number(quantity) >= Number(option.bulkMinimumOrderQuantity)
    ? option.bulkCostGross
    : option.dropshipCostGross
}

/** 会话内存中的处理权（不写 URL / localStorage） */
type SessionLease = {
  workItemId: string
  claimedByLabel: string
}

type ResultState = SharedResultState<FormalOutcome>

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
    `queue:procurement-confirmation:${scope}`

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

  const [confirmOpen, setConfirmOpen] = React.useState(false)

  const queueQuery = useProcurementConfirmationQuery(filters)
  const claimMutation = useClaimProcurementMutation()
  const saveMutation = useSaveProcurementConfirmationMutation()
  const completeMutation = useCompleteProcurementMutation()
  const deferMutation = useDeferProcurementMutation()
  const { data: supplierOptions } = useSupplierOptionsQuery()

  const view = queueQuery.data
  const tasks = React.useMemo(() => view?.tasks ?? [], [view?.tasks])
  const context = view?.context
  const task =
    tasks.find((t) => t.workItemId === currentWorkItemId) ??
    view?.current ??
    tasks[0]
  const recommendationQuery = useProcurementRecommendationQuery(
    task?.confirmation.confirmationId ?? "",
    confirmOpen
  )
  const recommendation = recommendationQuery.data
  const contractQuery = useContractCenterQuery(
    task?.salesSubmission.contractId ?? ""
  )
  const taskSkuIds = React.useMemo(
    () => task?.salesSubmission.lines.map((line) => line.itemSku) ?? [],
    [task]
  )
  const supplyOptionsQuery = useProcurementSupplyOptionsQuery(taskSkuIds)
  const supplyOptions = React.useMemo(
    () => supplyOptionsQuery.data ?? [],
    [supplyOptionsQuery.data]
  )
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
  const [loadedPlanKey, setLoadedPlanKey] = React.useState<string | null>(null)
  const [rejectOpen, setRejectOpen] = React.useState(false)
  const [contractOpen, setContractOpen] = React.useState(false)
  const [advanceAfterConfirm, setAdvanceAfterConfirm] = React.useState(true)
  const [lastResult, setLastResult] = React.useState<ResultState>(null)
  /** 自动下一项跳转后保留的上一条结果（轻量条，可关闭） */
  const [finishedResult, setFinishedResult] = React.useState<ResultState>(null)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [saveMessage, setSaveMessage] = React.useState<string | null>(null)
  /** 单号搜索草稿：输入过程不打 URL，防抖/回车才提交（P3） */
  const [orderNoDraft, setOrderNoDraft] = React.useState(orderNo ?? "")
  const orderNoInputRef = React.useRef<HTMLInputElement>(null)
  const headingRef = React.useRef<HTMLHeadingElement>(null)
  const resultRef = React.useRef<HTMLDivElement>(null)
  /** 会话内存处理权，禁止序列化到 URL / storage */
  const [sessionLease, setSessionLease] =
    React.useState<SessionLease | null>(null)

  // 同步当前任务分行草稿
  React.useEffect(() => {
    if (!task) {
      setLineDrafts([])
      setDirty(false)
      setLoadedPlanKey(null)
      return
    }
    setLineDrafts(task.confirmation.lines.map((line) => ({ ...line })))
    setDirty(false)
    setLoadedPlanKey(null)
    setActionError(null)
    setSaveMessage(null)
  }, [task])

  // 只有采购点击“确认通过”后才把最低成本方案装入可编辑草稿；关闭再打开时保留本次调整。
  React.useEffect(() => {
    if (!confirmOpen || !task || !recommendation?.ready || dirty) return
    const planKey = `${task.confirmation.confirmationId}:${recommendation.calculatedAt}`
    if (loadedPlanKey === planKey) return
    setLineDrafts(recommendation.lines.map((line) => ({ ...line })))
    setLoadedPlanKey(planKey)
    setSaveMessage(null)
  }, [confirmOpen, dirty, loadedPlanKey, recommendation, task])

  // 选项异步到达时仅回填展示名称，不覆盖用户已编辑的草稿字段。
  React.useEffect(() => {
    if (!supplierOptions?.length) return
    setLineDrafts((current) =>
      current.map((line) => {
        const supplier = supplierOptions.find(
          (option) => option.supplierId === line.supplierId
        )
        return supplier && supplier.supplierName !== line.supplierName
          ? { ...line, supplierName: supplier.supplierName }
          : line
      })
    )
  }, [supplierOptions])

  // 单号搜索框与 URL 双向同步：后退/分享后输入框与结果保持一致
  React.useEffect(() => {
    setOrderNoDraft(orderNo ?? "")
  }, [orderNo])

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
      task.workItemId !== currentWorkItemId ||
      task.lease
    ) {
      return
    }
    if (sessionLease?.workItemId === task.workItemId) return
    if (claimMutation.isPending) return
    let cancelled = false
    void claimMutation
      .mutateAsync(task.workItemId)
      .then((lease) => {
        if (cancelled) return
        setSessionLease({
          workItemId: lease.workItemId,
          claimedByLabel: lease.claimedByLabel,
        })
      })
      .catch((error) => {
        if (cancelled) return
        setActionError(
          getErrorMessage(error, "领取处理权失败，请点击「领取任务」重试"),
        )
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅在任务切换时领取
  }, [
    currentWorkItemId,
    isProcessingEntry,
    sessionLease?.workItemId,
    task?.lease,
    task?.workItemId,
  ])

  React.useEffect(() => {
    if (lastResult) {
      resultRef.current?.focus()
    } else if (task) {
      headingRef.current?.focus()
    }
  }, [task, lastResult])

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

  // 单号搜索：300ms 防抖写 URL（replace）；筛选变化时清空焦点（P3）
  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (orderNoDraft.trim() === (orderNo ?? "")) return
      replaceUrl({
        orderNo: orderNoDraft.trim() || null,
        currentWorkItemId: null,
      })
    }, 300)
    return () => globalThis.clearTimeout(handle)
  }, [orderNo, orderNoDraft, replaceUrl])

  const commitOrderNo = React.useCallback(() => {
    if (orderNoDraft.trim() === (orderNo ?? "")) return
    replaceUrl({
      orderNo: orderNoDraft.trim() || null,
      currentWorkItemId: null,
    })
  }, [orderNo, orderNoDraft, replaceUrl])

  // scope/sort 不算筛选（P4 保留项）；due/orderNo 才算激活筛选
  const hasActiveFilter = Boolean(
    orderNo || dueParam === "today" || dueParam === "overdue"
  )

  // 清除筛选：清 orderNo/due + 焦点，保留 scope/sort/queueContextId（P4）
  const clearFilters = React.useCallback(() => {
    setOrderNoDraft("")
    replaceUrl({ orderNo: null, due: null, currentWorkItemId: null })
  }, [replaceUrl])

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

  const activeLease = React.useMemo(() => {
    if (sessionLease?.workItemId === task?.workItemId) {
      return sessionLease
    }
    if (task?.lease) {
      return {
        workItemId: task.workItemId,
        claimedByLabel: task.lease.claimedByLabel,
      }
    }
    return null
  }, [sessionLease, task?.lease, task?.workItemId])

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

  const linesValid =
    lineDrafts.length > 0 &&
    lineDrafts.every(
      (line) =>
        line.supplierId.trim().length > 0 &&
        line.offeringRevisionId.trim().length > 0 &&
        line.confirmedQuantity.trim().length > 0 &&
        Number(line.confirmedQuantity) > 0 &&
        line.latestCostGross.trim().length > 0 &&
        Number(line.latestCostGross) >= 0 &&
        line.inputTaxRate.trim().length > 0 &&
        Number(line.inputTaxRate) >= 0 &&
        line.expectedDeliveryDate.trim().length > 0 &&
        line.capabilityRevisionId.trim().length > 0
    )
  const allCovered =
    coverage.length > 0 && coverage.every((c) => c.complete) && linesValid
  const clientBlocking = coverage
    .filter((c) => !c.complete)
    .map((c) => ({
      id: c.submissionLineId,
      label: c.itemName,
      message: `已确认 ${c.confirmed}/${c.required}，缺口 ${c.gap}`,
      targetId: `submission-line-${c.submissionLineId}`,
    }))

  const estimatedPurchase = recommendation?.estimatedPurchaseGross

  const currentPlanSummary = React.useMemo(() => {
    let purchaseGross = 0
    const chargedGroups = new Set<string>()
    const orderGroups = new Set<string>()
    for (const line of lineDrafts) {
      const quantity = Number(line.confirmedQuantity || 0)
      const unitCost = Number(line.latestCostGross || 0)
      if (Number.isFinite(quantity) && Number.isFinite(unitCost)) {
        purchaseGross += quantity * unitCost
      }
      if (!line.offeringRevisionId || !line.supplierId) continue
      const feeGroup = `${line.offeringRevisionId}:${line.fulfillmentMode}`
      orderGroups.add(`${line.supplierId}:${line.fulfillmentMode}`)
      if (chargedGroups.has(feeGroup)) continue
      chargedGroups.add(feeGroup)
      const offering = supplyOptions.find(
        (option) =>
          option.offeringRevisionId === line.offeringRevisionId
      )
      if (!offering) continue
      purchaseGross += Number(offering.serviceFeeAmount || 0)
      if (line.fulfillmentMode === "WAREHOUSE") {
        purchaseGross += Number(offering.freightAmount || 0)
      }
    }
    return {
      purchaseGross,
      grossMargin:
        Number(task?.salesSubmission.grossAmount ?? 0) - purchaseGross,
      orderCount: orderGroups.size,
    }
  }, [lineDrafts, supplyOptions, task?.salesSubmission.grossAmount])

  const updateLine = React.useCallback(
    (lineKey: string, patch: Partial<ConfirmationLineDraft>) => {
      setLineDrafts((prev) =>
        prev.map((l) => (l.lineKey === lineKey ? { ...l, ...patch } : l))
      )
      setDirty(true)
    },
    []
  )

  const repriceDrafts = React.useCallback(
    (drafts: ConfirmationLineDraft[]) => {
      const quantityByOffering = new Map<string, number>()
      for (const line of drafts) {
        if (!line.offeringRevisionId) continue
        quantityByOffering.set(
          line.offeringRevisionId,
          (quantityByOffering.get(line.offeringRevisionId) ?? 0) +
            Number(line.confirmedQuantity || 0)
        )
      }
      return drafts.map((line) => {
        const offering = supplyOptions.find(
          (option) =>
            option.offeringRevisionId === line.offeringRevisionId
        )
        if (!offering) return line
        return {
          ...line,
          latestCostGross: supplyCostForQuantity(
            offering,
            String(quantityByOffering.get(line.offeringRevisionId) ?? 0)
          ),
        }
      })
    },
    [supplyOptions]
  )

  const updatePlanLine = React.useCallback(
    (lineKey: string, patch: Partial<ConfirmationLineDraft>) => {
      setLineDrafts((current) =>
        repriceDrafts(
          current.map((line) =>
            line.lineKey === lineKey ? { ...line, ...patch } : line
          )
        )
      )
      setDirty(true)
    },
    [repriceDrafts]
  )

  const applyRecommendation = React.useCallback(() => {
    if (!recommendation?.ready || recommendation.lines.length === 0) {
      setActionError("当前没有可执行的系统采购方案，请先处理阻断项")
      return
    }
    setLineDrafts(recommendation.lines.map((line) => ({ ...line })))
    setDirty(true)
    setSaveMessage("已重新载入系统最低成本方案，请核对交期后保存")
    setActionError(null)
  }, [recommendation])

  const offeringOptionsForSku = React.useCallback(
    (skuId: string) =>
      supplyOptions
        .filter((option) => option.skuId === skuId)
        .map((option) => {
          const supplier = supplierOptions?.find(
            (row) => row.supplierId === option.supplierId
          )
          return {
            value: option.offeringRevisionId,
            label: `${supplier?.supplierName ?? "供应商"} · 一件代发 ${money.format(Number(option.dropshipCostGross))} / 集采 ${money.format(Number(option.bulkCostGross))}（满 ${option.bulkMinimumOrderQuantity}）`,
          }
        }),
    [supplierOptions, supplyOptions]
  )

  const capabilityOptionsForOffering = React.useCallback(
    (offeringRevisionId: string, fulfillmentMode: FulfillmentMode) =>
      supplyOptions
        .find(
          (option) => option.offeringRevisionId === offeringRevisionId
        )
        ?.capabilities.filter(
          (capability) =>
            capability.capabilityCode ===
            capabilityCodeForMode(fulfillmentMode)
        )
        .map((capability) => ({
          value: capability.revisionId,
          label: capability.label,
        })) ?? [],
    [supplyOptions]
  )

  const fulfillmentOptionsForOffering = React.useCallback(
    (offeringRevisionId: string) => {
      const offering = supplyOptions.find(
        (option) => option.offeringRevisionId === offeringRevisionId
      )
      const modes = (
        Object.keys(FULFILLMENT_MODE_LABEL) as FulfillmentMode[]
      ).filter(
        (mode) =>
          !offering ||
          (supplyCostForQuantity(offering, "1").length > 0 &&
            offering.capabilities.some(
              (capability) =>
                capability.capabilityCode === capabilityCodeForMode(mode)
            ))
      )
      return modes.map((mode) => ({
        value: mode,
        label: FULFILLMENT_MODE_LABEL[mode],
      }))
    },
    [supplyOptions]
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
          supplierId: "",
          supplierName: "",
          offeringRevisionId: "",
          confirmedQuantity: "0",
          latestCostGross: "",
          inputTaxRate: "",
          expectedDeliveryDate: "",
          fulfillmentMode: "WAREHOUSE",
          capabilityRevisionId: "",
          capabilitySummary: "请选择供应商并核对供给与能力",
          qualificationStatus: "INVALID",
        },
      ])
      setDirty(true)
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
    if (task.lease) {
      return {
        workItemId: task.workItemId,
        claimedByLabel: task.lease.claimedByLabel,
      }
    }
    if (sessionLease?.workItemId === task.workItemId) {
      return sessionLease
    }
    const lease = await claimMutation.mutateAsync(task.workItemId)
    const session: SessionLease = {
      workItemId: lease.workItemId,
      claimedByLabel: lease.claimedByLabel,
    }
    setSessionLease(session)
    if (scope === "role_pool") {
      replaceUrl({
        scope: null,
        queueContextId: null,
        currentWorkItemId: task.workItemId,
      })
    }
    return session
  }, [claimMutation, replaceUrl, scope, sessionLease, task])

  const handleSave = React.useCallback(async (): Promise<boolean> => {
    if (!task) return false
    if (!linesValid) {
      setActionError("请先补齐供应商、数量、成本、税率、交期和供应资质后再保存")
      return false
    }
    try {
      await ensureLease()
      const result = await saveMutation.mutateAsync({
        workItemId: task.workItemId,
        confirmationId: task.confirmation.confirmationId,
        submissionId: task.salesSubmission.submissionId,
        expectedEditVersion: task.confirmation.editVersion,
        lines: lineDrafts,
      })
      setDirty(false)
      setSaveMessage(`已保存 · 第 ${result.editVersion} 次修改`)
      setActionError(null)
      return true
    } catch (error) {
      setActionError(getErrorMessage(error, "保存失败"))
      return false
    }
  }, [ensureLease, lineDrafts, linesValid, saveMutation, task])

  /** 终局操作打开前：dirty 时先保存，保存失败则中止打开（防止按旧草稿提交） */
  const guardTerminalOpen = React.useCallback(
    async (): Promise<boolean> => {
      if (!dirty) return true
      const saved = await handleSave()
      if (!saved) {
        setActionError("有未保存的确认分行修改且保存失败，请先处理后再继续")
        return false
      }
      return true
    },
    [dirty, handleSave]
  )

  const advanceIfNeeded = React.useCallback(
    (shouldAdvance: boolean) => {
      if (!shouldAdvance) return
      const nextId = neighborId(1) ?? tasks.find((t) => t.workItemId !== task?.workItemId)
        ?.workItemId
      if (nextId) {
        // 清空本任务租约引用（任务已终局）
        setSessionLease(null)
        goToWorkItem(nextId)
      } else {
        replaceUrl({ currentWorkItemId: null })
      }
    },
    [goToWorkItem, neighborId, replaceUrl, task?.workItemId, tasks]
  )

  const handleApprove = React.useCallback(async () => {
    if (!task) return
    if (!recommendation?.ready || recommendation.lines.length === 0) {
      setActionError("当前采购方案还不能执行，请先处理方案中的问题")
      return
    }
    if (!allCovered) {
      setActionError("请先补齐每项商品的供应商、采购数量、交期和供应资质")
      return
    }
    setActionError(null)
    try {
      await ensureLease()
      await saveMutation.mutateAsync({
        workItemId: task.workItemId,
        confirmationId: task.confirmation.confirmationId,
        submissionId: task.salesSubmission.submissionId,
        expectedEditVersion: task.confirmation.editVersion,
        lines: lineDrafts,
      })
      setDirty(false)
      // 方案落库后编辑版本已变化：生成采购单前重读当前任务。
      const latestTask =
        (await queueQuery.refetch()).data?.tasks.find(
          (t) => t.workItemId === task.workItemId
        ) ?? task
      const response = await completeMutation.mutateAsync({
        workItemId: latestTask.workItemId,
        expectedSubjectVersion: latestTask.subjectVersion,
        decision: {
          reviewResult: "APPROVED",
          confirmationId: latestTask.confirmation.confirmationId,
          submissionId: latestTask.salesSubmission.submissionId,
          expectedConfirmationEditVersion:
            latestTask.confirmation.editVersion,
          salesOrderId: latestTask.salesSubmission.salesOrderId,
          salesOrderNo: latestTask.salesSubmission.salesOrderNo,
          subjectHash: latestTask.salesSubmission.subjectHash,
        },
      })

      if (response.status === "failed") {
        setActionError(response.message)
        return
      }

      const outcome = response.outcome
      if (outcome.kind !== "APPROVED_AND_SALES_EFFECTIVE") return
      setConfirmOpen(false)
      setSessionLease(null)
      const approvedResult: ResultState = {
        status: "succeeded",
        title: "采购确认已通过 · 采购单已生成",
        description: advanceAfterConfirm && autoNext
          ? `销售单已生效，已生成 ${outcome.purchaseOrders.length} 张采购单；队列将打开下一条。`
          : `销售单已生效，已生成 ${outcome.purchaseOrders.length} 张采购单，可直接核对后提交。`,
        reference: outcome.reference,
        outcome,
        stayOnItem: !(advanceAfterConfirm && autoNext),
      }
      setLastResult(approvedResult)
      if (advanceAfterConfirm && autoNext) {
        setFinishedResult(approvedResult)
        advanceIfNeeded(true)
      } else {
        setFinishedResult(null)
      }
    } catch (error) {
      setActionError(getErrorMessage(error, "通过失败"))
    }
  }, [
    advanceAfterConfirm,
    advanceIfNeeded,
    allCovered,
    autoNext,
    completeMutation,
    ensureLease,
    lineDrafts,
    queueQuery,
    recommendation,
    saveMutation,
    task,
  ])

  const handleRejectSubmit = React.useCallback(
    async (value: { reasonCode: RejectReasonCode; comment: string }) => {
      if (!task) return
      setActionError(null)
      try {
        await ensureLease()
        // guard 自动保存后编辑版本可能已 +1：终局提交前重读任务
        const latestTask =
          (await queueQuery.refetch()).data?.tasks.find(
            (t) => t.workItemId === task.workItemId
          ) ?? task
        const response = await completeMutation.mutateAsync({
          workItemId: latestTask.workItemId,
          expectedSubjectVersion: latestTask.subjectVersion,
          decision: {
            reviewResult: "REJECTED",
            confirmationId: latestTask.confirmation.confirmationId,
            submissionId: latestTask.salesSubmission.submissionId,
            expectedConfirmationEditVersion:
              latestTask.confirmation.editVersion,
            salesOrderId: latestTask.salesSubmission.salesOrderId,
            salesOrderNo: latestTask.salesSubmission.salesOrderNo,
            subjectHash: latestTask.salesSubmission.subjectHash,
            rejectReasonCode: value.reasonCode,
            comment: value.comment,
          },
        })
        setRejectOpen(false)
        if (response.status === "failed") {
          setActionError(response.message)
          return
        }
        const outcome = response.outcome
        if (outcome.kind !== "REJECTED_TO_SALES") return
        setSessionLease(null)
        const rejectedResult: ResultState = {
          status: "rejected",
          title: "采购确认已驳回 · 本次提交已结束",
          description:
            "已形成本次采购确认的驳回结论；未创建采购单、变更单或后继任务。销售可在销售单选择三条固定出路。",
          reference: outcome.reference,
          outcome,
          stayOnItem: !autoNext,
        }
        setLastResult(rejectedResult)
        if (autoNext) {
          setFinishedResult(rejectedResult)
          advanceIfNeeded(true)
        } else {
          setFinishedResult(null)
        }
      } catch (error) {
        setActionError(getErrorMessage(error, "驳回失败"))
      }
    },
    [advanceIfNeeded, autoNext, completeMutation, ensureLease, queueQuery, task]
  )

  const rejectForm = useAppForm({
    defaultValues: {
      reasonCode: "",
      comment: "",
    },
    validators: { onChange: rejectSchema, onMount: rejectSchema },
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
        const saved = await handleSave()
        if (!saved) {
          setActionError("有未保存的确认分行修改且保存失败；请重试保存后再跳过")
          return
        }
      }
      await ensureLease()
      const nextId = neighborId(1)
      const response = await deferMutation.mutateAsync({
        workItemId: task.workItemId,
        queueContextId,
        nextWorkItemId: nextId,
      })
      if (response.status !== "succeeded" || response.outcome.kind !== "DEFERRED") {
        if (response.status === "failed") setActionError(response.message)
        return
      }
      setSessionLease(null)
      setLastResult({
        status: "blocked",
        title: "当前项已跳过",
        description:
          "任务仍保留在待处理队列，未形成通过或驳回结论。本次已结束并打开下一条。",
        reference: response.outcome.reference,
        outcome: response.outcome,
      })
      if (nextId) goToWorkItem(nextId)
    } catch (error) {
      setActionError(getErrorMessage(error, "跳过失败"))
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
        if (activeLease) {
          setAdvanceAfterConfirm(autoNext)
          setConfirmOpen(true)
        }
        return
      }
      if (inField) return
      if (event.key === "/") {
        event.preventDefault()
        orderNoInputRef.current?.focus()
        return
      }
      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault()
        if (dirty) {
          setActionError("有未保存修改，请先保存后再切换")
          return
        }
        const next = neighborId(1)
        if (next) goToWorkItem(next)
      }
      if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault()
        if (dirty) {
          setActionError("有未保存修改，请先保存后再切换")
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
    autoNext,
    dirty,
    goToWorkItem,
    handleSave,
    neighborId,
    orderNoInputRef,
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
    saveMutation.isPending ||
    deferMutation.isPending ||
    claimMutation.isPending

  const returnTo = buildReturnHref(
    new URLSearchParams(searchParams.toString())
  )

  if (queueQuery.isPending) {
    return (
      <PageScaffold>
        <PageHeader title="采购二次确认" description="正在加载队列…" />
        <div
          className="h-12 animate-pulse rounded-lg bg-muted"
          aria-hidden="true"
        />
        <div className="grid gap-3 md:gap-4 xl:grid-cols-[minmax(0,2fr)_minmax(16rem,1fr)]">
          <div className="h-80 animate-pulse rounded-lg bg-muted" />
          <div className="h-64 animate-pulse rounded-lg bg-muted" />
        </div>
      </PageScaffold>
    )
  }

  if (queueQuery.isError) {
    return (
      <PageScaffold>
        <PageHeader title="采购二次确认" description="队列加载失败" />
        <BusinessFailureState
          title="队列加载失败"
          error={queueQuery.error}
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

  return (
    <PageScaffold>
      <PageHeader
        title="采购二次确认"
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
              updatedAt={
                context?.queueContextUpdatedAt
                  ? formatDateTime(context.queueContextUpdatedAt, "default")
                  : "刚刚"
              }
              dateTime={context?.queueContextUpdatedAt}
              state="fresh"
              label={freshnessText.queueUpdatedAt}
            />
            <span className="text-xs text-muted-foreground" aria-live="polite">
              {context?.filterSummary ?? "仅我的"} · 待处理{" "}
              {context?.total ?? 0}
            </span>
          </div>
        }
      />

      <div
        className={cn(
          surfacePanelClassName,
          "sticky top-0 z-10 space-y-2.5 px-3 py-2.5 text-sm"
        )}
      >
        <div className="flex flex-wrap items-center gap-3">
          <div
            role="group"
            aria-label="责任范围"
            className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
          >
            {(
              [
                { value: "mine" as const, label: "我的待办" },
                { value: "role_pool" as const, label: "团队待认领" },
              ] as const
            ).map((opt) => (
              <button
                key={opt.value}
                type="button"
                aria-pressed={scope === opt.value}
                onClick={() =>
                  replaceUrl({
                    scope: opt.value === "mine" ? null : opt.value,
                    queueContextId: null,
                    currentWorkItemId: null,
                  })
                }
                className={cn(
                  "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  scope === opt.value
                    ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                    : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
                )}
              >
                {opt.label}
              </button>
            ))}
          </div>
          <div
            role="group"
            aria-label="到期时限"
            className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
          >
            {(
              [
                { value: "all" as const, label: "全部时限" },
                { value: "today" as const, label: "今日到期" },
                { value: "overdue" as const, label: "已超期" },
              ] as const
            ).map((opt) => (
              <button
                key={opt.value}
                type="button"
                aria-pressed={
                  opt.value === "all" ? due === "active" : due === opt.value
                }
                onClick={() =>
                  replaceUrl({
                    due: opt.value === "all" ? null : opt.value,
                    currentWorkItemId: null,
                  })
                }
                className={cn(
                  "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  (opt.value === "all" ? due === "active" : due === opt.value)
                    ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                    : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
                )}
              >
                {opt.label}
              </button>
            ))}
          </div>
          <span className="ml-auto hidden text-xs text-muted-foreground md:inline">
            快捷键：j / k 切换 · ⌘S 保存 · ⌘↵ 通过确认 · / 按单号搜索
          </span>
        </div>
        <ListToolbar
          aria-label="二次确认筛选"
          search={
            <form
              onSubmit={(event) => {
                event.preventDefault()
                commitOrderNo()
              }}
            >
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon className="size-4" aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  ref={orderNoInputRef}
                  value={orderNoDraft}
                  onChange={(event) => setOrderNoDraft(event.target.value)}
                  placeholder="按销售单号搜索"
                  aria-label="按单号搜索队列"
                />
              </InputGroup>
            </form>
          }
          actions={
            <>
              {hasActiveFilter ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={clearFilters}
                >
                  清除筛选
                </Button>
              ) : null}
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
                  该偏好仅在本次操作内生效
                </span>
              </div>
              <Badge variant="outline" className="font-normal">
                仅本次会话
              </Badge>
            </>
          }
        />
      </div>

      {finishedResult && !lastResult ? (
        <div
          role="status"
          className={`${surfaceInsetClassName} flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2 text-sm`}
        >
          <CircleCheckIcon
            className="size-4 shrink-0 text-success-soft-foreground"
            aria-hidden="true"
          />
          <span className="font-medium">
            上一项已{finishedResult.status === "rejected" ? "驳回" : "通过"}
          </span>
          {finishedResult.reference ? (
            <span className="num text-muted-foreground">
              {finishedResult.reference}
            </span>
          ) : null}
          {finishedResult.status === "rejected" ? (
            <span className="text-muted-foreground">
              销售可在销售单选择三条固定出路
            </span>
          ) : (
            <span className="text-muted-foreground">
              已自动生成采购单草稿，可直接核对并提交
            </span>
          )}
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={
                <Link
                  href={w05Href(
                    finishedResult.outcome &&
                      "salesOrderId" in finishedResult.outcome
                      ? finishedResult.outcome.salesOrderId
                      : task?.salesSubmission.salesOrderId ?? "#",
                    returnTo
                  )}
                />
              }
            >
              打开销售单
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => setFinishedResult(null)}
            >
              关闭
            </Button>
          </div>
        </div>
      ) : null}

      {lastResult ? (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
          <FormalActionResult
            status={lastResult.status === "failed" ? "blocked" : lastResult.status}
            title={lastResult.title}
            description={lastResult.description}
            reference={lastResult.reference}
            facts={buildResultFacts(
              lastResult.outcome,
              context,
              task?.salesSubmission.submissionNo
            )}
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
              />
            }
          />
          {lastResult.outcome?.kind === "REJECTED_TO_SALES" ? (
            <ProcurementRejectionNextSteps
              salesOrderId={lastResult.outcome.salesOrderId}
              returnTo={returnTo}
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
        view?.emptyReason === "FILTER_NO_RESULT" ? (
          <BusinessEmptyState
            kind="filter"
            title="当前筛选无结果"
            description="没有单号或范围匹配的待确认事项，可清除筛选后重试。"
            action={
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  onClick={clearFilters}
                >
                  清除筛选
                </Button>
                <Button render={<Link href="/workspace" />}>
                  返回今日工作台
                </Button>
              </div>
            }
          />
        ) : view?.emptyReason === "NO_DATA_SCOPE" ? (
          <BusinessEmptyState
            kind="no-scope"
            title="当前角色无数据范围"
            description="你可以进入此页面，但当前角色范围内没有可查看的待确认事项。"
            action={<Button render={<Link href="/workspace" />}>返回今日工作台</Button>}
          />
        ) : (
          <BusinessEmptyState
            kind="no-tasks"
            title="本筛选项已处理完"
            description="当前采购二次确认队列已经清空，可以返回工作台处理其它事项。"
            action={
              <Button render={<Link href="/workspace" />}>返回今日工作台</Button>
            }
          />
        )
      ) : task ? (
        <>
          {/* 1440 双栏：左销售提交+分行，右决策摘要 sticky */}
          <div className="grid min-w-0 gap-3 md:gap-4 xl:grid-cols-[minmax(0,66fr)_minmax(17rem,34fr)]">
            <div className={`min-w-0 space-y-4 p-3 md:p-4 ${surfacePanelClassName}`}>
              {task.salesSubmission.resubmissionContext ? (
                <Alert variant="info">
                  <TriangleAlertIcon aria-hidden="true" />
                  <AlertTitle>改品/改价后新提交 · 须重新确认</AlertTitle>
                  <AlertDescription>
                    第 {task.salesSubmission.submissionNo} 次提交 ·{" "}
                    {task.salesSubmission.submittedAt} ·{" "}
                    {task.salesSubmission.submittedByLabel}
                    ；上一驳回提交已作废。不得复用旧确认分行。
                  </AlertDescription>
                </Alert>
              ) : null}

              <ProcurementSalesDocument
                task={task}
                onOpenContract={
                  task.salesSubmission.contractId
                    ? () => setContractOpen(true)
                    : undefined
                }
              />

              <Card className="hidden min-w-0" size="sm">
                <CardHeader className="border-b">
                  <div className="flex flex-wrap items-center gap-2">
                    <CardTitle>
                      <h2
                        tabIndex={-1}
                        className="outline-none"
                        aria-live="polite"
                      >
                        最低成本采购方案
                      </h2>
                    </CardTitle>
                    <BusinessStatusBadge
                      context="list"
                      label={recommendation?.ready ? "可执行" : "待补齐"}
                      tone={recommendation?.ready ? "success" : "warning"}
                    />
                    <Badge variant="secondary">
                      系统推荐 · 可人工调整
                    </Badge>
                  </div>
                  <CardDescription>
                    按供应商当前有效供给、能力、起订量、可供量、采购价及费用组合；审批时系统重新校验。
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-5">
                  <Alert
                    variant={
                      recommendationQuery.isError
                        ? "destructive"
                        : recommendation?.ready
                          ? "success"
                          : "warning"
                    }
                  >
                    {recommendation?.ready ? (
                      <CircleCheckIcon aria-hidden="true" />
                    ) : (
                      <TriangleAlertIcon aria-hidden="true" />
                    )}
                    <AlertTitle>
                      {recommendationQuery.isPending
                        ? "正在计算最低成本方案"
                        : recommendationQuery.isError
                          ? "最低成本方案计算失败"
                          : recommendation?.ready
                            ? `已组合 ${recommendation.purchaseOrders.length} 张采购单草稿`
                            : "当前无法形成完整采购方案"}
                    </AlertTitle>
                    <AlertDescription>
                      {recommendation?.ready
                        ? `预计采购含税 ${money.format(Number(recommendation.estimatedPurchaseGross))}，预计毛利 ${money.format(Number(recommendation.estimatedGrossMargin))}。交期仍需采购核对。`
                        : recommendation?.blockingIssues
                            .map((issue) => issue.message)
                            .join("；") ||
                          "请等待系统完成计算，或刷新后重试。"}
                    </AlertDescription>
                  </Alert>

                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <p className="text-xs text-muted-foreground">
                      推荐策略：{recommendation?.policyVersion ?? "正在读取"}
                    </p>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={
                        formalPending ||
                        recommendationQuery.isPending ||
                        !recommendation?.ready
                      }
                      onClick={applyRecommendation}
                    >
                      重新载入最低成本方案
                    </Button>
                  </div>

                  <Separator />

                  <section aria-labelledby="confirm-lines-title">
                    <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                      <h3
                        id="confirm-lines-title"
                        className="text-sm font-semibold"
                      >
                        采购明细
                      </h3>
                      <span className="text-xs text-muted-foreground">
                      同一商品可向多个供应商采购；每条销售明细分别核对数量
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
                                  {subLine.itemName}
                                </p>
                                <p className="text-xs text-muted-foreground">
                                  承诺{" "}
                                  <span className="num">
                                    {subLine.committedQuantity} {subLine.unit}
                                  </span>{" "}
                                  · 客户期望 {subLine.requestedDeliveryDate}
                                  {subLine.referenceSupplier
                                    ? ` · 参考 ${subLine.referenceSupplier} / ${money.format(Number(subLine.referenceCost))}`
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
                                  覆盖 {cov?.confirmed}/{cov?.required}{" "}
                                  {subLine.unit}
                                  {cov && !cov.complete
                                    ? ` · 缺口 ${cov.gap} ${subLine.unit}`
                                    : " · 完整"}
                                </Badge>
                              </div>
                            </div>

                            <div className="overflow-x-auto">
                              <table className="w-full min-w-[40rem] text-sm">
                                <caption className="sr-only">
                                  {subLine.itemName} 采购明细
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
                                    <th className="hidden px-3 py-2 font-medium num md:table-cell">
                                      进项税率
                                    </th>
                                    <th className="hidden px-3 py-2 font-medium sm:table-cell">
                                      预计交期
                                    </th>
                                    <th className="px-3 py-2 font-medium">
                                      交付方式
                                    </th>
                                    <th className="hidden px-3 py-2 font-medium lg:table-cell">
                                      供应资质
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
                                        <OptionCombobox
                                          value={
                                            line.offeringRevisionId || undefined
                                          }
                                          onValueChange={(revisionId) => {
                                            const offering = supplyOptions.find(
                                              (option) =>
                                                option.offeringRevisionId ===
                                                revisionId
                                            )
                                            const supplier =
                                              supplierOptions?.find(
                                                (option) =>
                                                  option.supplierId ===
                                                  offering?.supplierId
                                              )
                                            const onlyCapability =
                                              offering?.capabilities.filter(
                                                (capability) =>
                                                  capability.capabilityCode ===
                                                  capabilityCodeForMode(
                                                    line.fulfillmentMode
                                                  )
                                              ).length === 1
                                                ? offering.capabilities.find(
                                                    (capability) =>
                                                      capability.capabilityCode ===
                                                      capabilityCodeForMode(
                                                        line.fulfillmentMode
                                                      )
                                                  )
                                                : undefined
                                            updateLine(line.lineKey, {
                                              supplierId:
                                                offering?.supplierId ?? "",
                                              supplierName:
                                                supplier?.supplierName ?? "",
                                              offeringRevisionId:
                                                offering?.offeringRevisionId ?? "",
                                              latestCostGross:
                                                offering
                                                  ? supplyCostForQuantity(
                                                      offering,
                                                      line.confirmedQuantity
                                                    )
                                                  : "",
                                              inputTaxRate:
                                                offering?.inputTaxRate ?? "",
                                              capabilityRevisionId:
                                                onlyCapability?.revisionId ?? "",
                                              capabilitySummary:
                                                onlyCapability?.label ??
                                                "请选择有效供应资质",
                                              qualificationStatus:
                                                onlyCapability
                                                  ? "VALID"
                                                  : "INVALID",
                                            })
                                          }}
                                          options={offeringOptionsForSku(
                                            subLine.itemSku
                                          )}
                                          disabled={formalPending}
                                          placeholder="选择供应商报价"
                                          className="min-w-[12rem]"
                                        />
                                      </td>
                                      <td className="px-3 py-2">
                                        <Input
                                          className="num w-20"
                                          inputMode="decimal"
                                          value={line.confirmedQuantity}
                                          onChange={(e) => {
                                            const confirmedQuantity =
                                              e.target.value
                                            const offering = supplyOptions.find(
                                              (option) =>
                                                option.offeringRevisionId ===
                                                line.offeringRevisionId
                                            )
                                            updateLine(line.lineKey, {
                                              confirmedQuantity,
                                              latestCostGross: offering
                                                ? supplyCostForQuantity(
                                                    offering,
                                                    confirmedQuantity
                                                  )
                                                : line.latestCostGross,
                                            })
                                          }}
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
                                      <td className="hidden px-3 py-2 md:table-cell">
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
                                      <td className="hidden px-3 py-2 sm:table-cell">
                                        <DatePicker
                                          className="w-[9.5rem]"
                                          value={
                                            line.expectedDeliveryDate ||
                                            undefined
                                          }
                                          onValueChange={(next) =>
                                            updateLine(line.lineKey, {
                                              expectedDeliveryDate: next ?? "",
                                            })
                                          }
                                          disabled={formalPending}
                                        />
                                      </td>
                                      <td className="px-3 py-2">
                                        <OptionCombobox
                                          value={line.fulfillmentMode}
                                          onValueChange={(value) => {
                                            if (!value) return
                                            const fulfillmentMode =
                                              value as FulfillmentMode
                                            const offering = supplyOptions.find(
                                              (option) =>
                                                option.offeringRevisionId ===
                                                line.offeringRevisionId
                                            )
                                            const capabilities =
                                              offering?.capabilities.filter(
                                                (capability) =>
                                                  capability.capabilityCode ===
                                                  capabilityCodeForMode(
                                                    fulfillmentMode
                                                  )
                                              ) ?? []
                                            const onlyCapability =
                                              capabilities.length === 1
                                                ? capabilities[0]
                                                : undefined
                                            updateLine(line.lineKey, {
                                              fulfillmentMode,
                                              capabilityRevisionId:
                                                onlyCapability?.revisionId ?? "",
                                              capabilitySummary:
                                                onlyCapability?.label ??
                                                "请选择有效供应资质",
                                              qualificationStatus:
                                                onlyCapability
                                                  ? "VALID"
                                                  : "INVALID",
                                            })
                                          }}
                                          options={fulfillmentOptionsForOffering(
                                            line.offeringRevisionId
                                          )}
                                          size="sm"
                                          allowClear={false}
                                          disabled={formalPending}
                                          aria-label="交付方式"
                                          placeholder="交付方式"
                                          className="min-w-[8rem]"
                                        />
                                      </td>
                                      <td className="hidden px-3 py-2 lg:table-cell">
                                        <OptionCombobox
                                          value={
                                            line.capabilityRevisionId || undefined
                                          }
                                          onValueChange={(revisionId) => {
                                            const capability =
                                              capabilityOptionsForOffering(
                                                line.offeringRevisionId,
                                                line.fulfillmentMode
                                              ).find(
                                                (option) =>
                                                  option.value === revisionId
                                              )
                                            updateLine(line.lineKey, {
                                              capabilityRevisionId:
                                                revisionId ?? "",
                                              capabilitySummary:
                                                capability?.label ?? "",
                                              qualificationStatus: revisionId
                                                ? "VALID"
                                                : "INVALID",
                                            })
                                          }}
                                          options={capabilityOptionsForOffering(
                                            line.offeringRevisionId,
                                            line.fulfillmentMode
                                          )}
                                          size="sm"
                                          disabled={formalPending}
                                          placeholder="选择供应资质"
                                          className="min-w-[8rem]"
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
                    <p className="text-xs text-warning-soft-foreground" role="status">
                      有未保存的确认分行修改（⌘S 保存）
                    </p>
                  ) : null}
                </CardContent>
              </Card>
            </div>

            {/* 决策摘要：桌面 sticky；top 避开 sticky 处理条 */}
            <aside className="space-y-3 md:space-y-4 xl:sticky xl:top-16 xl:self-start">
              <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="rounded-t-lg border-b border-border/30">
                  <CardTitle>
                    <h2
                      ref={headingRef}
                      tabIndex={-1}
                      className="outline-none"
                    >
                      本次确认
                    </h2>
                  </CardTitle>
                  <CardDescription>
                    系统将在你点击确认通过后计算采购方案。
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3">
                  <dl className="space-y-2 text-sm">
                    <div className="flex justify-between gap-2">
                      <dt className="text-muted-foreground">销售单</dt>
                      <dd className="font-medium">
                        {task.salesSubmission.salesOrderNo}
                      </dd>
                    </div>
                    <div className="flex justify-between gap-2">
                      <dt className="text-muted-foreground">商品明细</dt>
                      <dd>{task.salesSubmission.lines.length} 项</dd>
                    </div>
                    <div className="flex justify-between gap-2">
                      <dt className="text-muted-foreground">销售含税金额</dt>
                      <dd className="num">
                        {money.format(Number(task.salesSubmission.grossAmount))}
                      </dd>
                    </div>
                  </dl>
                  <Alert variant="info">
                    <CircleCheckIcon aria-hidden="true" />
                    <AlertTitle>确认通过不会立即生成采购单</AlertTitle>
                    <AlertDescription>
                      系统会先展示成本最低的采购组合，只有再次确认该方案后才生成采购单。
                    </AlertDescription>
                  </Alert>
                  <div
                    className="grid grid-cols-3 gap-2"
                    role="group"
                    aria-label="本次确认操作"
                  >
                    <Button
                      type="button"
                      variant="destructive"
                      disabled={formalPending}
                      onClick={() => {
                        void (async () => {
                          if (!(await guardTerminalOpen())) return
                          setRejectOpen(true)
                        })()
                      }}
                    >
                      驳回
                    </Button>
                    <Button
                      type="button"
                      disabled={formalPending}
                      onClick={() => {
                        void ensureLease()
                          .then(() => {
                            setAdvanceAfterConfirm(autoNext)
                            setConfirmOpen(true)
                          })
                          .catch((error) => {
                            setActionError(getErrorMessage(error, "领取任务失败"))
                          })
                      }}
                    >
                      确认通过
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      disabled={formalPending}
                      onClick={() => void handleDefer()}
                    >
                      跳过
                    </Button>
                  </div>
                </CardContent>
              </Card>

              <Card size="sm" className={cn(surfacePanelClassName, "hidden")}>
                <CardHeader className="rounded-t-lg border-b border-border/30">
                  <CardTitle>决策摘要</CardTitle>
                  <CardDescription>
                    数量覆盖按明细独立展示，不可跨行抵消。
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4">
                  <ul className="space-y-2" aria-label="逐明细数量覆盖">
                    {coverage.map((c) => {
                      const unit = task.salesSubmission.lines.find(
                        (l) => l.submissionLineId === c.submissionLineId
                      )?.unit
                      return (
                        <li
                          key={c.submissionLineId}
                          className="flex items-start justify-between gap-2 text-sm"
                        >
                          <span className="min-w-0 truncate">
                            {c.itemName}
                          </span>
                          <span
                            className={
                              c.complete
                              ? "num shrink-0 text-success-soft-foreground"
                              : "num shrink-0 text-destructive"
                          }
                        >
                          {c.confirmed}/{c.required} {unit ?? ""}
                          {!c.complete ? ` 缺${c.gap} ${unit ?? ""}` : ""}
                        </span>
                        </li>
                      )
                    })}
                  </ul>
                  <Separator />
                  <dl className="space-y-2 text-sm">
                    <div className="flex justify-between gap-2">
                      <dt className="text-muted-foreground">推荐采购含税</dt>
                      <dd className="num font-medium">
                        {estimatedPurchase
                          ? money.format(Number(estimatedPurchase))
                          : "—"}
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
                  {recommendation?.purchaseOrders.length ? (
                    <>
                      <Separator />
                      <div className="space-y-2">
                        <p className="text-xs font-medium text-muted-foreground">
                          审批后自动生成
                        </p>
                        <ul className="space-y-2">
                          {recommendation.purchaseOrders.map((order) => (
                            <li
                              key={`${order.supplierId}-${order.fulfillmentMode}`}
                              className="rounded-md border border-border bg-muted/30 p-2 text-xs"
                            >
                              <div className="flex justify-between gap-2">
                                <span className="font-medium">
                                  {order.supplierName}
                                </span>
                                <span className="num">
                                  {money.format(Number(order.estimatedGross))}
                                </span>
                              </div>
                              <p className="mt-1 text-muted-foreground">
                                {FULFILLMENT_MODE_LABEL[order.fulfillmentMode]} ·{" "}
                                {order.lineCount} 条明细 · 采购单草稿
                              </p>
                            </li>
                          ))}
                        </ul>
                      </div>
                    </>
                  ) : null}
                  {clientBlocking.length > 0 ? (
                    <ValidationSummary
                      title="通过前须补齐"
                      issues={clientBlocking}
                    />
                  ) : (
                    <p className="flex items-center gap-2 text-sm text-success-soft-foreground">
                      <CircleCheckIcon
                        className="size-4"
                        aria-hidden="true"
                      />
                      当前编辑态覆盖完整（最终以系统重验为准）
                    </p>
                  )}
                  {(recommendation?.warnings ?? task.decisionSummary.warnings).map((w, index) => (
                    <p
                      key={`${w.code}-${w.lineId ?? "none"}-${index}`}
                      className="text-xs text-muted-foreground"
                    >
                      警告：{w.message}
                    </p>
                  ))}
                </CardContent>
              </Card>

              <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="rounded-t-lg border-b border-border/30">
                  <CardTitle>销售单详情</CardTitle>
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
                    打开销售单 · {task.salesSubmission.salesOrderNo}
                  </Button>
                </CardContent>
              </Card>
            </aside>
          </div>

          {contractQuery.data ? (
            <ContractPaperDialog
              contract={contractQuery.data}
              open={contractOpen}
              onOpenChange={setContractOpen}
            />
          ) : (
            <Dialog open={contractOpen} onOpenChange={setContractOpen}>
              <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                  <DialogTitle>
                    {contractQuery.isPending
                      ? "正在读取合同"
                      : "合同信息暂不可读取"}
                  </DialogTitle>
                  <DialogDescription>
                    {contractQuery.isPending
                      ? "合同仍在加载，完成后会在当前页面显示。"
                      : "当前账号未取得合同详情，或合同记录已不存在。采购确认仍以销售提交中的合同快照为准。"}
                  </DialogDescription>
                </DialogHeader>
                <dl className="grid gap-3 rounded-lg border border-border bg-muted/30 p-4 text-sm">
                  <div className="flex justify-between gap-4">
                    <dt className="text-muted-foreground">合同编号</dt>
                    <dd className="font-medium">
                      {task.salesSubmission.contractSnapshot ?? "—"}
                    </dd>
                  </div>
                  <div className="flex justify-between gap-4">
                    <dt className="text-muted-foreground">客户</dt>
                    <dd>{task.salesSubmission.customerSnapshot}</dd>
                  </div>
                  <div className="flex justify-between gap-4">
                    <dt className="text-muted-foreground">付款条件</dt>
                    <dd>{task.salesSubmission.paymentTermLabel}</dd>
                  </div>
                </dl>
                <DialogFooter>
                  <DialogClose render={<Button type="button" />}>关闭</DialogClose>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          )}

          <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
            <DialogContent className="max-h-[88vh] w-[calc(100vw-2rem)] overflow-x-hidden overflow-y-auto sm:max-w-6xl">
              <DialogHeader>
                <DialogTitle>确认采购方案</DialogTitle>
                <DialogDescription>
                  销售单 {task.salesSubmission.salesOrderNo}。系统按采购数量匹配报价档位，并在可供范围内组合预计成本最低的采购方案。
                </DialogDescription>
              </DialogHeader>

              {recommendationQuery.isPending ? (
                <div className="rounded-lg border border-border bg-muted/30 p-6 text-center text-sm text-muted-foreground">
                  正在核对供应商报价、起订量和可供数量…
                </div>
              ) : recommendationQuery.isError ? (
                <BusinessFailureState
                  title="采购方案计算失败"
                  error={recommendationQuery.error}
                  onRetry={() => void recommendationQuery.refetch()}
                />
              ) : recommendation?.ready ? (
                <div className="space-y-4">
                  <Alert variant="success">
                    <CircleCheckIcon aria-hidden="true" />
                    <AlertTitle>
                      当前方案将生成 {currentPlanSummary.orderCount} 张采购单
                    </AlertTitle>
                    <AlertDescription>
                      当前采购含税 {money.format(currentPlanSummary.purchaseGross)}，预计毛利 {money.format(currentPlanSummary.grossMargin)}。
                    </AlertDescription>
                  </Alert>

                  <div className="grid gap-3 sm:grid-cols-3">
                    <div className="rounded-lg border border-border p-3">
                      <p className="text-xs text-muted-foreground">销售含税金额</p>
                      <p className="num mt-1 font-semibold">
                        {money.format(Number(recommendation.salesGross))}
                      </p>
                    </div>
                    <div className="rounded-lg border border-border p-3">
                      <p className="text-xs text-muted-foreground">预计采购金额</p>
                      <p className="num mt-1 font-semibold">
                        {money.format(currentPlanSummary.purchaseGross)}
                      </p>
                    </div>
                    <div className="rounded-lg border border-border p-3">
                      <p className="text-xs text-muted-foreground">预计毛利</p>
                      <p className="num mt-1 font-semibold">
                        {money.format(currentPlanSummary.grossMargin)}
                      </p>
                    </div>
                  </div>

                  <section className="space-y-3" aria-labelledby="editable-plan-title">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div>
                        <h3 id="editable-plan-title" className="font-semibold">
                          当前采购安排
                        </h3>
                        <p className="mt-1 text-xs text-muted-foreground">
                          可以调整供应商、数量、交付方式和交期；采购价会按该供应商承接的合计数量自动重算。
                        </p>
                      </div>
                      <Badge variant={dirty ? "secondary" : "outline"}>
                        {dirty ? "已人工调整" : "系统初始方案"}
                      </Badge>
                    </div>

                    {task.salesSubmission.lines.map((subLine) => {
                      const planLines = lineDrafts.filter(
                        (line) =>
                          line.submissionLineId === subLine.submissionLineId
                      )
                      const lineCoverage = coverage.find(
                        (item) =>
                          item.submissionLineId === subLine.submissionLineId
                      )
                      return (
                        <div
                          key={subLine.submissionLineId}
                          className="overflow-hidden rounded-lg border border-border"
                        >
                          <div className="flex flex-wrap items-start justify-between gap-2 bg-muted/40 px-4 py-3">
                            <div>
                              <p className="font-medium">{subLine.itemName}</p>
                              <p className="mt-1 text-xs text-muted-foreground">
                                需要采购 {subLine.committedQuantity} {subLine.unit} · 客户交期 {subLine.requestedDeliveryDate}
                              </p>
                            </div>
                            <Badge
                              variant={
                                lineCoverage?.complete
                                  ? "secondary"
                                  : "destructive"
                              }
                            >
                              已安排 {lineCoverage?.confirmed ?? "0"}/{lineCoverage?.required ?? subLine.committedQuantity} {subLine.unit}
                            </Badge>
                          </div>

                          <div className="space-y-3 p-3">
                            {planLines.map((line) => {
                              const offering = supplyOptions.find(
                                (option) =>
                                  option.offeringRevisionId ===
                                  line.offeringRevisionId
                              )
                              const allocatedQuantity = lineDrafts
                                .filter(
                                  (item) =>
                                    item.offeringRevisionId ===
                                    line.offeringRevisionId
                                )
                                .reduce(
                                  (total, item) =>
                                    total + Number(item.confirmedQuantity || 0),
                                  0
                                )
                              const usesBulkPrice = offering
                                ? allocatedQuantity >=
                                  Number(offering.bulkMinimumOrderQuantity)
                                : false
                              return (
                                <div
                                  key={line.lineKey}
                                  className="grid min-w-0 gap-3 rounded-md border border-border p-3 md:grid-cols-2 lg:grid-cols-4"
                                >
                                  <div className="min-w-0 space-y-1.5 md:col-span-2 lg:col-span-2">
                                    <Label>供应商报价</Label>
                                    <OptionCombobox
                                      value={
                                        line.offeringRevisionId || undefined
                                      }
                                      onValueChange={(revisionId) => {
                                        const nextOffering = supplyOptions.find(
                                          (option) =>
                                            option.offeringRevisionId ===
                                            revisionId
                                        )
                                        const supplier = supplierOptions?.find(
                                          (option) =>
                                            option.supplierId ===
                                            nextOffering?.supplierId
                                        )
                                        const matchingCapabilities =
                                          nextOffering?.capabilities.filter(
                                            (capability) =>
                                              capability.capabilityCode ===
                                              capabilityCodeForMode(
                                                line.fulfillmentMode
                                              )
                                          ) ?? []
                                        const capability =
                                          matchingCapabilities.length === 1
                                            ? matchingCapabilities[0]
                                            : undefined
                                        updatePlanLine(line.lineKey, {
                                          supplierId:
                                            nextOffering?.supplierId ?? "",
                                          supplierName:
                                            supplier?.supplierName ?? "",
                                          offeringRevisionId:
                                            nextOffering?.offeringRevisionId ??
                                            "",
                                          inputTaxRate:
                                            nextOffering?.inputTaxRate ?? "",
                                          capabilityRevisionId:
                                            capability?.revisionId ?? "",
                                          capabilitySummary:
                                            capability?.label ??
                                            "请选择供应资质",
                                          qualificationStatus: capability
                                            ? "VALID"
                                            : "INVALID",
                                        })
                                      }}
                                      options={offeringOptionsForSku(
                                        subLine.itemSku
                                      )}
                                      disabled={formalPending}
                                      placeholder="选择供应商报价"
                                      className="w-full min-w-0"
                                      inputClassName="h-9 min-h-9"
                                    />
                                  </div>

                                  <div className="min-w-0 space-y-1.5">
                                    <Label>采购数量</Label>
                                    <Input
                                      className="num h-9"
                                      inputMode="decimal"
                                      value={line.confirmedQuantity}
                                      onChange={(event) =>
                                        updatePlanLine(line.lineKey, {
                                          confirmedQuantity:
                                            event.target.value,
                                        })
                                      }
                                      disabled={formalPending}
                                    />
                                  </div>

                                  <div className="min-w-0 space-y-1.5">
                                    <Label>含税单价</Label>
                                    <div className="flex h-9 min-w-0 items-center justify-between gap-2 rounded-md border border-border bg-muted/30 px-3 text-sm">
                                      <span className="num shrink-0 font-medium">
                                        {line.latestCostGross
                                          ? money.format(
                                              Number(line.latestCostGross)
                                            )
                                          : "—"}
                                      </span>
                                      <span className="truncate text-xs text-muted-foreground">
                                        {offering
                                          ? usesBulkPrice
                                            ? "集采价"
                                            : "一件代发价"
                                          : "选择供应商后计算"}
                                      </span>
                                    </div>
                                  </div>

                                  <div className="min-w-0 space-y-1.5">
                                    <Label>交付方式</Label>
                                    <OptionCombobox
                                      value={line.fulfillmentMode}
                                      onValueChange={(value) => {
                                        if (!value) return
                                        const fulfillmentMode =
                                          value as FulfillmentMode
                                        const capabilities =
                                          offering?.capabilities.filter(
                                            (capability) =>
                                              capability.capabilityCode ===
                                              capabilityCodeForMode(
                                                fulfillmentMode
                                              )
                                          ) ?? []
                                        const capability =
                                          capabilities.length === 1
                                            ? capabilities[0]
                                            : undefined
                                        updatePlanLine(line.lineKey, {
                                          fulfillmentMode,
                                          capabilityRevisionId:
                                            capability?.revisionId ?? "",
                                          capabilitySummary:
                                            capability?.label ??
                                            "请选择供应资质",
                                          qualificationStatus: capability
                                            ? "VALID"
                                            : "INVALID",
                                        })
                                      }}
                                      options={fulfillmentOptionsForOffering(
                                        line.offeringRevisionId
                                      )}
                                      allowClear={false}
                                      disabled={formalPending}
                                      placeholder="选择交付方式"
                                      className="w-full min-w-0"
                                      inputClassName="h-9 min-h-9"
                                    />
                                  </div>

                                  <div className="min-w-0 space-y-1.5">
                                    <Label>供应商交期</Label>
                                    <DatePicker
                                      value={
                                        line.expectedDeliveryDate || undefined
                                      }
                                      onValueChange={(value) =>
                                        updatePlanLine(line.lineKey, {
                                          expectedDeliveryDate: value ?? "",
                                        })
                                      }
                                      disabled={formalPending}
                                      className="h-9 w-full min-w-0"
                                    />
                                  </div>

                                  <div className="min-w-0 space-y-1.5">
                                    <Label>供应资质</Label>
                                    <OptionCombobox
                                      value={
                                        line.capabilityRevisionId || undefined
                                      }
                                      onValueChange={(revisionId) => {
                                        const capability =
                                          capabilityOptionsForOffering(
                                            line.offeringRevisionId,
                                            line.fulfillmentMode
                                          ).find(
                                            (option) =>
                                              option.value === revisionId
                                          )
                                        updatePlanLine(line.lineKey, {
                                          capabilityRevisionId:
                                            revisionId ?? "",
                                          capabilitySummary:
                                            capability?.label ?? "",
                                          qualificationStatus: revisionId
                                            ? "VALID"
                                            : "INVALID",
                                        })
                                      }}
                                      options={capabilityOptionsForOffering(
                                        line.offeringRevisionId,
                                        line.fulfillmentMode
                                      )}
                                      disabled={formalPending}
                                      placeholder="选择供应资质"
                                      className="w-full min-w-0"
                                      inputClassName="h-9 min-h-9"
                                    />
                                  </div>

                                  <div className="flex min-w-0 items-end justify-end">
                                    <Button
                                      type="button"
                                      size="sm"
                                      variant="ghost"
                                      disabled={
                                        formalPending || planLines.length <= 1
                                      }
                                      onClick={() => removeLine(line.lineKey)}
                                    >
                                      删除
                                    </Button>
                                  </div>
                                </div>
                              )
                            })}

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
                              增加供应商
                            </Button>
                          </div>
                        </div>
                      )
                    })}

                    {clientBlocking.length > 0 ? (
                      <ValidationSummary
                        title="生成采购单前需要补齐"
                        issues={clientBlocking}
                      />
                    ) : null}
                  </section>

                  {recommendation.warnings.length > 0 ? (
                    <Alert variant="warning">
                      <TriangleAlertIcon aria-hidden="true" />
                      <AlertTitle>生成前请确认</AlertTitle>
                      <AlertDescription>
                        {recommendation.warnings.map((item) => item.message).join("；")}
                      </AlertDescription>
                    </Alert>
                  ) : null}
                </div>
              ) : (
                <Alert variant="destructive">
                  <TriangleAlertIcon aria-hidden="true" />
                  <AlertTitle>暂时无法形成完整采购方案</AlertTitle>
                  <AlertDescription>
                    {recommendation?.blockingIssues.map((item) => item.message).join("；") || "当前供应商报价或可供数量不足。"}
                  </AlertDescription>
                </Alert>
              )}

              <DialogFooter>
                <DialogClose render={<Button type="button" variant="outline" /> }>
                  返回销售单
                </DialogClose>
                <Button
                  type="button"
                  disabled={
                    !recommendation?.ready ||
                    !allCovered ||
                    recommendationQuery.isPending ||
                    saveMutation.isPending ||
                    completeMutation.isPending
                  }
                  onClick={() => void handleApprove()}
                >
                  <CheckIcon data-icon="inline-start" aria-hidden="true" />
                  {saveMutation.isPending || completeMutation.isPending
                      ? "正在生成采购单…"
                      : advanceAfterConfirm
                      ? "保存调整、生成采购单并打开下一条"
                      : "保存调整并生成采购单"}
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>

          <Dialog open={rejectOpen} onOpenChange={setRejectOpen}>
            <DialogContent className="sm:max-w-lg">
              <DialogHeader>
                <DialogTitle>驳回采购二次确认</DialogTitle>
                <DialogDescription>
                  将形成本次确认的驳回结论并结束当前任务；不创建采购单、变更单或后继任务。销售可在销售单选择三条固定出路。
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
                      <OptionCombobox
                        id="reject-reason-code"
                        value={field.state.value}
                        onValueChange={(value) => {
                          if (value)
                            field.handleChange(value as RejectReasonCode)
                        }}
                        options={(
                          Object.keys(REJECT_REASON_LABEL) as RejectReasonCode[]
                        ).map((code) => ({
                          value: code,
                          label: REJECT_REASON_LABEL[code],
                        }))}
                        allowClear={false}
                        aria-label="驳回原因"
                        placeholder="请选择驳回原因"
                      />
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
                    销售后续三条固定出路（驳回后只读展示）
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
    </PageScaffold>
  )
}

function buildResultFacts(
  outcome: FormalOutcome | undefined,
  context:
    | {
        position: number
        total: number
      }
    | undefined,
  submissionNo?: number
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
        label: "提交记录",
        value: submissionNo ? `第 ${submissionNo} 次提交` : "本次提交",
      },
      { label: "处理结果", value: "销售单已生效" },
      {
        label: "采购单草稿",
        value:
          outcome.purchaseOrders.length > 0
            ? outcome.purchaseOrders.map((order) => order.purchaseNo).join("、")
            : "未生成，请联系管理员核查",
      },
      { label: "下一环节", value: "核对采购单草稿并提交" },
    ]
  }
  if (outcome.kind === "REJECTED_TO_SALES") {
    return [
      { label: "销售单", value: outcome.salesOrderNo },
      {
        label: "处理结果",
        value: "本次提交已驳回，未创建采购单或后继任务",
      },
      {
        label: "驳回原因",
        value: `${REJECT_REASON_LABEL[outcome.rejectReasonCode]} · ${outcome.comment}`,
      },
      { label: "销售下一步", value: "改品/改价后重提，或作废" },
    ]
  }
  return [
    {
      label: "任务状态",
      value: outcome.workItemStatus === "IN_PROGRESS" ? "处理中" : "待处理",
    },
    {
      label: "处理状态",
      value: outcome.leaseDisposition === "RELEASED" ? "已结束" : "保留",
    },
  ]
}

function ResultActions({
  lastResult,
  taskSalesOrderId,
  returnTo,
  onNext,
}: {
  lastResult: NonNullable<ResultState>
  taskSalesOrderId?: string
  returnTo: string
  onNext: () => void
}) {
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
          查看详情
        </Button>
      ) : null}
      {lastResult.outcome?.kind === "APPROVED_AND_SALES_EFFECTIVE" ? (
        lastResult.outcome.purchaseOrders.map((order) => (
          <Button
            key={order.purchaseOrderId}
            type="button"
            size="sm"
            variant="outline"
            render={
              <Link href={`/procurement/orders/${order.purchaseOrderId}`} />
            }
          >
            查看采购单 · {order.purchaseNo}
          </Button>
        ))
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
}: {
  salesOrderId: string
  returnTo: string
}) {
  return (
    <Card size="sm" className={`mt-3 ${surfacePanelClassName}`}>
      <CardHeader className="rounded-t-lg border-b border-border/30">
        <CardTitle>销售三条固定出路</CardTitle>
        <CardDescription>
          上一驳回提交已作废；本页只读展示出路，不代销售选择。
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
          打开销售单驳回处理
          <ArrowRightIcon data-icon="inline-end" aria-hidden="true" />
        </Button>
      </CardContent>
    </Card>
  )
}
