"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  DownloadIcon,
  FileTextIcon,
  PlusIcon,
  RefreshCwIcon,
  SearchIcon,
  WalletIcon,
} from "lucide-react"

import {
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FilterChip,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  MoneyValue,
  OptionCombobox,
  PageActions,
  PageHeader,
  PageScaffold,
  QuickPreviewSheet,
  SettlementPartyCombobox,
} from "@/components/business"
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
import { Textarea } from "@/components/ui/textarea"
import { AllocationSessionPanel } from "@/features/customer-receivables/allocation-session-panel"
import {
  useAllocationSessionQuery,
  useCreateAllocationSessionMutation,
  useCustomerAccountsDetailQuery,
  useCustomerAccountsListQuery,
  useReverseFactMutation,
} from "@/features/customer-receivables/queries"
import type {
  AllocationMode,
  CounterpartyOption,
  CustomerAccountsListView,
  CustomerAccountsQuery,
  CustomerAccountsView,
  DueFilter,
  ReceiptRow,
  ReceivableAccountRow,
  SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import { DUE_LABEL, VIEW_LABEL } from "@/features/customer-receivables/types"
import { freshnessText } from "@/lib/ui-text"

function parseView(raw: string | null): CustomerAccountsView {
  if (
    raw === "receipt" ||
    raw === "sales_invoice" ||
    raw === "unallocated" ||
    raw === "receivable"
  ) {
    return raw
  }
  return "receivable"
}

function parseDue(raw: string | null): DueFilter | undefined {
  if (
    raw === "not_due" ||
    raw === "due_today" ||
    raw === "overdue" ||
    raw === "all"
  ) {
    return raw
  }
  return undefined
}

type PreviewKind = "receivable" | "receipt" | "invoice"
type PreviewState = { kind: PreviewKind; id: string } | null

export function CustomerReceivablesPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const view = parseView(searchParams.get("view"))
  const qParam = searchParams.get("q") ?? ""
  const counterpartyPartyId =
    searchParams.get("counterpartyId") ?? undefined
  const customerId = searchParams.get("customerId") ?? undefined
  const due = parseDue(searchParams.get("due"))
  const status = searchParams.get("status") ?? undefined
  const reviewStatus = searchParams.get("reviewStatus") ?? undefined
  const focusId = searchParams.get("focusId") ?? undefined
  const salesOrderId = searchParams.get("salesOrderId") ?? undefined
  const receivableAccountId =
    searchParams.get("receivableAccountId") ?? undefined
  const returnTo = searchParams.get("returnTo") ?? undefined
  const from = searchParams.get("from") ?? undefined
  const sessionId = searchParams.get("sessionId") ?? undefined
  const previewKind = searchParams.get("previewKind") as PreviewKind | null
  const previewId = searchParams.get("previewId") ?? undefined

  const [searchInput, setSearchInput] = React.useState(qParam)
  const searchInputRef = React.useRef<HTMLInputElement | null>(null)
  const [preview, setPreview] = React.useState<PreviewState>(() =>
    previewKind && previewId
      ? { kind: previewKind, id: previewId }
      : focusId
        ? { kind: "receivable", id: focusId }
        : null
  )
  const [partyPickerOpen, setPartyPickerOpen] = React.useState(false)
  const [partyPickerMode, setPartyPickerMode] =
    React.useState<AllocationMode>("receipt")
  const [selectedPartyId, setSelectedPartyId] = React.useState("")
  const [lastResult, setLastResult] = React.useState<ResultState>(null)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [reverseConfirm, setReverseConfirm] = React.useState<{
    kind: "receipt_reverse" | "refund" | "red_invoice"
    sourceFactId: string
    label: string
    amount?: string
  } | null>(null)
  const [reverseReason, setReverseReason] = React.useState("")
  const [reverseAmount, setReverseAmount] = React.useState("")

  const query: CustomerAccountsQuery = React.useMemo(
    () => ({
      view,
      q: qParam || undefined,
      counterpartyPartyId,
      customerId,
      due,
      status,
      reviewStatus,
      salesOrderId,
      receivableAccountId,
      returnTo,
      from,
    }),
    [
      view,
      qParam,
      counterpartyPartyId,
      customerId,
      due,
      status,
      reviewStatus,
      salesOrderId,
      receivableAccountId,
      returnTo,
      from,
    ]
  )

  // 分页从 URL 派生（P6）；筛选变更写 URL 并回第 1 页。
  const pageFromUrl = React.useMemo(
    () => Math.max(1, Number.parseInt(searchParams.get("page") ?? "1", 10) || 1),
    [searchParams]
  )
  const pagination = React.useMemo<PaginationState>(
    () => ({ pageIndex: Math.max(0, pageFromUrl - 1), pageSize: 20 }),
    [pageFromUrl]
  )

  const listQuery = useCustomerAccountsListQuery(query)
  const detailQuery = useCustomerAccountsDetailQuery(
    preview?.kind ?? null,
    preview?.id ?? null
  )
  const sessionQuery = useAllocationSessionQuery(sessionId ?? null)
  const createSession = useCreateAllocationSessionMutation()
  const reverseMutation = useReverseFactMutation()

  const data = listQuery.data

  function patchUrl(
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean }
  ) {
    patchSearchParams({ router, pathname, searchParams, view }, patch, options)
  }

  /** 客户锁定（customerId）显性化为可移除 chip。 */
  const lockedCustomerName = React.useMemo(
    () =>
      (data?.counterparties ?? []).find((c) => c.customerId === customerId)
        ?.customerName,
    [data?.counterparties, customerId]
  )

  const hasActiveFilters = Boolean(
    qParam.trim() ||
      counterpartyPartyId ||
      customerId ||
      due ||
      status ||
      reviewStatus ||
      salesOrderId ||
      receivableAccountId
  )

  /** P4：清全部筛选参数 + 分页回 1；保留 view/导航上下文。 */
  const clearFilters = React.useCallback(() => {
    setSearchInput("")
    patchUrl(
      {
        q: null,
        counterpartyId: null,
        customerId: null,
        due: null,
        status: null,
        reviewStatus: null,
        salesOrderId: null,
        receivableAccountId: null,
        focusId: null,
        previewKind: null,
        previewId: null,
        page: null,
      },
      { replace: true }
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, pathname, view])

  const handlePaginationChange = React.useCallback(
    (next: PaginationState) => {
      patchUrl(
        {
          page:
            next.pageIndex + 1 > 1 ? String(next.pageIndex + 1) : null,
        },
        { replace: true }
      )
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [searchParams, pathname, view]
  )

  React.useEffect(() => {
    setSearchInput(qParam)
  }, [qParam])

  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (searchInput === qParam) return
      patchUrl({ q: searchInput.trim() || null, page: null }, { replace: true })
    }, 300)
    return () => globalThis.clearTimeout(handle)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchInput])

  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
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
      searchInputRef.current?.focus()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  // W05 链入：自动打开核销会话（有 salesOrderId + counterparty 或可推断）
  const autoSessionRef = React.useRef(false)
  React.useEffect(() => {
    if (autoSessionRef.current || sessionId || !data) return
    if (from !== "W05" || !returnTo) return
    if (!data.canRegister) return
    const party =
      counterpartyPartyId ??
      data.receivables[0]?.counterpartyPartyId ??
      data.counterparties.find((c) => c.customerId === customerId)
        ?.counterpartyPartyId
    if (!party) return
    autoSessionRef.current = true
    void (async () => {
      try {
        const session = await createSession.mutateAsync({
          mode: "receipt",
          counterpartyPartyId: party,
          salesOrderId,
          receivableAccountId,
          returnTo,
          from,
        })
        patchUrl({ sessionId: session.draftSessionId }, { replace: true })
      } catch (err) {
        setActionError(
          err instanceof Error ? err.message : "无法开始本次核销"
        )
      }
    })()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data, from, returnTo, sessionId])

  const openPreview = React.useCallback(
    (next: PreviewState) => {
      setPreview(next)
      if (next) {
        // 打开/关闭详情用 push（P2）；旧 focusId 一并清理
        patchUrl(
          { previewKind: next.kind, previewId: next.id, focusId: null },
          { replace: false }
        )
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [searchParams, pathname, view]
  )

  const closePreview = React.useCallback(() => {
    setPreview(null)
    patchUrl(
      {
        previewKind: null,
        previewId: null,
        focusId: null,
      },
      { replace: false }
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, pathname, view])

  async function startSession(
    mode: AllocationMode,
    partyId: string,
    existingFactId?: string,
    target?: { salesOrderId?: string; receivableAccountId?: string }
  ) {
    setActionError(null)
    setLastResult(null)
    try {
      const session = await createSession.mutateAsync({
        mode,
        counterpartyPartyId: partyId,
        existingFactId,
        salesOrderId: target?.salesOrderId ?? salesOrderId,
        receivableAccountId:
          target?.receivableAccountId ?? receivableAccountId,
        returnTo,
        from,
      })
      setPartyPickerOpen(false)
      patchUrl({
        sessionId: session.draftSessionId,
        counterpartyId: partyId,
      })
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "创建本次核销失败")
    }
  }

  function openRegister(mode: AllocationMode) {
    setPartyPickerMode(mode)
    setSelectedPartyId(counterpartyPartyId ?? "")
    setPartyPickerOpen(true)
  }

  async function confirmReverse() {
    if (!reverseConfirm) return
    const key = `w11-rev-${reverseConfirm.sourceFactId}-${Date.now()}`
    const res = await reverseMutation.mutateAsync({
      kind: reverseConfirm.kind,
      sourceFactId: reverseConfirm.sourceFactId,
      amount:
        reverseConfirm.kind === "red_invoice" ? reverseAmount : undefined,
      reason: reverseReason || "纠错",
      idempotencyKey: key,
    })
    if (res.status === "succeeded") {
      setLastResult({
        status: "succeeded",
        title: "反向记录已追加",
        description: res.message,
        reference: res.operationId,
        facts: [
          { label: "反向单号", value: res.reverseFactNo },
          { label: "原记录", value: reverseConfirm.label },
        ],
      })
      setReverseConfirm(null)
      setReverseReason("")
      setReverseAmount("")
      closePreview()
      return
    }
    if (res.status === "unknown") {
      setLastResult({
        status: "unknown",
        title: "纠错结果不确定",
        description: res.message,
        reference: res.idempotencyKey,
      })
      setReverseConfirm(null)
      return
    }
    setActionError(res.message)
    setReverseConfirm(null)
  }

  const receivableColumns = React.useMemo<ColumnDef<ReceivableAccountRow>[]>(
    () => [
      {
        id: "party",
        header: "往来主体 / 客户",
        meta: { label: "往来主体", width: "reference" },
        cell: ({ row }) => (
          <div className="flex min-w-0 items-center gap-1.5">
            <span className="truncate text-sm font-medium">
              {row.original.counterpartyPartyName}
            </span>
            <span className="shrink-0 text-muted-foreground">·</span>
            <span className="truncate text-xs text-muted-foreground">
              {row.original.customerName}
            </span>
          </div>
        ),
      },
      {
        id: "order",
        header: "销售单 / 子账",
        meta: { label: "销售单", width: "reference" },
        cell: ({ row }) => (
          <div className="flex items-center gap-1.5">
            <span className="num text-sm">{row.original.salesOrderNo}</span>
            <span className="text-xs text-muted-foreground">
              子账 #{row.original.accountSeq} · {row.original.businessTypeLabel}
            </span>
          </div>
        ),
      },
      {
        id: "open",
        header: "开放应收（含税）",
        meta: { label: "开放应收", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <MoneyValue value={row.original.openTotal} />
        ),
      },
      {
        id: "settled",
        header: "已核销回款（含税）",
        meta: { label: "已核销", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <MoneyValue value={row.original.settledTotal} />
        ),
      },
      {
        id: "invoice",
        header: "净已开票 / 可开票（含税）",
        meta: { label: "开票", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <div className="flex items-center justify-end gap-1.5 text-right">
            <MoneyValue value={row.original.invoicedTotal} />
            <span className="text-xs text-muted-foreground">/ 可开</span>
            <MoneyValue value={row.original.openInvoiceableTotal} />
          </div>
        ),
      },
      {
        id: "due",
        header: "到期",
        meta: { label: "到期" },
        cell: ({ row }) => (
          <div className="flex items-center gap-1.5">
            <span className="num text-sm">{row.original.dueDate}</span>
            <BusinessStatusBadge
              context="list"
              label={row.original.dueStateLabel}
              tone={
                row.original.dueState === "overdue"
                  ? "destructive"
                  : row.original.dueState === "due_today"
                    ? "warning"
                    : "neutral"
              }
            />
          </div>
        ),
      },
      {
        id: "status",
        header: "状态",
        meta: { label: "状态" },
        cell: ({ row }) => (
          <div className="flex items-center gap-1.5">
            <BusinessStatusBadge
              context="list"
              label={row.original.statusLabel}
              tone={row.original.statusTone}
            />
            {row.original.reviewStatus !== "na" ? (
              <span className="text-xs text-muted-foreground">
                {row.original.reviewStatusLabel}
              </span>
            ) : null}
          </div>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default" },
        cell: ({ row }) => (
          <div className="flex flex-nowrap justify-end gap-1">
            <Button
              type="button"
              size="xs"
              variant="ghost"
              onClick={() =>
                openPreview({ kind: "receivable", id: row.original.accountId })
              }
            >
              预览
            </Button>
            <Button
              type="button"
              size="xs"
              variant="outline"
              disabled={!row.original.allowedActions.includes("REGISTER_RECEIPT")}
              title={
                row.original.allowedActions.includes("REGISTER_RECEIPT")
                  ? undefined
                  : "当前无回款登记/核销权限"
              }
              onClick={() =>
                void startSession(
                  "receipt",
                  row.original.counterpartyPartyId,
                  undefined,
                  {
                    salesOrderId: row.original.salesOrderId,
                    receivableAccountId: row.original.accountId,
                  }
                )
              }
            >
              核销
            </Button>
          </div>
        ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [data?.canRegister]
  )

  const receiptColumns = React.useMemo<ColumnDef<ReceiptRow>[]>(
    () => [
      {
        id: "doc",
        header: "回款单号",
        meta: { label: "回款单号", width: "reference" },
        cell: ({ row }) => (
          <div>
            <div className="num text-sm font-medium">
              {row.original.receiptNo}
            </div>
            <div className="truncate text-xs text-muted-foreground">
              {row.original.counterpartyPartyName}
            </div>
          </div>
        ),
      },
      {
        id: "receivedAt",
        header: "到账时间",
        cell: ({ row }) => (
          <span className="num text-sm">
            {formatDateTime(row.original.receivedAt, "full", "passthrough")}
          </span>
        ),
      },
      {
        id: "amount",
        header: "到账金额",
        meta: { label: "金额", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <MoneyValue value={row.original.amount} taxBasis="gross" />
        ),
      },
      {
        id: "alloc",
        header: "净已分配 / 未分配",
        meta: { label: "分配", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <div className="text-right">
            <MoneyValue value={row.original.allocatedTotal} />
            <div className="text-xs text-muted-foreground">
              未分配{" "}
              <MoneyValue value={row.original.unallocatedAmount} />
            </div>
          </div>
        ),
      },
      {
        id: "status",
        header: "状态",
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default" },
        cell: ({ row }) => (
          <div className="flex flex-wrap justify-end gap-1">
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() =>
                openPreview({ kind: "receipt", id: row.original.receiptId })
              }
            >
              预览
            </Button>
            {row.original.allowedActions.includes("CONTINUE_ALLOCATE") ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  void startSession(
                    "receipt",
                    row.original.counterpartyPartyId,
                    row.original.receiptId
                  )
                }
              >
                继续核销
              </Button>
            ) : null}
          </div>
        ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    []
  )

  const invoiceColumns = React.useMemo<ColumnDef<SalesInvoiceRow>[]>(
    () => [
      {
        id: "doc",
        header: "发票",
        meta: { label: "发票", width: "reference" },
        cell: ({ row }) => (
          <div>
            <div className="flex items-center gap-2">
              <span className="num text-sm font-medium">
                {row.original.invoiceNo}
              </span>
              <Badge
                variant={
                  row.original.invoiceKind === "red" ? "warning" : "secondary"
                }
              >
                {row.original.invoiceKindLabel}
              </Badge>
            </div>
            <div className="truncate text-xs text-muted-foreground">
              {row.original.invoiceCode
                ? `代码 ${row.original.invoiceCode} · `
                : ""}
              {row.original.counterpartyPartyName}
            </div>
          </div>
        ),
      },
      {
        id: "date",
        header: "开票日期",
        cell: ({ row }) => (
          <span className="num text-sm">{row.original.invoiceDate}</span>
        ),
      },
      {
        id: "gross",
        header: "含税金额",
        meta: { label: "含税", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <MoneyValue value={row.original.grossAmount} taxBasis="gross" />
        ),
      },
      {
        id: "alloc",
        header: "净已分配 / 未分配",
        meta: { label: "分配", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <div className="text-right">
            <MoneyValue value={row.original.allocatedTotal} />
            <div className="text-xs text-muted-foreground">
              未分配{" "}
              <MoneyValue value={row.original.unallocatedAmount} />
            </div>
          </div>
        ),
      },
      {
        id: "status",
        header: "状态",
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default" },
        cell: ({ row }) => (
          <div className="flex flex-wrap justify-end gap-1">
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() =>
                openPreview({ kind: "invoice", id: row.original.invoiceId })
              }
            >
              预览
            </Button>
            {row.original.allowedActions.includes("CONTINUE_ALLOCATE") ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  void startSession(
                    "invoice",
                    row.original.counterpartyPartyId,
                    row.original.invoiceId
                  )
                }
              >
                继续分配
              </Button>
            ) : null}
          </div>
        ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    []
  )

  // 核销会话全屏
  if (sessionId) {
    if (sessionQuery.isPending) {
      return (
        <PageScaffold>
          <div className="h-10 w-64 animate-pulse rounded-lg bg-muted" />
          <div className="h-96 animate-pulse rounded-lg bg-muted" />
        </PageScaffold>
      )
    }
    if (!sessionQuery.data) {
      return (
        <PageScaffold>
          <BusinessFailureState
            kind="business"
            title="本次核销无效"
            description="本次核销已失效，请重新开始。"
            action={
              <Button
                type="button"
                onClick={() => patchUrl({ sessionId: null })}
              >
                返回列表
              </Button>
            }
          />
        </PageScaffold>
      )
    }
    return (
      <PageScaffold>
        <AllocationSessionPanel
          session={sessionQuery.data}
          onClose={() => {
            const ret = sessionQuery.data?.returnContext
            if (ret?.returnTo && ret.from === "W05") {
              router.push(ret.returnTo)
              return
            }
            patchUrl({ sessionId: null })
          }}
          onPosted={() => {
            void listQuery.refetch()
          }}
        />
      </PageScaffold>
    )
  }

  if (listQuery.isError) {
    return (
      <PageScaffold>
        <BusinessFailureState
          kind="system"
          title="客户往来加载失败"
          description="请重试；失败时不展示任何结清或金额结论。"
          action={
            <Button type="button" onClick={() => void listQuery.refetch()}>
              重试
            </Button>
          }
        />
      </PageScaffold>
    )
  }

  const metrics = data?.metrics
  const counterparties: readonly CounterpartyOption[] =
    data?.counterparties ?? []

  return (
    <PageScaffold density="compact">
      <PageHeader
        title="客户往来"
        breadcrumbs={[
          { id: "fin", label: "财务", href: "/finance/customer-accounts" },
          { id: "ar", label: "客户往来", current: true },
        ]}
        metadata={
          data ? (
            <DataFreshness
              updatedAt={freshnessText.dataUpdatedAt}
              dateTime={data.queriedAt}
              state="fresh"
              label="客户往来"
            />
          ) : null
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "export",
                label: "导出",
                icon: DownloadIcon,
                variant: "outline",
                mobileVisibility: "hide",
                disabled: !data?.canExport || data.total === 0,
                onClick: () => {
                  if (!data) return
                  const fileName = `客户往来-${VIEW_LABEL[data.view]}-${new Date().toISOString().slice(0, 10)}.csv`
                  downloadCsv(fileName, buildAccountsCsv(data))
                  setLastResult({
                    status: "succeeded",
                    title: "导出已完成",
                    description: `已按当前筛选生成 CSV 文件 ${fileName}，并开始下载。`,
                  })
                },
              },
              {
                actionKey: "register-invoice",
                label: "登记销项发票",
                icon: FileTextIcon,
                variant: "outline",
                mobileVisibility: "hide",
                disabled: !data?.canRegister,
                title: data?.canRegister
                  ? undefined
                  : "当前无销项发票登记权限",
                onClick: () => openRegister("invoice"),
              },
              {
                actionKey: "register-receipt",
                label: "登记回款",
                icon: WalletIcon,
                mobileVisibility: "hide",
                disabled: !data?.canRegister,
                title: data?.canRegister ? undefined : "当前无回款登记权限",
                onClick: () => openRegister("receipt"),
              },
            ]}
          />
        }
      />

      {from === "W05" && returnTo ? (
        <Alert variant="info">
          <AlertTitle>销售单票款入口</AlertTitle>
          <AlertDescription className="flex flex-wrap items-center gap-2">
            已携带来源页签返回上下文
            {salesOrderId ? (
              ` · 销售单 ${
                data?.receivables.find(
                  (r) => r.salesOrderId === salesOrderId
                )?.salesOrderNo ?? ""
              }`
            ) : (
              ""
            )}
            。核销完成后可回到销售单。
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={<Link href={returnTo} />}
            >
              返回销售单
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {lastResult ? (
        <FormalActionResult
          status={lastResult.status === "failed" ? "blocked" : lastResult.status}
          title={lastResult.title}
          description={lastResult.description}
          reference={lastResult.reference}
          facts={lastResult.facts}
        />
      ) : null}

      {actionError ? (
        <Alert variant="destructive">
          <AlertTitle>操作未成功</AlertTitle>
          <AlertDescription>{actionError}</AlertDescription>
        </Alert>
      ) : null}

      {data && !data.moduleAllowed ? (
        <BusinessFailureState
          kind="permission"
          description="无客户往来模块权限或权限已收回。"
        />
      ) : data && !data.hasDataScope ? (
        <BusinessEmptyState
          kind="no-scope"
          title="当前角色未配置客户往来范围"
          description="不得用 0 元假装无应收。请申请财务数据范围。"
        />
      ) : (
        <>
          {metrics ? (
            <MetricStrip columns={4} aria-label="客户往来指标">
              <MetricFilterItem
                label="开放应收"
                value={<MoneyValue value={metrics.openReceivableTotal} />}
                detail={
                  data
                    ? `更新 ${formatDateTime(data.queriedAt, "monthDayIntl")}`
                    : undefined
                }
                active={view === "receivable"}
                onClick={() => {
                  // 其余指标点击只设 view（P7），回第 1 页
                  patchUrl({ view: "receivable", page: null }, { replace: true })
                }}
              />
              <MetricFilterItem
                label="已逾期应收"
                value={<MoneyValue value={metrics.overdueReceivableTotal} />}
                detail="需催收"
                active={view === "receivable" && due === "overdue"}
                onClick={() => {
                  // view+filter 双重语义；与状态/复核维度重叠时一并重置避免矛盾空结果
                  patchUrl(
                    {
                      view: "receivable",
                      due: "overdue",
                      status: null,
                      reviewStatus: null,
                      page: null,
                    },
                    { replace: true }
                  )
                }}
              />
              <MetricFilterItem
                label="待分配回款"
                value={<MoneyValue value={metrics.unallocatedReceiptTotal} />}
                detail="已到账"
                active={view === "unallocated"}
                onClick={() => {
                  patchUrl(
                    {
                      view: "unallocated",
                      due: null,
                      status: null,
                      reviewStatus: null,
                      page: null,
                    },
                    { replace: true }
                  )
                }}
              />
              <MetricFilterItem
                label="待分配销项发票"
                value={<MoneyValue value={metrics.unallocatedInvoiceTotal} />}
                detail={
                  metrics.cardPendingReviewCount > 0
                    ? `卡券待复核 ${metrics.cardPendingReviewCount}`
                    : "独立轨道"
                }
                active={view === "sales_invoice"}
                onClick={() => {
                  patchUrl(
                    {
                      view: "sales_invoice",
                      due: null,
                      status: null,
                      reviewStatus: null,
                      page: null,
                    },
                    { replace: true }
                  )
                }}
              />
            </MetricStrip>
          ) : (
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
              {Array.from({ length: 4 }).map((_, i) => (
                <div
                  key={i}
                  className="h-20 animate-pulse rounded-lg bg-muted"
                />
              ))}
            </div>
          )}

          <Tabs
            value={view}
            onValueChange={(v) => {
              // 非 receivable 视图隐藏 due/status/reviewStatus，切视图时清除残留
              const patch: Record<string, string | null | undefined> = {
                view: v,
                page: null,
              }
              if (v !== "receivable") {
                patch.due = null
                patch.status = null
                patch.reviewStatus = null
              }
              patchUrl(patch, { replace: true })
            }}
          >
            <TabsList>
              {(
                [
                  "receivable",
                  "receipt",
                  "sales_invoice",
                  "unallocated",
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
                {data?.filterSummary ?? "加载中…"}
                {data ? (
                  <span className="text-muted-foreground">
                    {" "}
                    · 提交方式：{data.submitPolicy.label}
                  </span>
                ) : null}
              </span>
            }
            toolbar={
              <ListToolbar
                search={
                  <InputGroup className="max-w-sm">
                    <InputGroupAddon>
                      <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                      ref={searchInputRef}
                      placeholder="往来主体、销售单、回款单、发票号"
                      value={searchInput}
                      onChange={(e) => setSearchInput(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          patchUrl(
                            { q: searchInput.trim() || null, page: null },
                            { replace: true }
                          )
                        }
                      }}
                      aria-label="搜索客户往来"
                    />
                  </InputGroup>
                }
                filters={
                  <>
                    <label className="flex items-center gap-1.5 text-sm">
                      <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                        往来主体
                      </span>
                      <SettlementPartyCombobox
                        value={counterpartyPartyId || undefined}
                        onValueChange={(id) => {
                          patchUrl(
                            { counterpartyId: id || null, page: null },
                            { replace: true }
                          )
                        }}
                        parties={counterparties.map((c) => ({
                          partyId: c.counterpartyPartyId,
                          displayName: c.counterpartyPartyName,
                          description: c.customerName
                            ? `经营客户 ${c.customerName}`
                            : undefined,
                          statusLabel: "可选",
                          statusTone: "neutral",
                        }))}
                        className="w-56"
                        aria-label="筛选往来主体"
                        placeholder="全部主体"
                      />
                    </label>
                    {view === "receivable" ? (
                      <>
                        <label className="flex items-center gap-1.5 text-sm">
                          <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                            到期
                          </span>
                          <OptionCombobox
                            value={due ?? "all"}
                            onValueChange={(v) => {
                              const next = v ?? "all"
                              patchUrl(
                                {
                                  due: next === "all" ? null : next,
                                  page: null,
                                },
                                { replace: true }
                              )
                            }}
                            options={(
                              Object.keys(DUE_LABEL) as DueFilter[]
                            ).map((k) => ({
                              value: k,
                              label: DUE_LABEL[k],
                            }))}
                            className="w-32"
                            size="sm"
                            allowClear={false}
                            aria-label="筛选到期"
                            placeholder="到期"
                          />
                        </label>
                        <label className="flex items-center gap-1.5 text-sm">
                          <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                            状态
                          </span>
                          <OptionCombobox
                            value={status ?? ""}
                            onValueChange={(v) => {
                              patchUrl(
                                { status: v || null, page: null },
                                { replace: true }
                              )
                            }}
                            options={[
                              { value: "", label: "全部状态" },
                              { value: "open", label: "未结" },
                              { value: "partial", label: "部分结清" },
                              { value: "settled", label: "已结清" },
                            ]}
                            className="w-32"
                            size="sm"
                            allowClear={false}
                            aria-label="筛选状态"
                            placeholder="状态"
                          />
                        </label>
                      </>
                    ) : null}
                  </>
                }
                secondary={
                  customerId || view === "receivable" ? (
                    <>
                      {customerId ? (
                        <FilterChip
                          label={
                            lockedCustomerName
                              ? `经营客户 ${lockedCustomerName}`
                              : "经营客户锁定"
                          }
                          onClear={() =>
                            patchUrl({ customerId: null }, { replace: true })
                          }
                          clearLabel="清除客户筛选"
                        />
                      ) : null}
                      {view === "receivable" ? (
                        <label className="flex items-center gap-1.5 text-sm">
                          <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                            复核状态
                          </span>
                          <OptionCombobox
                            value={reviewStatus ?? ""}
                            onValueChange={(v) => {
                              patchUrl(
                                { reviewStatus: v || null, page: null },
                                { replace: true }
                              )
                            }}
                            options={[
                              { value: "", label: "全部复核状态" },
                              {
                                value: "pending_opening",
                                label: "期初待复核",
                              },
                              { value: "reviewed", label: "已复核" },
                              {
                                value: "pending_sync_diff",
                                label: "同步差额待复核",
                              },
                            ]}
                            className="w-40"
                            size="sm"
                            allowClear={false}
                            aria-label="筛选复核状态"
                            placeholder="复核状态"
                          />
                        </label>
                      ) : null}
                    </>
                  ) : undefined
                }
                actions={
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <span aria-live="polite">
                      共 {(data?.total ?? 0).toLocaleString("zh-CN")} 条
                    </span>
                    {hasActiveFilters ? (
                      <Button
                        type="button"
                        size="xs"
                        variant="ghost"
                        onClick={clearFilters}
                      >
                        清除筛选
                      </Button>
                    ) : null}
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      className="text-muted-foreground hover:text-foreground"
                      onClick={() => void listQuery.refetch()}
                    >
                      <RefreshCwIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                      />
                      刷新
                    </Button>
                  </div>
                }
              />
            }
            table={
              listQuery.isPending && !data ? (
                <div className="h-64 animate-pulse rounded-xl bg-muted" />
              ) : view === "unallocated" && data ? (
                <div className="space-y-6 p-1">
                  <Alert variant="info">
                    <AlertTitle>待核销分区</AlertTitle>
                    <AlertDescription>{data.unallocated.note}</AlertDescription>
                  </Alert>
                  <section className="space-y-2">
                    <h3 className="text-sm font-semibold">
                      待分配回款
                      <span className="ml-2 text-xs font-normal text-muted-foreground">
                        未分配{" "}
                        <MoneyValue
                          value={metrics?.unallocatedReceiptTotal ?? "0"}
                          className="inline"
                        />
                      </span>
                    </h3>
                    {data.unallocated.receipts.length === 0 ? (
                      <BusinessEmptyState
                        kind="no-data"
                        title="无待分配回款"
                        description="已确认且仍有未分配余额的回款将出现在此。"
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                      />
                    ) : (
                      <DataTable
                        data={[...data.unallocated.receipts]}
                        columns={receiptColumns}
                        getRowId={(r) => r.receiptId}
                        rowCount={data.unallocated.receipts.length}
                        layout="flush"
                        density="compact"
                        defaultColumnPinning={{
                          left: ["doc"],
                          right: ["actions"],
                        }}
                      />
                    )}
                  </section>
                  <Separator />
                  <section className="space-y-2">
                    <h3 className="text-sm font-semibold">
                      待分配销项发票
                      <span className="ml-2 text-xs font-normal text-muted-foreground">
                        未分配{" "}
                        <MoneyValue
                          value={metrics?.unallocatedInvoiceTotal ?? "0"}
                          className="inline"
                        />
                        （独立统计）
                      </span>
                    </h3>
                    {data.unallocated.invoices.length === 0 ? (
                      <BusinessEmptyState
                        kind="no-data"
                        title="无待分配销项发票"
                        description="已登记蓝票且仍有未分配余额的发票将出现在此。"
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                      />
                    ) : (
                      <DataTable
                        data={[...data.unallocated.invoices]}
                        columns={invoiceColumns}
                        getRowId={(r) => r.invoiceId}
                        rowCount={data.unallocated.invoices.length}
                        layout="flush"
                        density="compact"
                        defaultColumnPinning={{
                          left: ["doc"],
                          right: ["actions"],
                        }}
                      />
                    )}
                  </section>
                </div>
              ) : data?.total === 0 ? (
                data.emptyReason === "FILTER_NO_RESULT" ? (
                  <BusinessEmptyState
                    kind="filter"
                    title="无匹配往来记录"
                    description="无匹配记录，可清除筛选后重试。"
                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    action={
                      <Button
                        type="button"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        onClick={clearFilters}
                      >
                        清除筛选
                      </Button>
                    }
                  />
                ) : (
                  <BusinessEmptyState
                    kind="no-data"
                    title="当前范围尚无客户往来记录"
                    description="可从销售单进入登记；登记后刷新查看。"
                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                  />
                )
              ) : view === "receivable" && data ? (
                <DataTable
                  data={[...data.receivables]}
                  columns={receivableColumns}
                  getRowId={(r) => r.accountId}
                  rowCount={data.total}
                  pagination={pagination}
                  onPaginationChange={handlePaginationChange}
                  layout="flush"
                  density="compact"
                  defaultColumnPinning={{
                    left: ["party"],
                    right: ["actions"],
                  }}
                />
              ) : view === "receipt" && data ? (
                <DataTable
                  data={[...data.receipts]}
                  columns={receiptColumns}
                  getRowId={(r) => r.receiptId}
                  rowCount={data.total}
                  pagination={pagination}
                  onPaginationChange={handlePaginationChange}
                  layout="flush"
                  density="compact"
                  defaultColumnPinning={{
                    left: ["doc"],
                    right: ["actions"],
                  }}
                />
              ) : view === "sales_invoice" && data ? (
                <DataTable
                  data={[...data.invoices]}
                  columns={invoiceColumns}
                  getRowId={(r) => r.invoiceId}
                  rowCount={data.total}
                  pagination={pagination}
                  onPaginationChange={handlePaginationChange}
                  layout="flush"
                  density="compact"
                  defaultColumnPinning={{
                    left: ["doc"],
                    right: ["actions"],
                  }}
                />
              ) : (
                <div className="h-40 animate-pulse rounded-xl bg-muted" />
              )
            }
          />
        </>
      )}

      {/* Detail preview 768px */}
      <QuickPreviewSheet
        open={preview != null}
        onOpenChange={(open) => {
          if (!open) closePreview()
        }}
        size="detail"
        title={
          detailQuery.data?.receivable
            ? detailQuery.data.receivable.salesOrderNo
            : detailQuery.data?.receipt
              ? detailQuery.data.receipt.receiptNo
              : detailQuery.data?.invoice
                ? detailQuery.data.invoice.invoiceNo
                : "往来详情"
        }
        identity={
          detailQuery.data?.receivable ? (
            <span>{detailQuery.data.receivable.counterpartyPartyName}</span>
          ) : detailQuery.data?.receipt ? (
            <span>{detailQuery.data.receipt.counterpartyPartyName}</span>
          ) : detailQuery.data?.invoice ? (
            <span>{detailQuery.data.invoice.counterpartyPartyName}</span>
          ) : null
        }
        summary={
          detailQuery.data?.receivable ? (
            <BusinessStatusBadge
              context="preview"
              label={detailQuery.data.receivable.statusLabel}
              tone={detailQuery.data.receivable.statusTone}
            />
          ) : detailQuery.data?.receipt ? (
            <BusinessStatusBadge
              context="preview"
              label={detailQuery.data.receipt.statusLabel}
              tone={detailQuery.data.receipt.statusTone}
            />
          ) : detailQuery.data?.invoice ? (
            <div className="flex gap-2">
              <BusinessStatusBadge
                context="preview"
                label={detailQuery.data.invoice.statusLabel}
                tone={detailQuery.data.invoice.statusTone}
              />
              <Badge>{detailQuery.data.invoice.invoiceKindLabel}</Badge>
            </div>
          ) : null
        }
        footer={
          detailQuery.data ? (
            <>
              <Button type="button" variant="outline" onClick={closePreview}>
                关闭
              </Button>
              {detailQuery.data.receivable ? (
                <Button
                  type="button"
                  onClick={() =>
                    void startSession(
                      "receipt",
                      detailQuery.data!.receivable!.counterpartyPartyId,
                      undefined,
                      {
                        salesOrderId:
                          detailQuery.data!.receivable!.salesOrderId,
                        receivableAccountId:
                          detailQuery.data!.receivable!.accountId,
                      }
                    )
                  }
                >
                  登记回款并核销
                </Button>
              ) : null}
              {detailQuery.data.receipt?.allowedActions.includes(
                "CONTINUE_ALLOCATE"
              ) ? (
                <Button
                  type="button"
                  onClick={() =>
                    void startSession(
                      "receipt",
                      detailQuery.data!.receipt!.counterpartyPartyId,
                      detailQuery.data!.receipt!.receiptId
                    )
                  }
                >
                  继续核销
                </Button>
              ) : null}
              {detailQuery.data.receipt?.allowedActions.includes(
                "REVERSE_RECEIPT"
              ) ? (
                <Button
                  type="button"
                  variant="outline"
                  onClick={() =>
                    setReverseConfirm({
                      kind: "receipt_reverse",
                      sourceFactId: detailQuery.data!.receipt!.receiptId,
                      label: detailQuery.data!.receipt!.receiptNo,
                      amount: detailQuery.data!.receipt!.amount,
                    })
                  }
                >
                  冲正
                </Button>
              ) : null}
              {detailQuery.data.receipt?.allowedActions.includes("REFUND") ? (
                <Button
                  type="button"
                  variant="outline"
                  onClick={() =>
                    setReverseConfirm({
                      kind: "refund",
                      sourceFactId: detailQuery.data!.receipt!.receiptId,
                      label: detailQuery.data!.receipt!.receiptNo,
                      amount: detailQuery.data!.receipt!.amount,
                    })
                  }
                >
                  退款
                </Button>
              ) : null}
              {detailQuery.data.invoice?.allowedActions.includes(
                "CONTINUE_ALLOCATE"
              ) ? (
                <Button
                  type="button"
                  onClick={() =>
                    void startSession(
                      "invoice",
                      detailQuery.data!.invoice!.counterpartyPartyId,
                      detailQuery.data!.invoice!.invoiceId
                    )
                  }
                >
                  继续分配
                </Button>
              ) : null}
              {detailQuery.data.invoice?.allowedActions.includes(
                "ISSUE_RED_INVOICE"
              ) ? (
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => {
                    setReverseAmount(
                      detailQuery.data!.invoice!.allocatedTotal
                    )
                    setReverseConfirm({
                      kind: "red_invoice",
                      sourceFactId: detailQuery.data!.invoice!.invoiceId,
                      label: detailQuery.data!.invoice!.invoiceNo,
                      amount: detailQuery.data!.invoice!.allocatedTotal,
                    })
                  }}
                >
                  红票
                </Button>
              ) : null}
            </>
          ) : null
        }
      >
        {detailQuery.isPending ? (
          <div className="space-y-3 p-6">
            <div className="h-24 animate-pulse rounded-xl bg-muted" />
            <div className="h-40 animate-pulse rounded-xl bg-muted" />
          </div>
        ) : detailQuery.isError ? (
          <div className="space-y-3 p-6">
            <p className="text-sm text-muted-foreground">
              详情加载失败，请重试。
            </p>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => void detailQuery.refetch()}
            >
              重试
            </Button>
          </div>
        ) : detailQuery.data?.receivable ? (
          <ReceivableDetailBody row={detailQuery.data.receivable} />
        ) : detailQuery.data?.receipt ? (
          <ReceiptDetailBody row={detailQuery.data.receipt} />
        ) : detailQuery.data?.invoice ? (
          <InvoiceDetailBody row={detailQuery.data.invoice} />
        ) : (
          <div className="p-6 text-sm text-muted-foreground">
            未找到该笔记录，可能已超出当前数据范围。
          </div>
        )}
      </QuickPreviewSheet>

      {/* 选择往来主体后登记 */}
      <Dialog open={partyPickerOpen} onOpenChange={setPartyPickerOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {partyPickerMode === "receipt"
                ? "登记回款 — 选择往来主体"
                : "登记销项发票 — 选择往来主体"}
            </DialogTitle>
            <DialogDescription>
               本次核销创建后锁定往来主体，中途不可更换。
               经营客户与结算主体可能不同。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="pick-party">往来主体</Label>
            <SettlementPartyCombobox
              value={selectedPartyId || undefined}
              onValueChange={(id) => setSelectedPartyId(id ?? "")}
              parties={counterparties.map((c) => ({
                partyId: c.counterpartyPartyId,
                displayName: c.counterpartyPartyName,
                description: c.customerName
                  ? `经营客户 ${c.customerName}`
                  : undefined,
                statusLabel: "可选",
                statusTone: "neutral",
              }))}
              className="w-full"
              aria-label="往来主体"
              placeholder="请选择往来主体"
            />
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setPartyPickerOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={!selectedPartyId || createSession.isPending}
              onClick={() =>
                void startSession(partyPickerMode, selectedPartyId)
              }
            >
              <PlusIcon data-icon="inline-start" aria-hidden="true" />
              打开核销工作区
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={reverseConfirm != null}
        onOpenChange={(open) => {
          if (!open) {
            setReverseConfirm(null)
            setReverseReason("")
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {reverseConfirm?.kind === "red_invoice"
                ? "发起销项红票"
                : reverseConfirm?.kind === "refund"
                  ? "发起客户退款"
                  : "发起回款冲正"}
            </DialogTitle>
            <DialogDescription>
              不编辑、不删除已确认记录与分配；仅追加反向记录。原单{" "}
              {reverseConfirm?.label}。
              {reverseConfirm?.kind === "receipt_reverse"
                ? "冲正表示撤销本次回款记录。"
                : reverseConfirm?.kind === "refund"
                  ? "退款表示向客户退回资金。"
                  : "红票表示冲减原票的分配。"}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            {reverseConfirm?.kind === "red_invoice" ? (
              <div className="space-y-1.5">
                <Label htmlFor="rev-amount">红票金额</Label>
                <Input
                  id="rev-amount"
                  className="num"
                  inputMode="decimal"
                  value={reverseAmount}
                  onChange={(e) => setReverseAmount(e.target.value)}
                  placeholder={`不超过 ${reverseConfirm.amount ?? ""}`}
                />
                <p className="text-xs text-muted-foreground">
                  默认按原票有效净已分配全额；可输入部分金额。
                </p>
              </div>
            ) : (
              <p className="rounded-lg bg-muted/50 px-3 py-2 text-xs text-muted-foreground">
                将按原单全额追加反向记录
                {reverseConfirm?.amount ? (
                  <>
                    （<MoneyValue value={reverseConfirm.amount} />）
                  </>
                ) : (
                  ""
                )}
                ，原记录保留。
              </p>
            )}
            <div className="space-y-1.5">
              <Label htmlFor="rev-reason">原因说明</Label>
              <Textarea
                id="rev-reason"
                value={reverseReason}
                onChange={(e) => setReverseReason(e.target.value)}
                placeholder="业务依据与说明"
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                setReverseConfirm(null)
                setReverseReason("")
                setReverseAmount("")
              }}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={
                reverseMutation.isPending ||
                !reverseReason.trim() ||
                (reverseConfirm?.kind === "red_invoice"
                  ? !(
                      Number(reverseAmount) > 0 &&
                      Number(reverseAmount) <=
                        Number(reverseConfirm?.amount ?? 0) + 1e-9
                    )
                  : false)
              }
              onClick={() => void confirmReverse()}
            >
              确认追加反向记录
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </PageScaffold>
  )
}

function ReceivableDetailBody({ row }: { row: ReceivableAccountRow }) {
  return (
    <div className="space-y-5 overflow-auto p-6">
      <div className="grid grid-cols-2 gap-3">
        <Fact label="往来主体" value={row.counterpartyPartyName} />
        <Fact label="经营归属客户" value={row.customerName} />
        <Fact label="销售单" value={row.salesOrderNo} mono />
        <Fact label="业务性质" value={row.businessTypeLabel} />
        <Fact
          label="应收总额"
          value={<MoneyValue value={row.grossTotal} taxBasis="gross" />}
        />
        <Fact
          label="开放应收"
          value={<MoneyValue value={row.openTotal} taxBasis="gross" />}
        />
        <Fact
          label="已核销回款"
          value={<MoneyValue value={row.settledTotal} taxBasis="gross" />}
        />
        <Fact
          label="净已开票"
          value={<MoneyValue value={row.invoicedTotal} taxBasis="gross" />}
        />
        <Fact label="到期日" value={row.dueDate} mono />
        <Fact label="复核" value={row.reviewStatusLabel} />
      </div>
      <p className="text-xs text-muted-foreground">
        回款进度与开票进度独立；不可用开票状态推断结清。
      </p>
      <section>
        <h4 className="mb-2 text-sm font-semibold">不可变分录</h4>
        <ul className="space-y-2">
          {row.entries.map((e) => (
            <li
              key={e.entryId}
              className="rounded-xl border px-3 py-2 text-sm"
            >
              <div className="flex justify-between gap-2">
                <span>
                  {e.entryType} ·{" "}
                  {e.direction === "increase" ? "增加" : "减少"}
                </span>
                <MoneyValue value={e.amountGross} taxBasis="gross" />
              </div>
              <div className="text-xs text-muted-foreground">
                {e.sourceLabel} · 到期 {e.dueDate}
              </div>
            </li>
          ))}
        </ul>
      </section>
    </div>
  )
}

function ReceiptDetailBody({ row }: { row: ReceiptRow }) {
  return (
    <div className="space-y-5 overflow-auto p-6">
      <Alert variant="info">
        <AlertTitle>已确认记录只读</AlertTitle>
        <AlertDescription>
          已确认记录不可编辑、不可删除；纠错仅能追加退款/冲正。
        </AlertDescription>
      </Alert>
      <div className="grid grid-cols-2 gap-3">
        <Fact label="回款单号" value={row.receiptNo} mono />
        <Fact label="往来主体" value={row.counterpartyPartyName} />
        <Fact label="到账时间" value={formatDateTime(row.receivedAt, "full", "passthrough")} mono />
        <Fact
          label="到账金额"
          value={<MoneyValue value={row.amount} taxBasis="gross" />}
        />
        <Fact label="银行引用" value={row.bankReferenceMasked} mono />
        <Fact
          label="净已分配"
          value={<MoneyValue value={row.allocatedTotal} taxBasis="gross" />}
        />
        <Fact
          label="未分配"
          value={
            <MoneyValue value={row.unallocatedAmount} taxBasis="gross" />
          }
        />
      </div>
      <section>
        <h4 className="mb-2 text-sm font-semibold">分配明细（新增不覆盖原金额）</h4>
        {row.allocations.length === 0 ? (
          <p className="text-sm text-muted-foreground">尚无分配行</p>
        ) : (
          <ul className="space-y-2">
            {row.allocations.map((a) => (
              <li
                key={a.allocationId}
                className="rounded-xl border px-3 py-2 text-sm"
              >
                <div className="flex justify-between gap-2">
                  <span>
                    <Badge
                      variant={
                        a.action === "REVERSE" ? "warning" : "secondary"
                      }
                    >
                      {a.action}
                    </Badge>{" "}
                    {a.targetLabel}
                  </span>
                  <MoneyValue value={a.amountGross} />
                </div>
                <div className="text-xs text-muted-foreground">
                  {formatDateTime(a.occurredAt, "full", "passthrough")}
                  {a.reverseOfAllocationId
                    ? " · 冲减原分配"
                    : null}
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}

function InvoiceDetailBody({ row }: { row: SalesInvoiceRow }) {
  return (
    <div className="space-y-5 overflow-auto p-6">
      <Alert variant="info">
        <AlertTitle>已登记发票只读</AlertTitle>
        <AlertDescription>
          已登记发票不可编辑、不可删除；红票为独立记录加反向分配。
        </AlertDescription>
      </Alert>
      <div className="grid grid-cols-2 gap-3">
        <Fact label="发票号码" value={row.invoiceNo} mono />
        <Fact label="种类" value={row.invoiceKindLabel} />
        <Fact label="代码" value={row.invoiceCode ?? "—"} mono />
        <Fact label="开票日期" value={row.invoiceDate} mono />
        <Fact
          label="含税"
          value={<MoneyValue value={row.grossAmount} taxBasis="gross" />}
        />
        <Fact
          label="不含税 / 税额"
          value={
            <span>
              <MoneyValue value={row.netAmount} /> /{" "}
              <MoneyValue value={row.taxAmount} />
            </span>
          }
        />
        <Fact
          label="净已分配"
          value={<MoneyValue value={row.allocatedTotal} taxBasis="gross" />}
        />
        <Fact
          label="未分配"
          value={
            <MoneyValue value={row.unallocatedAmount} taxBasis="gross" />
          }
        />
      </div>
      <section>
        <h4 className="mb-2 text-sm font-semibold">分配明细（独立于回款）</h4>
        {row.allocations.length === 0 ? (
          <p className="text-sm text-muted-foreground">尚无分配行</p>
        ) : (
          <ul className="space-y-2">
            {row.allocations.map((a) => (
              <li
                key={a.allocationId}
                className="rounded-xl border px-3 py-2 text-sm"
              >
                <div className="flex justify-between gap-2">
                  <span>
                    <Badge
                      variant={
                        a.action === "REVERSE" ? "warning" : "secondary"
                      }
                    >
                      {a.action === "REVERSE" ? "反向记录" : "分配"}
                    </Badge>{" "}
                    {a.targetLabel}
                  </span>
                  <MoneyValue value={a.amountGross} />
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}

function Fact({
  label,
  value,
  mono,
}: {
  label: string
  value: React.ReactNode
  mono?: boolean
}) {
  return (
    <div>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={mono ? "num text-sm font-medium" : "text-sm font-medium"}>
        {value}
      </div>
    </div>
  )
}

function csvEscape(value: string | number | null | undefined): string {
  const s = value == null ? "" : String(value)
  return `"${s.replaceAll('"', '""')}"`
}

function buildAccountsCsv(data: CustomerAccountsListView): string {
  let header: string[] = []
  let rows: (string | number | null | undefined)[][] = []
  if (data.view === "receivable") {
    header = [
      "销售单",
      "往来主体",
      "经营客户",
      "到期日",
      "应收总额",
      "净已收",
      "开放应收",
      "净已开票",
      "可开票",
      "状态",
    ]
    rows = data.receivables.map((r) => [
      r.salesOrderNo,
      r.counterpartyPartyName,
      r.customerName,
      r.dueDate,
      r.grossTotal,
      r.settledTotal,
      r.openTotal,
      r.invoicedTotal,
      r.openInvoiceableTotal,
      r.statusLabel,
    ])
  } else if (data.view === "receipt") {
    header = ["回款单号", "往来主体", "到账时间", "到账金额", "净已分配", "未分配", "状态"]
    rows = data.receipts.map((r) => [
      r.receiptNo,
      r.counterpartyPartyName,
      r.receivedAt,
      r.amount,
      r.allocatedTotal,
      r.unallocatedAmount,
      r.statusLabel,
    ])
  } else if (data.view === "sales_invoice") {
    header = ["发票号码", "代码", "种类", "开票日期", "含税", "不含税", "税额", "净已分配", "未分配", "状态"]
    rows = data.invoices.map((r) => [
      r.invoiceNo,
      r.invoiceCode ?? "",
      r.invoiceKindLabel,
      r.invoiceDate,
      r.grossAmount,
      r.netAmount,
      r.taxAmount,
      r.allocatedTotal,
      r.unallocatedAmount,
      r.statusLabel,
    ])
  } else {
    header = ["轨道", "单号", "供应商", "记录金额", "未分配余额"]
    rows = [
      ...data.unallocated.receipts.map((r) => [
        "回款",
        r.receiptNo,
        r.counterpartyPartyName,
        r.amount,
        r.unallocatedAmount,
      ]),
      ...data.unallocated.invoices.map((r) => [
        "销项发票",
        r.invoiceNo,
        r.counterpartyPartyName,
        r.grossAmount,
        r.unallocatedAmount,
      ]),
    ]
  }
  return [
    header.map(csvEscape).join(","),
    ...rows.map((r) => r.map(csvEscape).join(",")),
  ].join("\r\n")
}

function downloadCsv(fileName: string, content: string): void {
  const blob = new Blob(["\uFEFF" + content], {
    type: "text/csv;charset=utf-8",
  })
  const url = URL.createObjectURL(blob)
  const link = document.createElement("a")
  link.href = url
  link.download = fileName
  document.body.appendChild(link)
  link.click()
  link.remove()
  URL.revokeObjectURL(url)
}
