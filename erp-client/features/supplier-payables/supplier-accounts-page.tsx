"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  ExternalLinkIcon,
  FilePlus2Icon,
  RefreshCwIcon,
  SearchIcon,
  WalletCardsIcon,
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
  SupplierCombobox,
} from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  DescriptionDetails,
  DescriptionItem,
  DescriptionList,
  DescriptionTerm,
} from "@/components/ui/description-list"
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
import { AllocationSession } from "@/features/supplier-payables/allocation-session"
import {
  useDemoPermissionMutation,
  useDemoSetPolicyMutation,
  usePayableDetailQuery,
  useReverseInvoiceMutation,
  useReversePaymentMutation,
  useSupplierAccountsQuery,
} from "@/features/supplier-payables/queries"
import type {
  AllocationTrack,
  FormalSubmitResult,
  PayableRow,
  PaymentRow,
  PurchaseInvoiceRow,
  SupplierAccountsView,
  UnallocatedRow,
} from "@/features/supplier-payables/types"
import { VIEW_LABEL } from "@/features/supplier-payables/types"

function parseView(raw: string | null): SupplierAccountsView {
  if (
    raw === "payment" ||
    raw === "purchase_invoice" ||
    raw === "unallocated" ||
    raw === "payable"
  ) {
    return raw
  }
  return "payable"
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

type SessionState = {
  track: AllocationTrack
  supplierId: string
  draftSessionId?: string
  purchaseOrderId?: string
  returnTo?: string
  fromWorkspace?: string
  existingPaymentId?: string
  existingInvoiceId?: string
  preselectPayableAccountId?: string
}

export function SupplierAccountsPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const view = parseView(searchParams.get("view"))
  const qParam = searchParams.get("q") ?? ""
  const supplierId = searchParams.get("supplierId") ?? undefined
  const sourceType = (searchParams.get("sourceType") as
    | "PURCHASE_ORDER"
    | "SUPPLIER_SETTLEMENT"
    | null) ?? undefined
  const status = searchParams.get("status") ?? undefined
  const due = (searchParams.get("due") as
    | "not_due"
    | "due_today"
    | "overdue"
    | "all"
    | null) ?? undefined
  const paymentGate = (searchParams.get("paymentGate") as
    | "satisfied"
    | "unsatisfied"
    | "all"
    | null) ?? undefined
  const purchaseOrderId = searchParams.get("purchaseOrderId") ?? undefined
  const fromWorkspace = searchParams.get("from") ?? undefined
  const returnTo = searchParams.get("returnTo") ?? undefined
  const sessionTrack = searchParams.get("session") as AllocationTrack | null
  const detailId = searchParams.get("detailId") ?? undefined
  const existingPaymentId = searchParams.get("paymentId") ?? undefined
  const existingInvoiceId = searchParams.get("invoiceId") ?? undefined

  const [searchInput, setSearchInput] = React.useState(qParam)
  const searchInputRef = React.useRef<HTMLInputElement | null>(null)
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [previewPayableId, setPreviewPayableId] = React.useState<string | null>(
    detailId ?? null
  )
  const [session, setSession] = React.useState<SessionState | null>(null)
  const [pickSupplierOpen, setPickSupplierOpen] = React.useState<
    null | AllocationTrack
  >(null)
  const [pickSupplierId, setPickSupplierId] = React.useState("")
  const [reverseTarget, setReverseTarget] = React.useState<
    | { kind: "payment"; id: string; no: string }
    | { kind: "invoice"; id: string; no: string }
    | null
  >(null)
  const [reverseReason, setReverseReason] = React.useState("")
  const [redInvoiceNo, setRedInvoiceNo] = React.useState("")
  const [lastResult, setLastResult] = React.useState<FormalSubmitResult | null>(
    null
  )
  const deepLinkHandled = React.useRef(false)

  const query = React.useMemo(
    () => ({
      view,
      q: qParam || undefined,
      supplierId,
      sourceType:
        sourceType === "PURCHASE_ORDER" || sourceType === "SUPPLIER_SETTLEMENT"
          ? sourceType
          : undefined,
      status,
      due: due === "not_due" || due === "due_today" || due === "overdue" ? due : undefined,
      paymentGate:
        paymentGate === "satisfied" || paymentGate === "unsatisfied"
          ? paymentGate
          : undefined,
      purchaseOrderId,
    }),
    [
      view,
      qParam,
      supplierId,
      sourceType,
      status,
      due,
      paymentGate,
      purchaseOrderId,
    ]
  )

  const listQuery = useSupplierAccountsQuery(query)
  const detailQuery = usePayableDetailQuery(previewPayableId)
  const reversePayment = useReversePaymentMutation()
  const reverseInvoice = useReverseInvoiceMutation()
  const demoPolicy = useDemoSetPolicyMutation()
  const demoPerm = useDemoPermissionMutation()

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
    }, 300)
    return () => globalThis.clearTimeout(handle)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchInput])

  // Deep-link from W08/W09: open payment session with PO preselected
  React.useEffect(() => {
    if (deepLinkHandled.current) return
    if (!data?.moduleAllowed) return
    if (sessionTrack === "payment" || sessionTrack === "purchase_invoice") {
      if (supplierId) {
        deepLinkHandled.current = true
        setSession({
          track: sessionTrack,
          supplierId,
          purchaseOrderId,
          returnTo,
          fromWorkspace,
          existingPaymentId,
          existingInvoiceId,
        })
        return
      }
    }
    // from=W08/W09 without session: auto open payment if we can resolve supplier
    if (
      (fromWorkspace === "W08" || fromWorkspace === "W09") &&
      purchaseOrderId
    ) {
      const match = data.payables.find(
        (p) =>
          p.sourceType === "PURCHASE_ORDER" &&
          p.sourceDocumentId === purchaseOrderId
      )
      const sid = supplierId ?? match?.supplierId
      if (sid) {
        deepLinkHandled.current = true
        setSession({
          track: "payment",
          supplierId: sid,
          purchaseOrderId,
          returnTo,
          fromWorkspace,
          preselectPayableAccountId: match?.payableAccountId,
        })
        patchUrl(
          {
            session: "payment",
            supplierId: sid,
          },
          { replace: true }
        )
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data?.queriedAt, fromWorkspace, purchaseOrderId, supplierId, sessionTrack])

  function openSession(next: SessionState) {
    setLastResult(null)
    setSession(next)
    patchUrl(
      {
        session: next.track,
        supplierId: next.supplierId,
        paymentId: next.existingPaymentId ?? null,
        invoiceId: next.existingInvoiceId ?? null,
        detailId: null,
      },
      { replace: true }
    )
  }

  function closeSession() {
    setSession(null)
    patchUrl(
      {
        session: null,
        paymentId: null,
        invoiceId: null,
      },
      { replace: true }
    )
  }

  function openPreview(payableAccountId: string) {
    setPreviewPayableId(payableAccountId)
    patchUrl({ detailId: payableAccountId }, { replace: true })
  }

  function closePreview() {
    setPreviewPayableId(null)
    patchUrl({ detailId: null }, { replace: true })
  }

  const payableColumns = React.useMemo<ColumnDef<PayableRow>[]>(
    () => [
      {
        id: "supplier",
        header: "供应商 / 来源",
        meta: { label: "供应商", width: "reference" },
        cell: ({ row }) => (
          <div className="flex min-w-0 items-center gap-1.5 text-sm">
            <span className="truncate font-medium">{row.original.supplierName}</span>
            <span className="shrink-0 text-muted-foreground">·</span>
            <span className="truncate text-xs text-muted-foreground">
              {row.original.sourceTypeLabel} ·{" "}
              <span className="num">{row.original.sourceDocumentNo}</span>
            </span>
          </div>
        ),
      },
      {
        id: "amounts",
        header: "应付（含税）/ 开放",
        meta: { label: "金额", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <div className="flex items-center justify-end gap-1 text-end text-sm">
            <MoneyValue value={row.original.grossTotal} />
            <span className="text-xs text-muted-foreground">/ 开放</span>
            <MoneyValue className="text-xs" value={row.original.openTotal} />
          </div>
        ),
      },
      {
        id: "tracks",
        header: "已付 / 已收票",
        meta: { label: "进度", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <div className="flex items-center justify-end gap-1.5 text-end text-xs text-muted-foreground">
            <span>付款</span> <MoneyValue value={row.original.settledTotal} />
            <span>/ 收票</span> <MoneyValue value={row.original.invoicedTotal} />
          </div>
        ),
      },
      {
        id: "due",
        header: "到期",
        meta: { label: "到期", width: "default" },
        cell: ({ row }) => (
          <div className="flex items-center gap-1.5 text-sm">
            <span className="num">{row.original.dueDate}</span>
            <span className="text-xs text-muted-foreground">
              {row.original.dueStateLabel}
            </span>
          </div>
        ),
      },
      {
        id: "status",
        header: "状态",
        meta: { label: "状态", width: "status" },
        cell: ({ row }) => (
          <div className="flex items-center gap-1.5">
            <BusinessStatusBadge
              context="list"
              label={row.original.statusLabel}
              tone={row.original.statusTone}
            />
            {row.original.paymentGateSummary &&
            row.original.paymentGateSummary.state !== "NOT_APPLICABLE" ? (
              <span className="text-[11px] text-muted-foreground">
                门禁{" "}
                {row.original.paymentGateSummary.state === "SATISFIED"
                  ? "已满足"
                  : "未满足"}
              </span>
            ) : null}
          </div>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        cell: ({ row }) => (
          <div className="flex flex-nowrap justify-end gap-1">
            <Button
              type="button"
              size="xs"
              variant="outline"
              onClick={() => openPreview(row.original.payableAccountId)}
            >
              预览
            </Button>
            <Button
              type="button"
              size="xs"
              onClick={() =>
                openSession({
                  track: "payment",
                  supplierId: row.original.supplierId,
                  preselectPayableAccountId: row.original.payableAccountId,
                  purchaseOrderId:
                    row.original.sourceType === "PURCHASE_ORDER"
                      ? row.original.sourceDocumentId
                      : undefined,
                  returnTo,
                  fromWorkspace,
                })
              }
              disabled={!data?.canRegisterPayment}
            >
              核销付款
            </Button>
          </div>
        ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [data?.canRegisterPayment, returnTo, fromWorkspace]
  )

  const paymentColumns = React.useMemo<ColumnDef<PaymentRow>[]>(
    () => [
      {
        id: "doc",
        header: "付款单",
        meta: { label: "付款单", width: "reference" },
        cell: ({ row }) => (
          <div className="text-sm">
            <div className="num font-medium">{row.original.paymentNo}</div>
            <div className="text-xs text-muted-foreground">
              {row.original.supplierName}
            </div>
          </div>
        ),
      },
      {
        id: "amount",
        header: "金额 / 未分配",
        meta: { label: "金额", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <div className="text-end text-sm">
            <MoneyValue value={row.original.amount} taxBasis="gross" />
            <div className="text-xs text-muted-foreground">
              未分配 <MoneyValue value={row.original.unallocatedAmount} />
            </div>
          </div>
        ),
      },
      {
        id: "bank",
        header: "银行引用",
        meta: { label: "银行", width: "default" },
        cell: ({ row }) => (
          <span className="num text-sm">{row.original.bankReferenceMasked}</span>
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
            description={
              row.original.status === "POSTED"
                ? "已过账不可编辑；纠错请冲正"
                : undefined
            }
          />
        ),
      },
      {
        id: "time",
        header: "付款时间",
        meta: { label: "时间", width: "default", numeric: true },
        cell: ({ row }) => (
          <span className="num text-xs text-muted-foreground">
            {formatDateTime(row.original.paidAt)}
          </span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        cell: ({ row }) => (
          <div className="flex flex-wrap justify-end gap-1">
            {row.original.allowedActions.includes("CONTINUE_ALLOCATE") ? (
              <Button
                type="button"
                size="xs"
                onClick={() =>
                  openSession({
                    track: "payment",
                    supplierId: row.original.supplierId,
                    existingPaymentId: row.original.paymentId,
                    returnTo,
                    fromWorkspace,
                  })
                }
              >
                继续核销
              </Button>
            ) : null}
            {row.original.allowedActions.includes("REVERSE") ? (
              <Button
                type="button"
                size="xs"
                variant="outline"
                onClick={() =>
                  setReverseTarget({
                    kind: "payment",
                    id: row.original.paymentId,
                    no: row.original.paymentNo,
                  })
                }
              >
                冲正
              </Button>
            ) : null}
          </div>
        ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [returnTo, fromWorkspace]
  )

  const invoiceColumns = React.useMemo<ColumnDef<PurchaseInvoiceRow>[]>(
    () => [
      {
        id: "doc",
        header: "进项发票",
        meta: { label: "发票", width: "reference" },
        cell: ({ row }) => (
          <div className="text-sm">
            <div className="font-medium">
              <span className="num">
                {row.original.invoiceCode}-{row.original.invoiceNo}
              </span>
              <Badge variant="neutral" className="ml-2">
                {row.original.invoiceKindLabel}
              </Badge>
            </div>
            <div className="text-xs text-muted-foreground">
              {row.original.supplierName}
            </div>
          </div>
        ),
      },
      {
        id: "amount",
        header: "含税 / 未分配",
        meta: { label: "金额", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <div className="text-end text-sm">
            <MoneyValue value={row.original.grossAmount} taxBasis="gross" />
            <div className="text-xs text-muted-foreground">
              未分配 <MoneyValue value={row.original.unallocatedAmount} />
            </div>
          </div>
        ),
      },
      {
        id: "alloc",
        header: "净已分配",
        meta: { label: "分配", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <div className="text-end">
            <MoneyValue value={row.original.allocatedTotal} />
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
            description="与付款进度独立"
          />
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        cell: ({ row }) => (
          <div className="flex flex-wrap justify-end gap-1">
            {row.original.allowedActions.includes("CONTINUE_ALLOCATE") ? (
              <Button
                type="button"
                size="xs"
                onClick={() =>
                  openSession({
                    track: "purchase_invoice",
                    supplierId: row.original.supplierId,
                    existingInvoiceId: row.original.invoiceId,
                  })
                }
              >
                继续核销
              </Button>
            ) : null}
            {row.original.allowedActions.includes("RED_INVOICE") ? (
              <Button
                type="button"
                size="xs"
                variant="outline"
                onClick={() => {
                  setRedInvoiceNo(`R${row.original.invoiceNo}`)
                  setReverseTarget({
                    kind: "invoice",
                    id: row.original.invoiceId,
                    no: `${row.original.invoiceCode}-${row.original.invoiceNo}`,
                  })
                }}
              >
                红票
              </Button>
            ) : null}
          </div>
        ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    []
  )

  const unallocatedColumns = React.useMemo<ColumnDef<UnallocatedRow>[]>(
    () => [
      {
        id: "track",
        header: "轨道",
        meta: { label: "轨道", width: "default" },
        cell: ({ row }) => (
          <Badge
            variant={row.original.track === "payment" ? "warning" : "info"}
          >
            {row.original.trackLabel}
          </Badge>
        ),
      },
      {
        id: "doc",
        header: "单据 / 供应商",
        meta: { label: "单据", width: "reference" },
        cell: ({ row }) => (
          <div className="text-sm">
            <div className="num font-medium">{row.original.documentNo}</div>
            <div className="text-xs text-muted-foreground">
              {row.original.supplierName}
            </div>
          </div>
        ),
      },
      {
        id: "amount",
        header: "未分配余额",
        meta: { label: "余额", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <div className="text-end">
            <MoneyValue value={row.original.unallocatedAmount} taxBasis="gross" />
            <div className="text-xs text-muted-foreground">
              记录 <MoneyValue value={row.original.amount} />
            </div>
          </div>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        cell: ({ row }) => {
          const payment = data?.payments.find(
            (p) => p.paymentNo === row.original.documentNo
          )
          const invoice = data?.invoices.find(
            (p) =>
              `${p.invoiceCode}-${p.invoiceNo}` === row.original.documentNo
          )
          return (
            <Button
              type="button"
              size="xs"
              onClick={() =>
                openSession({
                  track: row.original.track,
                  supplierId: row.original.supplierId,
                  existingPaymentId:
                    row.original.track === "payment"
                      ? payment?.paymentId
                      : undefined,
                  existingInvoiceId:
                    row.original.track === "purchase_invoice"
                      ? invoice?.invoiceId
                      : undefined,
                })
              }
            >
              继续核销
            </Button>
          )
        },
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [data?.payments, data?.invoices]
  )

  if (session) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <AllocationSession
          {...session}
          onClose={closeSession}
          onCompleted={(result) => {
            setLastResult(result)
          }}
        />
      </div>
    )
  }

  if (listQuery.isPending && !data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
        <div className="grid grid-cols-2 gap-3 md:grid-cols-5">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="h-20 animate-pulse rounded-2xl bg-muted" />
          ))}
        </div>
        <div className="h-[28rem] animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (listQuery.isError && !data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessFailureState
          kind="system"
          title="供应商往来加载失败"
          description="请重试。失败时不展示 0 元或门禁已满足结论。"
          action={
            <Button
              type="button"
              variant="outline"
              onClick={() => void listQuery.refetch()}
            >
              重试
            </Button>
          }
        />
      </div>
    )
  }

  if (!data) return null

  if (!data.moduleAllowed) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessEmptyState
          kind="no-scope"
          title="无供应商往来权限"
          description="权限已收回或未授权。敏感字段与导出结果已清除，不能提交。"
          action={
            <Button
              type="button"
              variant="outline"
              onClick={() => void demoPerm.mutateAsync("restore")}
            >
              演示：恢复权限
            </Button>
          }
        />
      </div>
    )
  }

  if (!data.hasDataScope) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessEmptyState
          kind="no-scope"
          title="当前角色未配置供应商往来范围"
          description="不能显示为 0 元应付。请联系管理员配置组织/供应商范围后再查询。"
        />
      </div>
    )
  }

  const rows =
    view === "payable"
      ? data.payables
      : view === "payment"
        ? data.payments
        : view === "purchase_invoice"
          ? data.invoices
          : data.unallocated

  const pageRows = rows.slice(
    pagination.pageIndex * pagination.pageSize,
    pagination.pageIndex * pagination.pageSize + pagination.pageSize
  )

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-2 p-3 md:p-4">
      <PageHeader
        title="供应商往来"
        breadcrumbs={[
          { id: "fin", label: "财务", href: "/finance/supplier-accounts" },
          { id: "ap", label: "供应商往来", current: true },
        ]}
        metadata={
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
            <DataFreshness
              updatedAt={new Date(data.queriedAt).toLocaleString("zh-CN")}
              dateTime={data.queriedAt}
              label={`数据更新时间 ${data.dataWatermark} · 查询于`}
            />
            <p className="text-xs text-muted-foreground">
              策略{" "}
              {data.payablePriorityPolicy.state === "AVAILABLE"
                ? `${data.payablePriorityPolicy.payablePriorityPolicyId}@v${data.payablePriorityPolicy.payablePriorityPolicyVersion}`
                : data.payablePriorityPolicy.state}
            </p>
          </div>
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "refresh",
                label: "刷新",
                icon: RefreshCwIcon,
                variant: "outline",
                onClick: () => void listQuery.refetch(),
              },
              {
                actionKey: "register-invoice",
                label: "登记进项发票",
                icon: FilePlus2Icon,
                variant: "outline",
                mobileVisibility: "hide",
                disabled: !data.canRegisterInvoice,
                onClick: () => {
                  setPickSupplierId(
                    supplierId ?? data.suppliers[0]?.supplierId ?? ""
                  )
                  setPickSupplierOpen("purchase_invoice")
                },
              },
              {
                actionKey: "register-payment",
                label: "登记付款",
                icon: WalletCardsIcon,
                mobileVisibility: "hide",
                disabled: !data.canRegisterPayment,
                onClick: () => {
                  setPickSupplierId(
                    supplierId ?? data.suppliers[0]?.supplierId ?? ""
                  )
                  setPickSupplierOpen("payment")
                },
              },
            ]}
          />
        }
      />

      {(fromWorkspace || purchaseOrderId) && (
        <Alert>
          <AlertTitle>跨页面进入</AlertTitle>
          <AlertDescription>
            {fromWorkspace ? `来源 ${fromWorkspace}` : null}
            {purchaseOrderId ? ` · 采购单 ${purchaseOrderId}` : null}
            。完成付款核销后请返回来源页重新校验付款条件；未核销付款不满足先款要求。
            {returnTo ? (
              <>
                {" "}
                <Link className="underline" href={returnTo}>
                  返回来源
                </Link>
              </>
            ) : null}
          </AlertDescription>
        </Alert>
      )}

      {data.payablePriorityPolicy.state !== "AVAILABLE" ? (
        <Alert>
          <AlertTitle>混合自动分配不可用</AlertTitle>
          <AlertDescription>
            {data.payablePriorityPolicy.blockerMessage}
          </AlertDescription>
        </Alert>
      ) : null}

      {lastResult ? (
        <FormalActionResult
          status={
            lastResult.status === "succeeded"
              ? "succeeded"
              : lastResult.status === "unknown"
                ? "unknown"
                : lastResult.status === "blocked"
                  ? "blocked"
                  : "rejected"
          }
          title={lastResult.title}
          description={lastResult.description}
          reference={lastResult.reference ?? lastResult.operationId}
          facts={lastResult.facts}
          actions={
            lastResult.returnTo && lastResult.status === "succeeded" ? (
              <Button
                type="button"
                size="sm"
                render={<Link href={lastResult.returnTo} />}
              >
                返回来源并重查门禁
              </Button>
            ) : null
          }
        />
      ) : null}

      <MetricStrip>
        <MetricFilterItem
          label="开放应付"
          value={<MoneyValue value={data.metrics.openPayableTotal} />}
          detail="系统口径"
          active={view === "payable" && !status}
          onClick={() => {
            setPagination((p) => ({ ...p, pageIndex: 0 }))
            patchUrl({ view: "payable", status: null })
          }}
        />
        <MetricFilterItem
          label="已到期应付"
          value={<MoneyValue value={data.metrics.overduePayableTotal} />}
          detail="含逾期开放"
          active={due === "overdue"}
          onClick={() => {
            setPagination((p) => ({ ...p, pageIndex: 0 }))
            patchUrl({
              view: "payable",
              due: due === "overdue" ? null : "overdue",
            })
          }}
        />
        <MetricFilterItem
          label="待分配付款"
          value={<MoneyValue value={data.metrics.unallocatedPaymentTotal} />}
          detail="付款轨道"
          active={view === "unallocated"}
          onClick={() => {
            setPagination((p) => ({ ...p, pageIndex: 0 }))
            patchUrl({ view: "unallocated" })
          }}
        />
        <MetricFilterItem
          label="待分配进项票"
          value={<MoneyValue value={data.metrics.unallocatedInvoiceTotal} />}
          detail="与付款独立"
          active={view === "purchase_invoice"}
          onClick={() => {
            setPagination((p) => ({ ...p, pageIndex: 0 }))
            patchUrl({ view: "purchase_invoice" })
          }}
        />
        <MetricFilterItem
          label="先款门禁待满足"
          value={String(data.metrics.prepayGateBlockedCount)}
          detail="户/单数"
          active={paymentGate === "unsatisfied"}
          onClick={() => {
            setPagination((p) => ({ ...p, pageIndex: 0 }))
            patchUrl({
              view: "payable",
              paymentGate:
                paymentGate === "unsatisfied" ? null : "unsatisfied",
            })
          }}
        />
      </MetricStrip>

      <Tabs
        value={view}
        onValueChange={(v) => {
          setPagination((p) => ({ ...p, pageIndex: 0 }))
          patchUrl({ view: v })
        }}
      >
        <TabsList>
          {(
            [
              "payable",
              "payment",
              "purchase_invoice",
              "unallocated",
            ] as const
          ).map((v) => (
            <TabsTrigger key={v} value={v}>
              {VIEW_LABEL[v]}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      <ListToolbar
        search={
          <InputGroup className="max-w-md">
            <InputGroupAddon>
              <SearchIcon className="size-4" />
            </InputGroupAddon>
            <InputGroupInput
              ref={searchInputRef}
              placeholder="供应商、采购单、付款单、发票号"
              value={searchInput}
              onChange={(e) => setSearchInput(e.target.value)}
              aria-label="搜索供应商往来"
            />
          </InputGroup>
        }
        filters={
          <div className="flex flex-wrap items-end gap-2">
            <div>
              <Label className="sr-only">供应商</Label>
              <SupplierCombobox
                value={supplierId || undefined}
                onValueChange={(id) => {
                  setPagination((p) => ({ ...p, pageIndex: 0 }))
                  patchUrl({ supplierId: id || null })
                }}
                suppliers={data.suppliers.map((s) => ({
                  supplierId: s.supplierId,
                  supplierName: s.supplierName,
                  statusLabel: "可选",
                  statusTone: "neutral",
                }))}
                className="w-[12rem]"
                aria-label="供应商"
                placeholder="全部供应商"
              />
            </div>
            {view === "payable" ? (
              <>
                <div>
                  <Label className="sr-only">来源类型</Label>
                  <OptionCombobox
                    value={sourceType ?? ""}
                    onValueChange={(v) => {
                      setPagination((p) => ({ ...p, pageIndex: 0 }))
                      patchUrl({ sourceType: v || null })
                    }}
                    options={[
                      { value: "", label: "全部来源" },
                      { value: "PURCHASE_ORDER", label: "采购单" },
                      {
                        value: "SUPPLIER_SETTLEMENT",
                        label: "供应商结算单",
                      },
                    ]}
                    className="w-[9rem]"
                    size="sm"
                    allowClear={false}
                    aria-label="来源类型"
                    placeholder="全部来源"
                  />
                </div>
                <div>
                  <Label className="sr-only">状态</Label>
                  <OptionCombobox
                    value={status ?? ""}
                    onValueChange={(v) => {
                      setPagination((p) => ({ ...p, pageIndex: 0 }))
                      patchUrl({ status: v || null })
                    }}
                    options={[
                      { value: "", label: "全部状态" },
                      { value: "OPEN", label: "未结" },
                      { value: "PARTIAL", label: "部分结清" },
                      { value: "SETTLED", label: "已结清" },
                    ]}
                    className="w-[8rem]"
                    size="sm"
                    allowClear={false}
                    aria-label="状态"
                    placeholder="全部状态"
                  />
                </div>
              </>
            ) : null}
          </div>
        }
        actions={
          <details className="text-xs text-muted-foreground">
            <summary className="cursor-pointer">演示</summary>
            <div className="mt-1 flex flex-col gap-1">
              <Button
                type="button"
                size="xs"
                variant="ghost"
                onClick={() => void demoPolicy.mutateAsync("MISSING")}
              >
                策略缺失
              </Button>
              <Button
                type="button"
                size="xs"
                variant="ghost"
                onClick={() => void demoPolicy.mutateAsync("AVAILABLE")}
              >
                策略恢复
              </Button>
              <Button
                type="button"
                size="xs"
                variant="ghost"
                onClick={() => void demoPerm.mutateAsync("revoke")}
              >
                收回权限
              </Button>
            </div>
          </details>
        }
      />

      {data.emptyReason === "FILTER_NO_RESULT" ? (
        <BusinessEmptyState
          kind="filter"
          title="当前筛选无结果"
          description={`没有符合「${data.filterSummary}」的记录。`}
          action={
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                setSearchInput("")
                patchUrl({
                  q: null,
                  supplierId: null,
                  sourceType: null,
                  status: null,
                  due: null,
                  paymentGate: null,
                })
              }}
            >
              清除筛选
            </Button>
          }
        />
      ) : data.emptyReason === "NO_DATA" ? (
        <BusinessEmptyState
          kind="no-data"
          title="当前范围尚无供应商往来记录"
          description="应付形成后刷新；可从采购单或结算来源进入。"
        />
      ) : (
        <BusinessTableFrame
          title={VIEW_LABEL[view]}
          description={`${data.filterSummary} · 金额与状态均来自系统最新数据；付款与进项票轨道独立。`}
          table={
            <>
          {view === "payable" ? (
            <DataTable
              columns={payableColumns}
              data={pageRows as PayableRow[]}
              getRowId={(r) => r.payableAccountId}
              pagination={pagination}
              onPaginationChange={setPagination}
              rowCount={data.payables.length}
              layout="flush"
              density="compact"
            />
          ) : null}
          {view === "payment" ? (
            <DataTable
              columns={paymentColumns}
              data={pageRows as PaymentRow[]}
              getRowId={(r) => r.paymentId}
              pagination={pagination}
              onPaginationChange={setPagination}
              rowCount={data.payments.length}
              layout="flush"
              density="compact"
            />
          ) : null}
          {view === "purchase_invoice" ? (
            <DataTable
              columns={invoiceColumns}
              data={pageRows as PurchaseInvoiceRow[]}
              getRowId={(r) => r.invoiceId}
              pagination={pagination}
              onPaginationChange={setPagination}
              rowCount={data.invoices.length}
              layout="flush"
              density="compact"
            />
          ) : null}
          {view === "unallocated" ? (
            <DataTable
              columns={unallocatedColumns}
              data={pageRows as UnallocatedRow[]}
              getRowId={(r) => r.id}
              pagination={pagination}
              onPaginationChange={setPagination}
              rowCount={data.unallocated.length}
              layout="flush"
              density="compact"
            />
          ) : null}
            </>
          }
        />
      )}

      <QuickPreviewSheet
        open={Boolean(previewPayableId)}
        onOpenChange={(open) => {
          if (!open) closePreview()
        }}
        title="应付预览"
        description="来源、金额、付款/收票进度与分配关系（系统最新数据）"
      >
        {detailQuery.isPending ? (
          <div className="h-40 animate-pulse rounded-xl bg-muted" />
        ) : detailQuery.data ? (
          <div className="space-y-4">
            <div>
              <h3 className="font-medium">
                {detailQuery.data.payable.supplierName}
              </h3>
              <p className="text-sm text-muted-foreground">
                {detailQuery.data.payable.sourceTypeLabel} ·{" "}
                <span className="num">
                  {detailQuery.data.payable.sourceDocumentNo}
                </span>
              </p>
            </div>
            <DescriptionList columns="two">
              <DescriptionItem>
                <DescriptionTerm>应付总额</DescriptionTerm>
                <DescriptionDetails>
                  <MoneyValue
                    value={detailQuery.data.payable.grossTotal}
                    taxBasis="gross"
                  />
                </DescriptionDetails>
              </DescriptionItem>
              <DescriptionItem>
                <DescriptionTerm>开放应付</DescriptionTerm>
                <DescriptionDetails>
                  <MoneyValue value={detailQuery.data.payable.openTotal} />
                </DescriptionDetails>
              </DescriptionItem>
              <DescriptionItem>
                <DescriptionTerm>净已付分配</DescriptionTerm>
                <DescriptionDetails>
                  <MoneyValue value={detailQuery.data.payable.settledTotal} />
                </DescriptionDetails>
              </DescriptionItem>
              <DescriptionItem>
                <DescriptionTerm>净已收票</DescriptionTerm>
                <DescriptionDetails>
                  <MoneyValue value={detailQuery.data.payable.invoicedTotal} />
                </DescriptionDetails>
              </DescriptionItem>
              <DescriptionItem>
                <DescriptionTerm>剩余可收票</DescriptionTerm>
                <DescriptionDetails>
                  <MoneyValue
                    value={detailQuery.data.payable.openInvoiceableTotal}
                  />
                </DescriptionDetails>
              </DescriptionItem>
              <DescriptionItem>
                <DescriptionTerm>状态</DescriptionTerm>
                <DescriptionDetails>
                  <BusinessStatusBadge
                    context="preview"
                    label={detailQuery.data.payable.statusLabel}
                    tone={detailQuery.data.payable.statusTone}
                  />
                </DescriptionDetails>
              </DescriptionItem>
            </DescriptionList>

            {detailQuery.data.payable.paymentGateSummary ? (
              <Alert>
                <AlertTitle>付款条件（系统校验）</AlertTitle>
                <AlertDescription>
                  {detailQuery.data.payable.paymentGateSummary.message} · 已核销{" "}
                  {detailQuery.data.payable.paymentGateSummary.allocated} / 门槛{" "}
                  {detailQuery.data.payable.paymentGateSummary.required} · 差额{" "}
                  {detailQuery.data.payable.paymentGateSummary.gap}
                </AlertDescription>
              </Alert>
            ) : null}

            <Separator />
            <div>
              <h4 className="mb-2 text-sm font-medium">应付分录</h4>
              <ul className="space-y-2 text-sm">
                {detailQuery.data.entries.map((e) => (
                  <li
                    key={e.entryId}
                    className="flex justify-between gap-2 rounded-lg border p-2"
                  >
                    <span>
                      {e.entryTypeLabel}
                      <span className="block text-xs text-muted-foreground">
                        {e.sourceLabel}
                      </span>
                    </span>
                    <MoneyValue value={e.amount} />
                  </li>
                ))}
              </ul>
            </div>
            <div>
              <h4 className="mb-2 text-sm font-medium">付款分配</h4>
              {detailQuery.data.paymentAllocations.length === 0 ? (
                <p className="text-sm text-muted-foreground">暂无</p>
              ) : (
                <ul className="space-y-1 text-sm">
                  {detailQuery.data.paymentAllocations.map((a) => (
                    <li key={a.allocationId} className="flex justify-between">
                      <span>
                        {a.action} · {a.sourceDocumentNo}
                      </span>
                      <MoneyValue value={a.amount} />
                    </li>
                  ))}
                </ul>
              )}
            </div>
            <div>
              <h4 className="mb-2 text-sm font-medium">进项票分配</h4>
              {detailQuery.data.invoiceAllocations.length === 0 ? (
                <p className="text-sm text-muted-foreground">暂无</p>
              ) : (
                <ul className="space-y-1 text-sm">
                  {detailQuery.data.invoiceAllocations.map((a) => (
                    <li key={a.allocationId} className="flex justify-between">
                      <span>
                        {a.action} · {a.sourceDocumentNo}
                      </span>
                      <MoneyValue value={a.amountGross} />
                    </li>
                  ))}
                </ul>
              )}
            </div>
            <div className="flex flex-wrap gap-2">
              {detailQuery.data.payable.sourceHref ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  render={
                    <Link href={detailQuery.data.payable.sourceHref} />
                  }
                >
                  查看来源
                  <ExternalLinkIcon className="size-3.5" />
                </Button>
              ) : null}
              <Button
                type="button"
                size="sm"
                onClick={() => {
                  const p = detailQuery.data!.payable
                  closePreview()
                  openSession({
                    track: "payment",
                    supplierId: p.supplierId,
                    preselectPayableAccountId: p.payableAccountId,
                    purchaseOrderId:
                      p.sourceType === "PURCHASE_ORDER"
                        ? p.sourceDocumentId
                        : undefined,
                    returnTo,
                    fromWorkspace,
                  })
                }}
              >
                登记付款
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => {
                  const p = detailQuery.data!.payable
                  closePreview()
                  openSession({
                    track: "purchase_invoice",
                    supplierId: p.supplierId,
                    preselectPayableAccountId: p.payableAccountId,
                  })
                }}
              >
                登记进项发票
              </Button>
            </div>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">未找到应付详情</p>
        )}
      </QuickPreviewSheet>

      <Dialog
        open={pickSupplierOpen != null}
        onOpenChange={(open) => {
          if (!open) setPickSupplierOpen(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {pickSupplierOpen === "payment"
                ? "选择供应商 · 登记付款"
                : "选择供应商 · 登记进项发票"}
            </DialogTitle>
            <DialogDescription>
              核销会话创建后锁定供应商；不同供应商目标不会进入同一核销池。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label>供应商</Label>
            <SupplierCombobox
              value={pickSupplierId || undefined}
              onValueChange={(id) => setPickSupplierId(id ?? "")}
              suppliers={data.suppliers.map((s) => ({
                supplierId: s.supplierId,
                supplierName: s.supplierName,
                statusLabel: "可选",
                statusTone: "neutral",
              }))}
              className="w-full"
              aria-label="供应商"
              placeholder="选择供应商"
            />
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setPickSupplierOpen(null)}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={!pickSupplierId || !pickSupplierOpen}
              onClick={() => {
                if (!pickSupplierOpen || !pickSupplierId) return
                setPickSupplierOpen(null)
                openSession({
                  track: pickSupplierOpen,
                  supplierId: pickSupplierId,
                  returnTo,
                  fromWorkspace,
                  purchaseOrderId,
                })
              }}
            >
              进入核销会话
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {reverseTarget ? (
        <Dialog open onOpenChange={() => setReverseTarget(null)}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>
                {reverseTarget.kind === "payment" ? "付款冲正" : "进项红票"}
              </DialogTitle>
              <DialogDescription>
                原单 {reverseTarget.no} 将保留；请填写业务原因。
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-3">
              <div className="space-y-1">
                <Label>原因</Label>
                <Textarea
                  value={reverseReason}
                  onChange={(e) => setReverseReason(e.target.value)}
                  placeholder="至少 2 个字"
                />
              </div>
              {reverseTarget.kind === "invoice" ? (
                <div className="space-y-1">
                  <Label>红票号码</Label>
                  <InputGroup>
                    <InputGroupInput
                      value={redInvoiceNo}
                      onChange={(e) => setRedInvoiceNo(e.target.value)}
                    />
                  </InputGroup>
                </div>
              ) : null}
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setReverseTarget(null)}
              >
                取消
              </Button>
              <Button
                type="button"
                disabled={
                  reverseReason.trim().length < 2 ||
                  reversePayment.isPending ||
                  reverseInvoice.isPending
                }
                onClick={async () => {
                  const key = `w12_rev_${reverseTarget.kind}_${reverseTarget.id}_${Date.now()}`
                  let res: FormalSubmitResult
                  if (reverseTarget.kind === "payment") {
                    res = await reversePayment.mutateAsync({
                      paymentId: reverseTarget.id,
                      reason: reverseReason,
                      idempotencyKey: key,
                    })
                  } else {
                    res = await reverseInvoice.mutateAsync({
                      invoiceId: reverseTarget.id,
                      reason: reverseReason,
                      redInvoiceNo: redInvoiceNo || `R${Date.now()}`,
                      idempotencyKey: key,
                    })
                  }
                  setLastResult(res)
                  setReverseTarget(null)
                  setReverseReason("")
                }}
              >
                确认追加反向记录
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      ) : null}
    </div>
  )
}
