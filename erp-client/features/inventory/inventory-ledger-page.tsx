"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  DownloadIcon,
  ExternalLinkIcon,
  RefreshCwIcon,
  SearchIcon,
  SlashIcon,
} from "lucide-react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { z } from "zod"

import {
  BackgroundJobProgress,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FormalActionConfirmDialog,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  OptionCombobox,
  PageActions,
  PageHeader,
  QuickPreviewSheet,
  WarehouseCombobox,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { formatDateTime } from "@/lib/datetime"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { type ResultState } from "@/components/business/feedback"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  useBalanceDetailQuery,
  useCreateAdjustmentDraftMutation,
  useExportJobQuery,
  useInventoryListQuery,
  useResolveAdjustmentUnknownMutation,
  useStartInventoryExportMutation,
  useSubmitAdjustmentMutation,
} from "@/features/inventory/queries"
import type {
  AdjustmentReasonType,
  InventoryAvailability,
  InventoryQuery,
  InventoryView,
  StockAdjustmentRow,
  StockBalanceRow,
  StockMovementRow,
  StockReservationRow,
} from "@/features/inventory/types"
import {
  AVAILABILITY_LABEL,
  REASON_TYPE_OPTIONS,
  VIEW_LABEL,
} from "@/features/inventory/types"
import {
  decodeInventoryCursor,
  encodeInventoryCursor,
} from "@/features/inventory/cursor"
import { bumpInventoryBalanceLock } from "@/mock/session-state"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"
import { resultText } from "@/lib/ui-text"

function parseView(raw: string | null): InventoryView {
  if (
    raw === "movement" ||
    raw === "reservation" ||
    raw === "adjustment" ||
    raw === "balance"
  ) {
    return raw
  }
  return "balance"
}

function parseAvailability(raw: string | null): InventoryAvailability {
  if (
    raw === "positive" ||
    raw === "zero" ||
    raw === "reserved" ||
    raw === "all"
  ) {
    return raw
  }
  return "all"
}

const MOVEMENT_FROM_DEFAULT = "2026-07-03"
const MOVEMENT_TO_DEFAULT = "2026-08-02"

const MOVEMENT_TYPE_OPTIONS = [
  { value: "PURCHASE_RECEIPT", label: "采购入库" },
  { value: "WAREHOUSE_DISPATCH", label: "仓库发出" },
  { value: "RESERVATION_ESTABLISH", label: "建立预占" },
  { value: "RESERVATION_CONSUME", label: "消耗预占" },
  { value: "STOCK_ADJUSTMENT", label: "库存调整" },
  { value: "OPENING_IMPORT", label: "期初导入" },
] as const

function defaultSortValue(view: InventoryView): string {
  if (view === "balance") return "warehouseCode:asc,skuCode:asc"
  if (view === "movement") return "occurredAt:desc,movementId:desc"
  if (view === "reservation") {
    return "establishedAt:desc,reservationId:desc"
  }
  return "createdAt:desc,adjustmentId:desc"
}

function sortOptions(view: InventoryView) {
  if (view === "balance") {
    return [
      { value: "warehouseCode:asc,skuCode:asc", label: "仓库 / SKU" },
      { value: "lastMovementAt:desc,skuCode:asc", label: "最近变动" },
    ]
  }
  if (view === "movement") {
    return [
      { value: "occurredAt:desc,movementId:desc", label: "发生时间（新到旧）" },
      { value: "occurredAt:asc,movementId:asc", label: "发生时间（旧到新）" },
      { value: "recordedAt:desc,movementId:desc", label: "记录时间（新到旧）" },
    ]
  }
  if (view === "reservation") {
    return [
      { value: "establishedAt:desc,reservationId:desc", label: "建立时间" },
      { value: "salesOrderNo:asc,reservationId:asc", label: "销售单号" },
    ]
  }
  return [
    { value: "createdAt:desc,adjustmentId:desc", label: "创建时间" },
    { value: "adjustmentNo:asc,adjustmentId:asc", label: "调整单号" },
  ]
}

function formatQty(value: string, unit: string) {
  return (
    <span className="num text-sm">
      {value}
      <span className="ml-1 text-xs font-normal text-muted-foreground">
        {unit}
      </span>
    </span>
  )
}

const adjustSchema = z.object({
  reasonType: z.enum(["COUNT_GAIN", "COUNT_LOSS", "DAMAGE", "OTHER"]),
  quantity: z
    .string()
    .trim()
    .min(1, "请填写调整数量")
    .refine((v) => {
      try {
        parseDecimal(v, { maxScale: 6 })
        return compareDecimal(v, "0", 6) > 0
      } catch {
        return false
      }
    }, "数量必须为正数"),
  note: z.string().trim().min(2, "请填写至少 2 个字的原因说明"),
  occurredAt: z.string().min(1, "请填写业务发生时间"),
})

