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
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  MoneyValue,
  OptionCombobox,
  PageActions,
  PageHeader,
  QuickPreviewSheet,
} from "@/components/business"
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

function formatDateTime(iso: string) {
  try {
    return new Date(iso).toLocaleString("zh-CN", {
      hour12: false,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    })
  } catch {
    return iso
  }
}

type PreviewKind = "receivable" | "receipt" | "invoice"
type PreviewState = { kind: PreviewKind; id: string } | null

type ResultState =
  | {
      status: "succeeded" | "unknown" | "rejected" | "blocked"
      title: string
      description: string
      reference?: string
      facts?: Array<{ label: string; value: string }>
    }
  | null

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
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
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
  } | null>(null)
  const [reverseReason, setReverseReason] = React.useState("")

  const query: CustomerAccountsQuery = React.useMemo(
    () => ({
      view,
      q: qParam || undefined,
      counterpartyPartyId,
      customerId,
      due,
      status,
      reviewStatus,
      focusId,
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
      focusId,
      salesOrderId,
      receivableAccountId,
      returnTo,
      from,
    ]
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
    const next = new URLSearchParams(searchParams.toString())
    for (const [key, value] of Object.entries(patch)) {
      if (value == null || value === "") next.delete(key)
      else next.set(key, value)
    }
    if (!next.get("view")) next.set("view", view)
    const qs = next.toString()
    const href = qs ? `${pathname}?${qs}` : pathname
    if (options?.replace) router.replace(href)
    else router.push(href)
  }

  React.useEffect(() => {
    setSearchInput(qParam)
  }, [qParam])

  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (searchInput === qParam) return
      patchUrl({ q: searchInput.trim() || null }, { replace: true })
      setPagination((p) => ({ ...p, pageIndex: 0 }))
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
          err instanceof Error ? err.message : "无法打开核销会话"
        )
      }
    })()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data, from, returnTo, sessionId])

  const openPreview = React.useCallback(
    (next: PreviewState) => {
      setPreview(next)
      if (next) {
        patchUrl(
          { previewKind: next.kind, previewId: next.id },
          { replace: true }
        )
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [searchParams, pathname, view]
  )

  const closePreview = React.useCallback(() => {
    setPreview(null)
    patchUrl({ previewKind: null, previewId: null }, { replace: true })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, pathname, view])

  async function startSession(
    mode: AllocationMode,
    partyId: string,
    existingFactId?: string
  ) {
    setActionError(null)
    setLastResult(null)
    try {
      const session = await createSession.mutateAsync({
        mode,
        counterpartyPartyId: partyId,
        existingFactId,
        salesOrderId,
        receivableAccountId,
        returnTo,
        from,
      })
      setPartyPickerOpen(false)
      patchUrl({
        sessionId: session.draftSessionId,
        counterpartyId: partyId,
      })
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "创建核销会话失败")
    }
  }

  function openRegister(mode: AllocationMode) {
    setPartyPickerMode(mode)
    setSelectedPartyId(counterpartyPartyId ?? data?.counterparties[0]?.counterpartyPartyId ?? "")
    setPartyPickerOpen(true)
  }

  async function confirmReverse() {
    if (!reverseConfirm) return
    const key = `w11-rev-${reverseConfirm.sourceFactId}-${Date.now()}`
    const res = await reverseMutation.mutateAsync({
      kind: reverseConfirm.kind,
      sourceFactId: reverseConfirm.sourceFactId,
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
        header: "净已开票 / 可开票",
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
              onClick={() =>
                void startSession(
                  "receipt",
                  row.original.counterpartyPartyId
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
            {formatDateTime(row.original.receivedAt)}
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
        <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
          <div className="h-10 w-64 animate-pulse rounded-lg bg-muted" />
          <div className="h-96 animate-pulse rounded-2xl bg-muted" />
        </div>
      )
    }
    if (!sessionQuery.data) {
      return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
          <BusinessFailureState
            kind="business"
            title="核销会话无效"
            description="核销会话不存在或已失效。"
            action={
              <Button
                type="button"
                onClick={() => patchUrl({ sessionId: null })}
              >
                返回列表
              </Button>
            }
          />
        </div>
      )
    }
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
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
      </div>
    )
  }

  if (listQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessFailureState
          kind="system"
          description="客户往来查询失败，未展示 0 元结清结论。"
          action={
            <Button type="button" onClick={() => void listQuery.refetch()}>
              重试
            </Button>
          }
        />
      </div>
    )
  }

  const metrics = data?.metrics
  const counterparties: readonly CounterpartyOption[] =
    data?.counterparties ?? []

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-2 p-3 md:p-4">
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
                  setLastResult({
                    status: "succeeded",
                    title: "导出任务已创建",
                    description: `服务端筛选结果：${data?.filterSummary ?? ""}。7 天内可下载（演示）。`,
                    reference: `EXP-W11-${Date.now().toString(36)}`,
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
                onClick: () => openRegister("invoice"),
              },
              {
                actionKey: "register-receipt",
                label: "登记回款",
                icon: WalletIcon,
                mobileVisibility: "hide",
                disabled: !data?.canRegister,
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
            {salesOrderId ? ` · 销售单 ${salesOrderId}` : ""}
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

      {customerId ? (
        <Alert variant="info">
          <AlertTitle>客户筛选</AlertTitle>
          <AlertDescription>
            已按经营归属客户过滤（customerId={customerId}
            ）。核销相等键仍为 counterparty_party_id。
            <Button
              type="button"
              size="sm"
              variant="link"
              className="ml-2 h-auto p-0"
              onClick={() => patchUrl({ customerId: null })}
            >
              清除
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {lastResult ? (
        <FormalActionResult
          status={lastResult.status}
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
                detail="服务端更新时间"
                active={view === "receivable"}
                onClick={() => {
                  patchUrl({ view: "receivable" })
                  setPagination((p) => ({ ...p, pageIndex: 0 }))
                }}
              />
              <MetricFilterItem
                label="已逾期应收"
                value={<MoneyValue value={metrics.overdueReceivableTotal} />}
                detail="需催收"
                active={view === "receivable" && due === "overdue"}
                onClick={() => {
                  patchUrl({ view: "receivable", due: "overdue" })
                  setPagination((p) => ({ ...p, pageIndex: 0 }))
                }}
              />
              <MetricFilterItem
                label="待分配回款"
                value={<MoneyValue value={metrics.unallocatedReceiptTotal} />}
                detail="已到账"
                active={view === "unallocated"}
                onClick={() => {
                  patchUrl({ view: "unallocated", due: null })
                  setPagination((p) => ({ ...p, pageIndex: 0 }))
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
                  patchUrl({ view: "sales_invoice", due: null })
                  setPagination((p) => ({ ...p, pageIndex: 0 }))
                }}
              />
            </MetricStrip>
          ) : (
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
              {Array.from({ length: 4 }).map((_, i) => (
                <div
                  key={i}
                  className="h-20 animate-pulse rounded-2xl bg-muted"
                />
              ))}
            </div>
          )}

          <Tabs
            value={view}
            onValueChange={(v) => {
              patchUrl({ view: v, due: v === "receivable" ? due ?? null : null })
              setPagination((p) => ({ ...p, pageIndex: 0 }))
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
                    · 提交策略：{data.submitPolicy.label}
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
                      <OptionCombobox
                        value={counterpartyPartyId ?? ""}
                        onValueChange={(v) => {
                          patchUrl({
                            counterpartyId: v || null,
                          })
                          setPagination((p) => ({ ...p, pageIndex: 0 }))
                        }}
                        options={[
                          { value: "", label: "全部主体" },
                          ...counterparties.map((c) => ({
                            value: c.counterpartyPartyId,
                            label: c.counterpartyPartyName,
                          })),
                        ]}
                        className="w-52"
                        size="sm"
                        allowClear={false}
                        aria-label="筛选往来主体"
                        placeholder="全部主体"
                      />
                    </label>
                    {view === "receivable" ? (
                      <label className="flex items-center gap-1.5 text-sm">
                        <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                          到期
                        </span>
                        <OptionCombobox
                          value={due ?? "all"}
                          onValueChange={(v) => {
                            const next = v ?? "all"
                            patchUrl({ due: next === "all" ? null : next })
                            setPagination((p) => ({ ...p, pageIndex: 0 }))
                          }}
                          options={(Object.keys(DUE_LABEL) as DueFilter[]).map(
                            (k) => ({
                              value: k,
                              label: DUE_LABEL[k],
                            })
                          )}
                          className="w-32"
                          size="sm"
                          allowClear={false}
                          aria-label="筛选到期"
                          placeholder="到期"
                        />
                      </label>
                    ) : null}
                  </>
                }
                actions={
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => void listQuery.refetch()}
                  >
                    <RefreshCwIcon data-icon="inline-start" aria-hidden="true" />
                    刷新
                  </Button>
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
                        未分配 {metrics?.unallocatedReceiptTotal}
                      </span>
                    </h3>
                    {data.unallocated.receipts.length === 0 ? (
                      <BusinessEmptyState
                        kind="no-data"
                        title="无待分配回款"
                        description="已过账且仍有未分配余额的回款将出现在此。"
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
                        未分配 {metrics?.unallocatedInvoiceTotal}（不与回款相加）
                      </span>
                    </h3>
                    {data.unallocated.invoices.length === 0 ? (
                      <BusinessEmptyState
                        kind="no-data"
                        title="无待分配销项发票"
                        description="已登记蓝票且仍有未分配余额的发票将出现在此。"
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
                    description="保留筛选摘要；可清除筛选后重试。"
                    action={
                      <Button
                        type="button"
                        variant="outline"
                        onClick={() => {
                          patchUrl({
                            q: null,
                            counterpartyId: null,
                            customerId: null,
                            due: null,
                            status: null,
                            reviewStatus: null,
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
                    title="当前范围尚无客户往来记录"
                    description="有权时从销售单链入登记；应收形成后刷新。"
                  />
                )
              ) : view === "receivable" && data ? (
                <DataTable
                  data={[...data.receivables]}
                  columns={receivableColumns}
                  getRowId={(r) => r.accountId}
                  rowCount={data.total}
                  pagination={pagination}
                  onPaginationChange={setPagination}
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
                  onPaginationChange={setPagination}
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
                  onPaginationChange={setPagination}
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
                      detailQuery.data!.receivable!.counterpartyPartyId
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
                  onClick={() =>
                    setReverseConfirm({
                      kind: "red_invoice",
                      sourceFactId: detailQuery.data!.invoice!.invoiceId,
                      label: detailQuery.data!.invoice!.invoiceNo,
                    })
                  }
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
        ) : detailQuery.data?.receivable ? (
          <ReceivableDetailBody row={detailQuery.data.receivable} />
        ) : detailQuery.data?.receipt ? (
          <ReceiptDetailBody row={detailQuery.data.receipt} />
        ) : detailQuery.data?.invoice ? (
          <InvoiceDetailBody row={detailQuery.data.invoice} />
        ) : (
          <div className="p-6 text-sm text-muted-foreground">未找到对象</div>
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
              核销会话创建后锁定 counterparty_party_id，中途不可更换主体。
              经营客户与结算主体可能不同。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="pick-party">往来主体</Label>
            <OptionCombobox
              id="pick-party"
              value={selectedPartyId}
              onValueChange={(v) => setSelectedPartyId(v ?? "")}
              options={[
                { value: "", label: "请选择" },
                ...counterparties.map((c) => ({
                  value: c.counterpartyPartyId,
                  label: `${c.counterpartyPartyName}（${c.customerName}）`,
                })),
              ]}
              className="w-full"
              allowClear={false}
              aria-label="往来主体"
              placeholder="请选择"
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
              不编辑、不删除已过账记录与分配；仅追加反向记录。原单{" "}
              {reverseConfirm?.label}。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="rev-reason">原因说明</Label>
            <Textarea
              id="rev-reason"
              value={reverseReason}
              onChange={(e) => setReverseReason(e.target.value)}
              placeholder="业务依据与说明"
            />
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                setReverseConfirm(null)
                setReverseReason("")
              }}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={reverseMutation.isPending || !reverseReason.trim()}
              onClick={() => void confirmReverse()}
            >
              确认追加反向记录
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
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
        回款进度与开票进度独立；不可用开票状态推断结清。金额均为服务端返回。
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
        <AlertTitle>已过账记录只读</AlertTitle>
        <AlertDescription>
          canEdit={String(row.canEdit)} · canDelete={String(row.canDelete)}
          。纠错仅能追加退款/冲正。
        </AlertDescription>
      </Alert>
      <div className="grid grid-cols-2 gap-3">
        <Fact label="回款单号" value={row.receiptNo} mono />
        <Fact label="往来主体" value={row.counterpartyPartyName} />
        <Fact label="到账时间" value={formatDateTime(row.receivedAt)} mono />
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
        <h4 className="mb-2 text-sm font-semibold">分配明细（追加式）</h4>
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
                  {formatDateTime(a.occurredAt)}
                  {a.reverseOfAllocationId
                    ? ` · 反向于 ${a.reverseOfAllocationId}`
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
          canEdit={String(row.canEdit)} · canDelete={String(row.canDelete)}
          。红票为独立记录 + 反向分配。
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
                      {a.action}
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