export function InventoryLedgerPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  // §10: 375 窄屏只读；平板仍可发起调整
  const isPhoneNarrow = React.useSyncExternalStore(
    (onChange) => {
      const mq = window.matchMedia("(max-width: 480px)")
      mq.addEventListener("change", onChange)
      return () => mq.removeEventListener("change", onChange)
    },
    () => window.matchMedia("(max-width: 480px)").matches,
    () => false
  )

  const view = parseView(searchParams.get("view"))
  const qParam = searchParams.get("q") ?? ""
  const warehouseId = searchParams.get("warehouseId") ?? undefined
  const skuId = searchParams.get("skuId") ?? undefined
  const salesOrderLineId = searchParams.get("salesOrderLineId") ?? undefined
  const availability = parseAvailability(searchParams.get("availability"))
  const balanceIdParam = searchParams.get("balanceId") ?? undefined
  const adjustmentIdParam = searchParams.get("adjustmentId") ?? undefined
  const movementTypeParam = searchParams.get("movementType") ?? ""
  const movementType = React.useMemo(
    () => movementTypeParam.split(",").filter(Boolean),
    [movementTypeParam]
  )
  const occurredFrom =
    searchParams.get("occurredFrom") ??
    (view === "movement" ? MOVEMENT_FROM_DEFAULT : undefined)
  const occurredTo =
    searchParams.get("occurredTo") ??
    (view === "movement" ? MOVEMENT_TO_DEFAULT : undefined)
  const sortValue = searchParams.get("sort") ?? defaultSortValue(view)
  const pageSizeParam = Number(searchParams.get("pageSize") ?? "20")
  const pageSize =
    Number.isSafeInteger(pageSizeParam) && pageSizeParam > 0
      ? Math.min(pageSizeParam, 100)
      : 20
  const cursorParam = searchParams.get("cursor") ?? undefined
  const cursorOffset = decodeInventoryCursor(cursorParam, view)

  const [searchInput, setSearchInput] = React.useState(qParam)
  const searchInputRef = React.useRef<HTMLInputElement | null>(null)
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: Math.floor(cursorOffset / pageSize),
    pageSize,
  })
  const [previewBalanceId, setPreviewBalanceId] = React.useState<string | null>(
    balanceIdParam ?? null
  )
  const [adjustBalanceId, setAdjustBalanceId] = React.useState<string | null>(
    null
  )
  const [adjustDraftId, setAdjustDraftId] = React.useState<string | null>(null)
  const [adjustLockVersion, setAdjustLockVersion] = React.useState<number>(0)
  const [adjustSeedLock, setAdjustSeedLock] = React.useState<number>(0)
  const [adjustMeta, setAdjustMeta] = React.useState<{
    warehouseName: string
    skuCode: string
    skuName: string
    baseUnit: string
    onHand: string
    available: string
    adjustmentNo: string
    editVersion: number
    segregationNote: string
  } | null>(null)
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [lastResult, setLastResult] = React.useState<ResultState>(null)
  const [exportJobId, setExportJobId] = React.useState<string | null>(null)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [forceUnknownOnce, setForceUnknownOnce] = React.useState(false)
  const [pendingPayload, setPendingPayload] = React.useState<{
    stockAdjustmentId: string
    expectedBalanceLockVersion: number
    seedBalanceLockVersion: number
    reasonType: AdjustmentReasonType
    reasonTypeLabel: string
    direction: "increase" | "decrease"
    quantity: string
    note: string
    occurredAt: string
    idempotencyKey: string
    forceUnknown?: boolean
  } | null>(null)

  const rowFocusRef = React.useRef<Map<string, HTMLButtonElement | null>>(
    new Map()
  )
  const restoreFocusIdRef = React.useRef<string | null>(null)
  const idempotencyRef = React.useRef<string | null>(null)

  // Debounce search → URL
  React.useEffect(() => {
    setSearchInput(qParam)
  }, [qParam])

  // `/` focuses list search (ignore when typing in inputs)
  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return
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
      searchInputRef.current?.focus()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (searchInput === qParam) return
      patchUrl({ q: searchInput.trim() || null })
    }, 300)
    return () => globalThis.clearTimeout(handle)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- patchUrl is stable enough via searchParams
  }, [searchInput])

  const query: InventoryQuery = React.useMemo(
    () => ({
      view,
      q: qParam || undefined,
      warehouseId,
      skuId,
      salesOrderLineId,
      availability,
      movementType,
      occurredFrom,
      occurredTo,
      cursor: cursorParam,
      pageSize: pagination.pageSize,
      sort: sortValue.split(",").filter(Boolean),
      balanceId: balanceIdParam,
      adjustmentId: adjustmentIdParam,
    }),
    [
      view,
      qParam,
      warehouseId,
      skuId,
      salesOrderLineId,
      availability,
      movementType,
      occurredFrom,
      occurredTo,
      cursorParam,
      pagination.pageSize,
      sortValue,
      balanceIdParam,
      adjustmentIdParam,
    ]
  )

  const listQuery = useInventoryListQuery(query)
  const detailQuery = useBalanceDetailQuery(previewBalanceId)
  const createDraftMutation = useCreateAdjustmentDraftMutation()
  const submitMutation = useSubmitAdjustmentMutation()
  const resolveUnknownMutation = useResolveAdjustmentUnknownMutation()
  const exportMutation = useStartInventoryExportMutation()
  const exportJobQuery = useExportJobQuery(exportJobId)

  const data = listQuery.data

  function patchUrl(
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean }
  ) {
    patchSearchParams(
      { router, pathname, searchParams, view, clearCursor: true },
      patch,
      options
    )
  }

  const resetPagination = React.useCallback(() => {
    setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
  }, [])

  const handlePaginationChange = React.useCallback(
    (next: PaginationState) => {
      setPagination(next)
      const offset = next.pageIndex * next.pageSize
      patchUrl(
        {
          cursor:
            offset === 0 ? null : encodeInventoryCursor(view, offset),
          pageSize: String(next.pageSize),
        },
        { replace: true }
      )
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps -- patchUrl uses the current URL snapshot
    [pathname, searchParams, view]
  )

  React.useEffect(() => {
    const expectedPageIndex = Math.floor(cursorOffset / pageSize)
    setPagination((current) =>
      current.pageIndex === expectedPageIndex && current.pageSize === pageSize
        ? current
        : { pageIndex: expectedPageIndex, pageSize }
    )
  }, [cursorOffset, pageSize])

  // Restore focus after detail/adjust close
  React.useEffect(() => {
    if (previewBalanceId || adjustDraftId) return
    const id = restoreFocusIdRef.current
    if (!id) return
    const el = rowFocusRef.current.get(id)
    if (el) {
      el.focus()
      restoreFocusIdRef.current = null
    }
  }, [previewBalanceId, adjustDraftId])

  const form = useAppForm({
    defaultValues: {
      reasonType: "COUNT_LOSS" as AdjustmentReasonType,
      quantity: "",
      note: "",
      occurredAt: new Date().toISOString().slice(0, 16),
    },
    validators: {
      onChange: adjustSchema,
    },
    onSubmit: async () => {
      setConfirmOpen(true)
    },
  })

  const openDetail = React.useCallback(
    (balanceId: string) => {
      restoreFocusIdRef.current = balanceId
      setPreviewBalanceId(balanceId)
      patchUrl({ balanceId }, { replace: true })
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [searchParams, pathname, view]
  )

  const closeDetail = React.useCallback(() => {
    setPreviewBalanceId(null)
    patchUrl({ balanceId: null }, { replace: true })
    // eslint-disable-next-line react-hooks/exhaustive-deps -- patchUrl uses the current URL snapshot
  }, [searchParams, pathname, view])

  const startAdjustment = React.useCallback(
    async (row: StockBalanceRow) => {
      if (isPhoneNarrow) {
        setActionError("窄屏（移动端）仅支持只读查询；库存调整请在桌面完成。")
        return
      }
      if (!row.allowedActions.includes("CREATE_ADJUSTMENT")) {
        setActionError(
          row.actionBlockers.find((b) => b.action === "CREATE_ADJUSTMENT")
            ?.message ?? "当前不允许发起库存调整"
        )
        return
      }
      restoreFocusIdRef.current = row.balanceId
      setActionError(null)
      setLastResult(null)
      idempotencyRef.current = null
      try {
        const draft = await createDraftMutation.mutateAsync({
          balanceId: row.balanceId,
        })
        setAdjustBalanceId(row.balanceId)
        setAdjustDraftId(draft.stockAdjustmentId)
        setAdjustLockVersion(draft.balanceLockVersion)
        setAdjustSeedLock(row.lockVersion)
        setAdjustMeta({
          warehouseName: draft.warehouseName,
          skuCode: draft.skuCode,
          skuName: draft.skuName,
          baseUnit: draft.baseUnit,
          onHand: row.onHandQuantity,
          available: row.availableQuantity,
          adjustmentNo: draft.adjustmentNo,
          editVersion: draft.editVersion,
          segregationNote: draft.segregationNote,
        })
        form.reset()
        form.setFieldValue("reasonType", draft.reasonType)
        form.setFieldValue("quantity", draft.quantity)
        form.setFieldValue("note", draft.note)
        form.setFieldValue(
          "occurredAt",
          draft.occurredAt.slice(0, 16) ||
            new Date().toISOString().slice(0, 16)
        )
        setPreviewBalanceId(null)
      } catch (err) {
        setActionError(
          err instanceof Error ? err.message : "创建调整草稿失败"
        )
      }
    },
    [createDraftMutation, form, isPhoneNarrow]
  )

  const closeAdjustment = React.useCallback(() => {
    setAdjustDraftId(null)
    setAdjustBalanceId(null)
    setAdjustMeta(null)
    setConfirmOpen(false)
    setPendingPayload(null)
  }, [])

  const doSubmit = React.useCallback(async () => {
    if (!adjustDraftId || !adjustMeta) return
    const values = form.state.values
    const reason =
      REASON_TYPE_OPTIONS.find((r) => r.value === values.reasonType) ??
      REASON_TYPE_OPTIONS[1]
    if (!idempotencyRef.current) {
      idempotencyRef.current = `w10-adj-${adjustDraftId}-${Date.now()}`
    }
    const payload = {
      stockAdjustmentId: adjustDraftId,
      expectedBalanceLockVersion: adjustLockVersion,
      seedBalanceLockVersion: adjustSeedLock,
      reasonType: values.reasonType,
      reasonTypeLabel: reason.label,
      direction: reason.direction,
      quantity: values.quantity.trim(),
      note: values.note.trim(),
      occurredAt: values.occurredAt,
      idempotencyKey: idempotencyRef.current,
      forceUnknown: forceUnknownOnce,
    }
    setForceUnknownOnce(false)
    setPendingPayload(payload)
    setActionError(null)
    const result = await submitMutation.mutateAsync(payload)
    if (result.status === "succeeded") {
      setLastResult({
        status: "succeeded",
        title: "调整已提交待复核",
        description: `单号 ${result.outcome.adjustmentNo}。下一责任方：${result.outcome.nextResponsible}。余额尚未变化，确认入账后由系统刷新。`,
        reference: result.outcome.reference,
      })
      setConfirmOpen(false)
      closeAdjustment()
      return
    }
    if (result.status === "unknown") {
      setLastResult({
        status: "unknown",
        title: resultText.unknown,
        description: result.message,
        reference: result.idempotencyKey,
        pendingIdempotencyKey: result.idempotencyKey,
      })
      setConfirmOpen(false)
      return
    }
    if (result.code === "VERSION_CONFLICT" && result.latestLockVersion != null) {
      setAdjustLockVersion(result.latestLockVersion)
      setActionError(result.message)
      setConfirmOpen(false)
      return
    }
    setActionError(result.message)
    setConfirmOpen(false)
  }, [
    adjustDraftId,
    adjustMeta,
    adjustLockVersion,
    adjustSeedLock,
    form.state.values,
    forceUnknownOnce,
    submitMutation,
    closeAdjustment,
  ])

  const balanceColumns = React.useMemo<ColumnDef<StockBalanceRow>[]>(
    () => [
      {
        id: "identity",
        header: "仓库 / SKU",
        meta: { label: "仓库 / SKU", width: "reference" },
        cell: ({ row }) => (
          <div className="min-w-0">
            <div className="truncate text-sm font-medium">
              {row.original.warehouseName}
              <span className="ml-1 num text-xs text-muted-foreground">
                {row.original.warehouseCode}
              </span>
            </div>
            <div className="truncate text-sm">
              <span className="num">{row.original.skuCode}</span>
              <span className="text-muted-foreground"> · </span>
              {row.original.skuName}
            </div>
            <div className="truncate text-xs text-muted-foreground">
              {row.original.specSummary}
            </div>
          </div>
        ),
      },
      {
        id: "onHand",
        header: "账面现存",
        meta: {
          label: "账面现存",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) =>
          formatQty(row.original.onHandQuantity, row.original.baseUnit),
      },
      {
        id: "reserved",
        header: "有效预占",
        meta: {
          label: "有效预占",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) =>
          formatQty(row.original.reservedQuantity, row.original.baseUnit),
      },
      {
        id: "available",
        header: "可用数量",
        meta: {
          label: "可用数量",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) => (
          <div className="flex flex-col items-end gap-0.5">
            {formatQty(row.original.availableQuantity, row.original.baseUnit)}
            {row.original.availableQuantity === "0" ? (
              <Badge variant="destructive" className="text-[10px]">
                零可用
              </Badge>
            ) : null}
          </div>
        ),
      },
      {
        id: "status",
        header: "状态",
        meta: { label: "状态", width: "status" },
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
      {
        id: "lastMovement",
        header: "最后变动",
        meta: { label: "最后变动", width: "default" },
        cell: ({ row }) => (
          <div className="text-sm">
            <div>{row.original.lastMovementTypeLabel}</div>
            <div className="num text-xs text-muted-foreground">
              {formatDateTime(row.original.lastMovementAt, "full", "passthrough")}
            </div>
          </div>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        cell: ({ row }) => {
          const canAdjust =
            !isPhoneNarrow &&
            row.original.allowedActions.includes("CREATE_ADJUSTMENT")
          const blocker = isPhoneNarrow
            ? {
                action: "CREATE_ADJUSTMENT",
                code: "MOBILE_READONLY",
                message: "窄屏仅只读，请在桌面发起库存调整",
              }
            : row.original.actionBlockers.find(
                (b) => b.action === "CREATE_ADJUSTMENT"
              )
          return (
            <div className="flex justify-end gap-1">
              <Button
                type="button"
                variant="ghost"
                size="xs"
                ref={(el) => {
                  rowFocusRef.current.set(row.original.balanceId, el)
                }}
                onClick={() => openDetail(row.original.balanceId)}
              >
                查看
              </Button>
              <Button
                type="button"
                variant="outline"
                size="xs"
                disabled={!canAdjust}
                title={blocker?.message}
                onClick={() => void startAdjustment(row.original)}
              >
                库存调整
              </Button>
            </div>
          )
        },
      },
    ],
    [openDetail, startAdjustment, isPhoneNarrow]
  )

  const movementColumns = React.useMemo<ColumnDef<StockMovementRow>[]>(
    () => [
      {
        id: "identity",
        header: "仓库 / SKU",
        meta: { label: "仓库 / SKU", width: "reference" },
        cell: ({ row }) => (
          <div className="min-w-0 text-sm">
            <div className="truncate font-medium">
              {row.original.warehouseName}
            </div>
            <div className="truncate">
              <span className="num">{row.original.skuCode}</span>
              <span className="text-muted-foreground"> · </span>
              {row.original.skuName}
            </div>
          </div>
        ),
      },
      {
        id: "type",
        header: "流水类型",
        meta: { label: "流水类型", width: "default" },
        cell: ({ row }) => (
          <div className="text-sm">
            <div>{row.original.movementTypeLabel}</div>
            <div className="text-xs text-muted-foreground">
              {row.original.direction === "increase" ? "增加" : "减少"}
            </div>
          </div>
        ),
      },
      {
        id: "qty",
        header: "数量",
        meta: {
          label: "数量",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) =>
          formatQty(row.original.quantity, row.original.baseUnit),
      },
      {
        id: "occurred",
        header: "发生 / 记录",
        meta: { label: "时间", width: "default", numeric: true },
        cell: ({ row }) => (
          <div className="num text-xs text-muted-foreground">
            <div>发生 {formatDateTime(row.original.occurredAt, "full", "passthrough")}</div>
            <div>记录 {formatDateTime(row.original.recordedAt, "full", "passthrough")}</div>
          </div>
        ),
      },
      {
        id: "source",
        header: "来源单据",
        meta: { label: "来源单据", width: "default" },
        cell: ({ row }) =>
          row.original.sourceHref ? (
            <Button
              type="button"
              variant="link"
              size="xs"
              className="h-auto px-0"
              render={
                <Link
                  href={row.original.sourceHref}
                  aria-label={`查看来源 ${row.original.sourceDocumentNo}`}
                />
              }
            >
              <span className="num">{row.original.sourceDocumentNo}</span>
              <ExternalLinkIcon className="ml-1 size-3" aria-hidden />
            </Button>
          ) : (
            <span className="num text-sm">{row.original.sourceDocumentNo}</span>
          ),
      },
      {
        id: "recorder",
        header: "记录人",
        meta: { label: "记录人", width: "default" },
        cell: ({ row }) => (
          <span className="text-sm">{row.original.recordedByLabel}</span>
        ),
      },
    ],
    []
  )

  const reservationColumns = React.useMemo<ColumnDef<StockReservationRow>[]>(
    () => [
      {
        id: "identity",
        header: "仓库 / SKU",
        meta: { label: "仓库 / SKU", width: "reference" },
        cell: ({ row }) => (
          <div className="min-w-0 text-sm">
            <div className="truncate font-medium">
              {row.original.warehouseName}
            </div>
            <div className="truncate">
              <span className="num">{row.original.skuCode}</span>
              <span className="text-muted-foreground"> · </span>
              {row.original.skuName}
            </div>
          </div>
        ),
      },
      {
        id: "sales",
        header: "销售单 / 明细",
        meta: { label: "销售单", width: "default" },
        cell: ({ row }) => (
          <div className="text-sm">
            <div className="num">{row.original.salesOrderNo}</div>
            <div className="text-xs text-muted-foreground">
              {row.original.salesOrderLineLabel}
            </div>
          </div>
        ),
      },
      {
        id: "qty",
        header: "建立 / 剩余",
        meta: {
          label: "数量",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) => (
          <div className="text-end text-sm">
            <div className="num">
              {row.original.establishedQuantity} /{" "}
              {row.original.remainingQuantity}
              <span className="ml-1 text-xs text-muted-foreground">
                {row.original.baseUnit}
              </span>
            </div>
            <div className="text-xs text-muted-foreground">
              已消耗 {row.original.consumedQuantity} · 已释放{" "}
              {row.original.releasedQuantity}
            </div>
          </div>
        ),
      },
      {
        id: "status",
        header: "状态",
        meta: { label: "状态", width: "status" },
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
      {
        id: "source",
        header: "入库来源",
        meta: { label: "入库来源", width: "default" },
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.inboundSourceDocumentNo ?? "—"}
          </span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        cell: ({ row }) => (
          <div className="flex justify-end gap-1">
            {row.original.fulfillmentHref ? (
              <Button
                type="button"
                variant="outline"
                size="xs"
                render={<Link href={row.original.fulfillmentHref} />}
              >
                履约上下文
              </Button>
            ) : null}
            {/* 明确不提供释放预占入口 */}
          </div>
        ),
      },
    ],
    []
  )

  const adjustmentColumns = React.useMemo<ColumnDef<StockAdjustmentRow>[]>(
    () => [
      {
        id: "doc",
        header: "调整单",
        meta: { label: "调整单", width: "reference" },
        cell: ({ row }) => (
          <div className="text-sm">
            <div className="num font-medium">{row.original.adjustmentNo}</div>
            <div className="text-xs text-muted-foreground">
              {row.original.reasonTypeLabel} ·{" "}
              {row.original.direction === "increase" ? "增加" : "减少"}{" "}
              {row.original.quantity} {row.original.baseUnit}
            </div>
          </div>
        ),
      },
      {
        id: "identity",
        header: "仓库 / SKU",
        meta: { label: "仓库 / SKU", width: "default" },
        cell: ({ row }) => (
          <div className="text-sm">
            <div>{row.original.warehouseName}</div>
            <div className="num text-xs text-muted-foreground">
              {row.original.skuCode} · {row.original.skuName}
            </div>
          </div>
        ),
      },
      {
        id: "status",
        header: "状态",
        meta: { label: "状态", width: "status" },
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
      {
        id: "people",
        header: "岗位",
        meta: { label: "岗位", width: "default" },
        cell: ({ row }) => (
          <div className="text-xs text-muted-foreground">
            <div>经办 {row.original.operatorLabel}</div>
            <div>
              仓储复核 {row.original.warehouseReviewerLabel ?? "—"}
            </div>
            <div>
              财务确认 {row.original.financeConfirmerLabel ?? "—"}
            </div>
          </div>
        ),
      },
      {
        id: "time",
        header: "创建 / 确认入账",
        meta: { label: "时间", width: "default", numeric: true },
        cell: ({ row }) => (
          <div className="num text-xs text-muted-foreground">
            <div>创建 {formatDateTime(row.original.createdAt, "full", "passthrough")}</div>
            <div>
              确认入账{" "}
              {row.original.postedAt
                ? formatDateTime(row.original.postedAt, "full", "passthrough")
                : "—"}
            </div>
          </div>
        ),
      },
    ],
    []
  )

  if (listQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
        <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-20 animate-pulse rounded-2xl bg-muted" />
          ))}
        </div>
        <div className="h-12 animate-pulse rounded-xl bg-muted" />
        <div className="h-[28rem] animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (listQuery.isError || !data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="库存台账" description="加载失败" />
        <BusinessFailureState
          kind="system"
          action={
            <Button type="button" onClick={() => void listQuery.refetch()}>
              重试
            </Button>
          }
        />
      </div>
    )
  }

  if (data.emptyReason === "PERMISSION_REVOKED") {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="库存台账"
          description="模块权限已收回，相关数据已不再展示。"
        />
        <BusinessFailureState
          kind="permission"
          title="权限已收回"
          description="当前账号的库存台账访问权限已被收回。余额、流水、导出结果与展开来源均不可见。"
          action={
            <Button type="button" onClick={() => void listQuery.refetch()}>
              重新检查权限
            </Button>
          }
        />
      </div>
    )
  }

  if (data.emptyReason === "NO_DATA_SCOPE") {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="库存台账"
          description="有模块权限但未配置仓库数据范围。"
        />
        <BusinessEmptyState
          kind="no-scope"
          title="当前角色未配置仓库数据范围"
          description="不能显示为库存为 0。请联系管理员配置仓库授权后再查询。"
        />
      </div>
    )
  }

  const pageRows = (() => {
    if (view === "balance") {
      return data.balances
    }
    if (view === "movement") {
      return data.movements
    }
    if (view === "reservation") {
      return data.reservations
    }
    return data.adjustments
  })()

  const metricActive =
    availability === "zero"
      ? "zero"
      : availability === "reserved"
        ? "reserved"
        : view === "adjustment"
          ? "pending"
          : "combos"

  const detail = detailQuery.data
  const exportJob = exportJobQuery.data

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="库存台账"
        description={
          isPhoneNarrow
            ? "移动端只读：可查看余额与流水。库存调整、列设置与全量导出请在桌面完成。"
            : "按仓库与 SKU 查看账面现存、有效预占与可用数量；追溯流水与销售预占。不可直接编辑库存或释放预占。"
        }
        breadcrumbs={[
          { id: "proc", label: "采购与履约", href: "/inventory" },
          { id: "inv", label: "库存台账", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt={formatDateTime(data.queriedAt, "full", "passthrough")}
            dateTime={data.queriedAt}
            state="fresh"
            label="库存记录更新时间"
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "refresh",
                label: "刷新",
                icon: RefreshCwIcon,
                variant: "outline",
                onClick: () => {
                  void listQuery.refetch()
                  if (previewBalanceId) void detailQuery.refetch()
                },
              },
              {
                actionKey: "export",
                label: "导出",
                icon: DownloadIcon,
                variant: "outline",
                mobileVisibility: "hide",
                disabled: !data.canExport || data.total === 0 || isPhoneNarrow,
                onClick: () => {
                  void exportMutation
                    .mutateAsync({
                      total: data.total,
                      filterSummary: data.filterSummary,
                    })
                    .then((job) => setExportJobId(job.jobId))
                },
              },
            ]}
          />
        }
      />

      {lastResult ? (
        <FormalActionResult
          status={
            lastResult.status === "succeeded"
              ? "succeeded"
              : lastResult.status === "unknown"
                ? "unknown"
                : "blocked"
          }
          title={lastResult.title}
          description={lastResult.description}
          reference={lastResult.reference}
          actions={
            lastResult.pendingIdempotencyKey ? (
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={resolveUnknownMutation.isPending}
                  onClick={() => {
                    void resolveUnknownMutation
                      .mutateAsync({
                        idempotencyKey: lastResult.pendingIdempotencyKey!,
                      })
                      .then((r) => {
                        if (r.status === "succeeded") {
                          setLastResult({
                            status: "succeeded",
                            title: "调整已提交待复核",
                            description: `单号 ${r.outcome.adjustmentNo}。下一责任方：${r.outcome.nextResponsible}。`,
                            reference: r.outcome.reference,
                          })
                          closeAdjustment()
                        } else if (r.status === "unknown") {
                          setLastResult({
                            status: "unknown",
                            title: "仍在查询最终结果",
                            description: r.message,
                            reference: r.idempotencyKey,
                            pendingIdempotencyKey: r.idempotencyKey,
                          })
                        } else {
                          setActionError(r.message)
                        }
                      })
                  }}
                >
                  查询最终结果
                </Button>
                <Button
                  type="button"
                  size="sm"
                  onClick={() => {
                    if (!pendingPayload) return
                    void resolveUnknownMutation
                      .mutateAsync({
                        idempotencyKey: lastResult.pendingIdempotencyKey!,
                        settle: true,
                        settlePayload: pendingPayload,
                      })
                      .then((r) => {
                        if (r.status === "succeeded") {
                          setLastResult({
                            status: "succeeded",
                            title: "调整已提交待复核",
                            description: `单号 ${r.outcome.adjustmentNo}。下一责任方：${r.outcome.nextResponsible}。`,
                            reference: r.outcome.reference,
                          })
                          closeAdjustment()
                        }
                      })
                  }}
                >
                  完成并确认结果（仅演示）
                </Button>
              </div>
            ) : undefined
          }
        />
      ) : null}

      {exportJobId && exportJob ? (
        <BackgroundJobProgress
          mode="all-or-nothing"
          status={
            exportJob.status === "queued"
              ? "queued"
              : exportJob.status === "running"
                ? "running"
                : exportJob.status === "succeeded"
                  ? "succeeded"
                  : "failed"
          }
          total={exportJob.total}
          completed={exportJob.completed}
          succeeded={
            exportJob.status === "succeeded" ? exportJob.total : undefined
          }
          label={`导出任务 ${exportJob.jobId}`}
          description={
            <>
              范围：{exportJob.filterSummary}。导出文件由系统生成，完成后可在此下载。
              {exportJob.downloadLabel ? (
                <span className="mt-1 block font-medium">
                  可下载：{exportJob.downloadLabel}
                </span>
              ) : null}
            </>
          }
          action={
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => setExportJobId(null)}
            >
              关闭
            </Button>
          }
        />
      ) : null}

      {actionError ? (
        <Alert variant="destructive">
          <AlertTitle>操作未完成</AlertTitle>
          <AlertDescription>{actionError}</AlertDescription>
        </Alert>
      ) : null}

      <Alert>
        <AlertTitle>自有实物库存边界</AlertTitle>
        <AlertDescription className="text-xs leading-relaxed">
          {data.excludedKindsNote}
          <span className="mt-1 block">{data.openingStockNote}</span>
        </AlertDescription>
      </Alert>

      <MetricStrip columns={4} aria-label="库存台账指标筛选">
        <MetricFilterItem
          label="库存组合"
          value={data.metrics.balanceDimensionCount}
          detail="仓库+SKU 组合数"
          active={metricActive === "combos" && view === "balance"}
          onClick={() => {
            patchUrl({
              view: "balance",
              availability: "all",
            })
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="有效预占组合"
          value={data.metrics.reservedDimensionCount}
          detail="有有效预占"
          active={metricActive === "reserved"}
          onClick={() => {
            patchUrl({
              view: "balance",
              availability: "reserved",
            })
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="零可用组合"
          value={data.metrics.zeroAvailableDimensionCount}
          detail="available = 0"
          active={metricActive === "zero"}
          onClick={() => {
            patchUrl({
              view: "balance",
              availability: "zero",
            })
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="待处理调整"
          value={data.metrics.pendingAdjustmentCount}
          detail="处理中"
          active={metricActive === "pending"}
          onClick={() => {
            patchUrl({
              view: "adjustment",
              availability: null,
            })
            resetPagination()
          }}
        />
      </MetricStrip>

      <Tabs
        value={view}
        onValueChange={(v) => {
          patchUrl({ view: v })
          resetPagination()
        }}
      >
        <TabsList variant="line" className="w-full justify-start">
          {(
            [
              "balance",
              "movement",
              "reservation",
              "adjustment",
            ] as const
          ).map((v) => (
            <TabsTrigger key={v} value={v}>
              {VIEW_LABEL[v]}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      <BusinessTableFrame
        title={VIEW_LABEL[view]}
        description={
          <span aria-live="polite">
            {data.filterSummary}
            {view === "balance" ? (
              <span className="text-muted-foreground">
                {" "}
                · 数量均带基础单位；可用数量以系统数据为准
              </span>
            ) : null}
          </span>
        }
        toolbar={
          <ListToolbar
            search={
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  ref={searchInputRef}
                  value={searchInput}
                  onChange={(e) => {
                    setSearchInput(e.target.value)
                    resetPagination()
                  }}
                  placeholder="SKU 编码、名称、规格、仓库"
                  aria-label="搜索库存"
                />
              </InputGroup>
            }
            filters={
              <>
                <label className="flex items-center gap-1.5 text-sm">
                  <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                    仓库
                  </span>
                  <WarehouseCombobox
                    className="w-44"
                    value={warehouseId || undefined}
                    onValueChange={(id) => {
                      patchUrl({
                        warehouseId: id || null,
                      })
                      resetPagination()
                    }}
                    warehouses={data.warehouses.map((w) => ({
                      warehouseId: w.id,
                      warehouseName: w.name,
                      warehouseCode: w.id,
                      statusLabel: "可选",
                      statusTone: "neutral",
                    }))}
                    aria-label="筛选仓库"
                    placeholder="全部仓库"
                  />
                </label>
                {view === "balance" ? (
                  <label className="flex items-center gap-1.5 text-sm">
                    <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                      可用状态
                    </span>
                    <OptionCombobox
                      className="w-28"
                      value={availability}
                      onValueChange={(v) => {
                        patchUrl({
                          availability: (v ??
                            "all") as InventoryAvailability,
                        })
                        resetPagination()
                      }}
                      options={(
                        ["all", "positive", "zero", "reserved"] as const
                      ).map((a) => ({
                        value: a,
                        label: AVAILABILITY_LABEL[a],
                      }))}
                      size="sm"
                      allowClear={false}
                      aria-label="筛选可用状态"
                      placeholder="可用状态"
                    />
                  </label>
                ) : null}
                {view === "movement" ? (
                  <>
                    <label className="flex items-center gap-1.5 text-sm">
                      <span className="sr-only">流水类型</span>
                      <OptionCombobox
                        className="w-32"
                        value={movementType[0] ?? "all"}
                        onValueChange={(value) => {
                          patchUrl({
                            movementType:
                              value && value !== "all" ? value : null,
                          })
                          resetPagination()
                        }}
                        options={[
                          { value: "all", label: "全部流水" },
                          ...MOVEMENT_TYPE_OPTIONS,
                        ]}
                        size="sm"
                        allowClear={false}
                        aria-label="筛选流水类型"
                        placeholder="全部流水"
                      />
                    </label>
                    <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                      发生日期
                      <Input
                        type="date"
                        className="h-8 w-32"
                        value={occurredFrom ?? ""}
                        max={occurredTo}
                        onChange={(event) => {
                          patchUrl({ occurredFrom: event.target.value })
                          resetPagination()
                        }}
                        aria-label="发生日期起"
                      />
                      <span>至</span>
                      <Input
                        type="date"
                        className="h-8 w-32"
                        value={occurredTo ?? ""}
                        min={occurredFrom}
                        onChange={(event) => {
                          patchUrl({ occurredTo: event.target.value })
                          resetPagination()
                        }}
                        aria-label="发生日期止"
                      />
                    </label>
                  </>
                ) : null}
                <label className="flex items-center gap-1.5 text-sm">
                  <span className="sr-only">排序</span>
                  <OptionCombobox
                    className="w-40"
                    value={sortValue}
                    onValueChange={(value) => {
                      patchUrl({ sort: value ?? defaultSortValue(view) })
                      resetPagination()
                    }}
                    options={sortOptions(view)}
                    size="sm"
                    allowClear={false}
                    aria-label="排序方式"
                    placeholder="排序"
                  />
                </label>
                {(qParam ||
                  warehouseId ||
                  (availability !== "all" && view === "balance") ||
                  skuId ||
                  movementType.length > 0 ||
                  searchParams.has("occurredFrom") ||
                  searchParams.has("occurredTo") ||
                  searchParams.has("sort")) && (
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      setSearchInput("")
                      patchUrl({
                        q: null,
                        warehouseId: null,
                        availability: "all",
                        skuId: null,
                        balanceId: null,
                        movementType: null,
                        occurredFrom: null,
                        occurredTo: null,
                        sort: null,
                      })
                      resetPagination()
                    }}
                  >
                    清除筛选
                  </Button>
                )}
              </>
            }
            actions={
              <span className="text-xs text-muted-foreground" aria-live="polite">
                共 {data.total.toLocaleString("zh-CN")} 条
              </span>
            }
          />
        }
        table={
          data.total === 0 ? (
            data.emptyReason === "FILTER_NO_RESULT" ? (
              <BusinessEmptyState
                kind="filter"
                title="当前筛选无结果"
                description={`没有符合「${data.filterSummary}」的记录。可清除筛选或切换视图。`}
                action={
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => {
                      setSearchInput("")
                      patchUrl({
                        q: null,
                        warehouseId: null,
                        availability: "all",
                        skuId: null,
                        view: "balance",
                      })
                    }}
                  >
                    清除筛选
                  </Button>
                }
              />
            ) : (
              <BusinessEmptyState
                kind="no-data"
                title="当前仓库尚无 ERP 自有库存记录"
                description="期初库存需在「导入与期初」完成导入后才会形成流水；商城旧库存不会自动显示在此。"
                action={
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    render={<Link href="/governance/imports" />}
                  >
                    前往导入与期初
                  </Button>
                }
              />
            )
          ) : view === "balance" ? (
            <DataTable
              data={pageRows as StockBalanceRow[]}
              columns={balanceColumns}
              getRowId={(row) => row.balanceId}
              rowCount={data.total}
              pagination={pagination}
              onPaginationChange={handlePaginationChange}
              layout="flush"
              density="compact"
              defaultColumnPinning={{
                left: ["identity"],
                right: ["actions"],
              }}
              onRowPreview={(row) => openDetail(row.balanceId)}
              onRowOpen={(row) => openDetail(row.balanceId)}
            />
          ) : view === "movement" ? (
            <DataTable
              data={pageRows as StockMovementRow[]}
              columns={movementColumns}
              getRowId={(row) => row.movementId}
              rowCount={data.total}
              pagination={pagination}
              onPaginationChange={handlePaginationChange}
              layout="flush"
              density="compact"
              defaultColumnPinning={{ left: ["identity"] }}
            />
          ) : view === "reservation" ? (
            <DataTable
              data={pageRows as StockReservationRow[]}
              columns={reservationColumns}
              getRowId={(row) => row.reservationId}
              rowCount={data.total}
              pagination={pagination}
              onPaginationChange={handlePaginationChange}
              layout="flush"
              density="compact"
              defaultColumnPinning={{
                left: ["identity"],
                right: ["actions"],
              }}
            />
          ) : (
            <DataTable
              data={pageRows as StockAdjustmentRow[]}
              columns={adjustmentColumns}
              getRowId={(row) => row.adjustmentId}
              rowCount={data.total}
              pagination={pagination}
              onPaginationChange={handlePaginationChange}
              layout="flush"
              density="compact"
              defaultColumnPinning={{ left: ["doc"] }}
            />
          )
        }
      />

      {/* 余额详情：最近流水、来源单据、有效预占 */}
      <QuickPreviewSheet
        open={previewBalanceId != null}
        onOpenChange={(open) => {
          if (!open) closeDetail()
        }}
        size="preview"
        title={
          detail
            ? `${detail.balance.skuName}`
            : "余额详情"
        }
        identity={
          detail ? (
            <span className="num text-sm">
              {detail.balance.warehouseCode} · {detail.balance.skuCode}
            </span>
          ) : null
        }
        summary={
          detail ? (
            <div className="flex flex-wrap items-center gap-2">
              <BusinessStatusBadge
                context="preview"
                label={detail.balance.statusLabel}
                tone={detail.balance.statusTone}
              />
              <Badge variant="secondary">自有实物</Badge>
            </div>
          ) : null
        }
        footer={
          detail ? (
            <>
              <Button type="button" variant="outline" onClick={closeDetail}>
                关闭
              </Button>
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  patchUrl({
                    view: "movement",
                    balanceId: detail.balance.balanceId,
                    warehouseId: detail.balance.warehouseId,
                    skuId: detail.balance.skuId,
                  })
                  closeDetail()
                }}
              >
                查看全部流水
              </Button>
              <Button
                type="button"
                disabled={
                  !detail.balance.allowedActions.includes("CREATE_ADJUSTMENT")
                }
                title={
                  detail.balance.actionBlockers.find(
                    (b) => b.action === "CREATE_ADJUSTMENT"
                  )?.message
                }
                onClick={() => void startAdjustment(detail.balance)}
              >
                发起库存调整
              </Button>
            </>
          ) : null
        }
      >
        {detailQuery.isPending ? (
          <div className="space-y-3 p-1">
            <div className="h-24 animate-pulse rounded-xl bg-muted" />
            <div className="h-40 animate-pulse rounded-xl bg-muted" />
          </div>
        ) : detail ? (
          <div className="flex flex-col gap-4">
            <div className="grid grid-cols-3 gap-2 rounded-xl border bg-card p-3">
              <div>
                <div className="text-xs text-muted-foreground">账面现存</div>
                <div className="num text-base font-semibold">
                  {detail.balance.onHandQuantity}
                  <span className="ml-1 text-xs font-normal text-muted-foreground">
                    {detail.balance.baseUnit}
                  </span>
                </div>
              </div>
              <div>
                <div className="text-xs text-muted-foreground">有效预占</div>
                <div className="num text-base font-semibold">
                  {detail.balance.reservedQuantity}
                  <span className="ml-1 text-xs font-normal text-muted-foreground">
                    {detail.balance.baseUnit}
                  </span>
                </div>
              </div>
              <div>
                <div className="text-xs text-muted-foreground">可用数量</div>
                <div className="num text-base font-semibold text-primary">
                  {detail.balance.availableQuantity}
                  <span className="ml-1 text-xs font-normal text-muted-foreground">
                    {detail.balance.baseUnit}
                  </span>
                </div>
                <div className="text-[10px] text-muted-foreground">
                  系统计算
                </div>
              </div>
            </div>

            <section className="space-y-2">
              <h3 className="text-sm font-medium">最近流水</h3>
              {detail.recentMovements.length === 0 ? (
                <p className="text-xs text-muted-foreground">暂无流水</p>
              ) : (
                <ul className="space-y-2">
                  {detail.recentMovements.map((m) => (
                    <li
                      key={m.movementId}
                      className="rounded-lg border px-3 py-2 text-sm"
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div>
                          <div className="font-medium">
                            {m.movementTypeLabel}
                            <span className="ml-2 text-xs text-muted-foreground">
                              {m.direction === "increase" ? "增加" : "减少"}
                            </span>
                          </div>
                          <div className="num text-xs text-muted-foreground">
                            {formatDateTime(m.occurredAt, "full", "passthrough")} · {m.recordedByLabel}
                          </div>
                        </div>
                        <div className="num shrink-0 font-medium">
                          {m.quantity} {m.baseUnit}
                        </div>
                      </div>
                      {m.sourceHref ? (
                        <Button
                          type="button"
                          variant="link"
                          size="xs"
                          className="mt-1 h-auto px-0"
                          render={<Link href={m.sourceHref} />}
                        >
                          来源 {m.sourceDocumentNo}
                        </Button>
                      ) : (
                        <div className="num mt-1 text-xs text-muted-foreground">
                          来源 {m.sourceDocumentNo}
                        </div>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </section>

            <Separator />

            <section className="space-y-2">
              <h3 className="text-sm font-medium">来源单据</h3>
              {detail.sourceDocuments.length === 0 ? (
                <p className="text-xs text-muted-foreground">无关联来源</p>
              ) : (
                <ul className="space-y-1.5">
                  {detail.sourceDocuments.map((doc) => (
                    <li
                      key={`${doc.documentType}:${doc.documentId}`}
                      className="flex items-center justify-between gap-2 text-sm"
                    >
                      <span>
                        {doc.label}
                        <span className="num ml-2 text-muted-foreground">
                          {doc.documentNo}
                        </span>
                      </span>
                      {doc.href ? (
                        <Button
                          type="button"
                          variant="outline"
                          size="xs"
                          render={<Link href={doc.href} />}
                        >
                          {doc.workspaceId ?? "打开"}
                        </Button>
                      ) : null}
                    </li>
                  ))}
                </ul>
              )}
            </section>

            <Separator />

            <section className="space-y-2">
              <h3 className="text-sm font-medium">有效预占</h3>
              {detail.reservations.length === 0 ? (
                <p className="text-xs text-muted-foreground">无有效预占</p>
              ) : (
                <ul className="space-y-2">
                  {detail.reservations.map((r) => (
                    <li
                      key={r.reservationId}
                      className="rounded-lg border px-3 py-2 text-sm"
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div>
                          <div className="num font-medium">{r.salesOrderNo}</div>
                          <div className="text-xs text-muted-foreground">
                            {r.salesOrderLineLabel}
                          </div>
                          <div className="mt-1 text-xs">
                            剩余{" "}
                            <span className="num">
                              {r.remainingQuantity} {r.baseUnit}
                            </span>
                            {" · "}
                            建立 {r.establishedQuantity} · 消耗{" "}
                            {r.consumedQuantity}
                          </div>
                        </div>
                        <BusinessStatusBadge
                          context="list"
                          label={r.statusLabel}
                          tone={r.statusTone}
                        />
                      </div>
                      {r.fulfillmentHref ? (
                        <Button
                          type="button"
                          variant="link"
                          size="xs"
                          className="mt-1 h-auto px-0"
                          render={<Link href={r.fulfillmentHref} />}
                        >
                          打开收货与发货
                        </Button>
                      ) : null}
                      {/* 无「释放预占」入口 */}
                    </li>
                  ))}
                </ul>
              )}
            </section>

            {detail.pendingAdjustments.length > 0 ? (
              <>
                <Separator />
                <section className="space-y-2">
                  <h3 className="text-sm font-medium">进行中的调整</h3>
                  <ul className="space-y-1 text-sm">
                    {detail.pendingAdjustments.map((a) => (
                      <li key={a.adjustmentId} className="flex justify-between">
                        <span className="num">{a.adjustmentNo}</span>
                        <BusinessStatusBadge
                          context="list"
                          label={a.statusLabel}
                          tone={a.statusTone}
                        />
                      </li>
                    ))}
                  </ul>
                </section>
              </>
            ) : null}

            <p className="text-[11px] leading-relaxed text-muted-foreground">
              查询于{" "}
              {formatDateTime(detail.queriedAt, "full", "passthrough")}
              。页面不提供编辑库存数量或直接释放预占；纠错须走调整单。
            </p>
          </div>
        ) : (
          <BusinessFailureState
            kind="business"
            title="无法加载余额详情"
            description="余额可能已不存在，或权限已变化。"
          />
        )}
      </QuickPreviewSheet>

      {/* 调整工作区：已生效单据，非余额编辑器 */}
      <Dialog
        open={adjustDraftId != null}
        onOpenChange={(open) => {
          if (!open) closeAdjustment()
        }}
      >
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>发起库存调整</DialogTitle>
            <DialogDescription>
              从当前余额上下文创建调整单草稿。提交后进入仓储复核，不会立即改库存。
            </DialogDescription>
          </DialogHeader>

          {adjustMeta ? (
            <div className="space-y-4">
              <div className="rounded-xl border bg-muted/40 p-3 text-sm">
                <div className="font-medium">
                  {adjustMeta.warehouseName}
                  <span className="num ml-2 text-muted-foreground">
                    {adjustMeta.skuCode}
                  </span>
                </div>
                <div>{adjustMeta.skuName}</div>
                <div className="mt-2 grid grid-cols-2 gap-2 text-xs text-muted-foreground">
                  <div>
                    账面现存{" "}
                    <span className="num text-foreground">
                      {adjustMeta.onHand} {adjustMeta.baseUnit}
                    </span>
                  </div>
                  <div>
                    可用{" "}
                    <span className="num text-foreground">
                      {adjustMeta.available} {adjustMeta.baseUnit}
                    </span>
                  </div>
                  <div>
                    草稿号{" "}
                    <span className="num text-foreground">
                      {adjustMeta.adjustmentNo}
                    </span>
                  </div>
                  <div>
                    余额版本{" "}
                    <span className="num text-foreground">
                      {adjustLockVersion}
                    </span>
                  </div>
                </div>
              </div>

              <Alert>
                <SlashIcon className="size-4" aria-hidden />
                <AlertTitle>岗位分离</AlertTitle>
                <AlertDescription className="text-xs">
                  {adjustMeta.segregationNote}
                </AlertDescription>
              </Alert>

              <form
                className="space-y-3"
                onSubmit={(e) => {
                  e.preventDefault()
                  void form.handleSubmit()
                }}
              >
                <div className="grid gap-1.5">
                  <Label htmlFor="reasonType">原因类型</Label>
                  <form.AppField
                    name="reasonType"
                    children={(field) => (
                      <OptionCombobox
                        id="reasonType"
                        value={field.state.value}
                        onValueChange={(v) => {
                          field.handleChange(
                            (v ?? field.state.value) as AdjustmentReasonType
                          )
                        }}
                        options={REASON_TYPE_OPTIONS.map((opt) => ({
                          value: opt.value,
                          label: `${opt.label}（${
                            opt.direction === "increase" ? "增加" : "减少"
                          }）`,
                        }))}
                        className="w-full"
                        allowClear={false}
                        aria-label="原因类型"
                        placeholder="原因类型"
                      />
                    )}
                  />
                </div>

                <form.AppField
                  name="quantity"
                  children={(field) => (
                    <field.TextField
                      label={`调整数量（${adjustMeta.baseUnit}，正数）`}
                    />
                  )}
                />

                <form.AppField
                  name="occurredAt"
                  children={(field) => (
                    <field.TextField label="业务发生时间" />
                  )}
                />

                <form.AppField
                  name="note"
                  children={(field) => (
                    <field.TextareaField label="原因说明" />
                  )}
                />

                <div className="rounded-lg border bg-card p-3 text-xs text-muted-foreground space-y-1">
                  <div className="font-medium text-foreground">提交约束</div>
                  <ul className="list-disc pl-4 space-y-0.5">
                    <li>不会直接修改账面或可用数量</li>
                    <li>经办与复核岗位分离，提交后待仓储复核</li>
                    <li>
                      按当前余额版本提交；若已被他人修改，将提示冲突并保留你的输入。
                    </li>
                  </ul>
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  <label className="flex items-center gap-2 text-xs text-muted-foreground">
                    <input
                      type="checkbox"
                      checked={forceUnknownOnce}
                      onChange={(e) => setForceUnknownOnce(e.target.checked)}
                    />
                    演示：强制结果不确定（仅演示）
                  </label>
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      if (!adjustBalanceId) return
                      bumpInventoryBalanceLock(
                        adjustBalanceId,
                        adjustSeedLock
                      )
                      setActionError(
                        `已模拟他人同时修改库存，本次提交将发生冲突（仅演示）。`
                      )
                    }}
                  >
                    演示：模拟余额并发变更（仅演示）
                  </Button>
                </div>

                <DialogFooter className="gap-2 sm:justify-between">
                  <Button
                    type="button"
                    variant="outline"
                    onClick={closeAdjustment}
                  >
                    取消
                  </Button>
                  <form.AppForm>
                    <form.SubmitButton label="提交待复核" />
                  </form.AppForm>
                </DialogFooter>
              </form>
            </div>
          ) : null}
        </DialogContent>
      </Dialog>

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        actionLabel="提交库存调整"
        confirmLabel="确认提交"
        fromStatus={{ label: "草稿", tone: "neutral" }}
        toStatus={{ label: "待仓储复核", tone: "warning" }}
        description="确认后形成调整单，进入仓储复核队列。余额在确认入账前不会变化。"
        lockedFields={[
          adjustMeta
            ? `${adjustMeta.warehouseName} / ${adjustMeta.skuCode}`
            : "当前余额",
          `余额版本 ${adjustLockVersion}`,
        ]}
        effects={[
          "创建待仓储复核的库存调整单",
          "不立即修改 on_hand / reserved / available",
          "经办人不得自行复核或确认入账",
        ]}
        nextDepartment="仓储复核"
        irreversibleEffects={["形成调整单号并进入连续队列"]}
        pending={submitMutation.isPending}
        onConfirm={() => void doSubmit()}
      />
    </div>
  )
}
