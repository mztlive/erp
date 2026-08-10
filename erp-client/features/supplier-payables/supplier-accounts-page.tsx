"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"
import {
    ExternalLinkIcon,
    FilePlus2Icon,
    RefreshCwIcon,
    SearchIcon,
    WalletCardsIcon,
    XIcon,
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
    PageScaffold,
    QuickPreviewSheet,
} from "@/components/business"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { workspaceLabel } from "@/lib/ui-text"
import type { WorkspaceId } from "@/lib/workspace-registry"

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

/** 工具条摘要去掉「N 条」计数：分页条已展示「共 N 条」，避免重复 */
function stripSummaryCount(summary: string): string {
    return summary.replace(/ · [\d,]+ 条$/, "")
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
    const sourceType =
        (searchParams.get("sourceType") as
            | "PURCHASE_ORDER"
            | "SUPPLIER_SETTLEMENT"
            | null) ?? undefined
    const status = searchParams.get("status") ?? undefined
    const due =
        (searchParams.get("due") as
            | "not_due"
            | "due_today"
            | "overdue"
            | "all"
            | null) ?? undefined
    const paymentGate =
        (searchParams.get("paymentGate") as
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
    // D23：分页写 URL（page），URL 最小化——第 1 页省略参数；本地不再持有分页副本。
    // 排序保留本地实现（服务端列表无排序参数，仅应付视图做客户端排序），记录在案。
    const pageParam = searchParams.get("page")
    const pageIndex = React.useMemo(() => {
        const n = pageParam ? Number.parseInt(pageParam, 10) : 1
        return Number.isFinite(n) && n >= 1 ? n - 1 : 0
    }, [pageParam])
    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex, pageSize: 20 }),
        [pageIndex],
    )
    const [previewPayableId, setPreviewPayableId] = React.useState<
        string | null
    >(detailId ?? null)
    const [session, setSession] = React.useState<SessionState | null>(null)
    const [pickSupplierOpen, setPickSupplierOpen] =
        React.useState<null | AllocationTrack>(null)
    const [pickSupplierId, setPickSupplierId] = React.useState("")
    const [reverseTarget, setReverseTarget] = React.useState<
        | { kind: "payment"; id: string; no: string }
        | { kind: "invoice"; id: string; no: string }
        | null
    >(null)
    const [reverseReason, setReverseReason] = React.useState("")
    const [redInvoiceNo, setRedInvoiceNo] = React.useState("")
    const [lastResult, setLastResult] =
        React.useState<FormalSubmitResult | null>(null)
    const [sorting, setSorting] = React.useState<SortingState>([])
    const trackFilter =
        (searchParams.get("track") as
            | "payment"
            | "purchase_invoice"
            | "all"
            | null) ?? "all"
    const deepLinkHandled = React.useRef(false)

    const query = React.useMemo(
        () => ({
            view,
            q: qParam || undefined,
            supplierId,
            sourceType:
                sourceType === "PURCHASE_ORDER" ||
                sourceType === "SUPPLIER_SETTLEMENT"
                    ? sourceType
                    : undefined,
            status,
            due:
                due === "not_due" || due === "due_today" || due === "overdue"
                    ? due
                    : undefined,
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
        ],
    )

    const listQuery = useSupplierAccountsQuery(query)
    const detailQuery = usePayableDetailQuery(previewPayableId)
    const reversePayment = useReversePaymentMutation()
    const reverseInvoice = useReverseInvoiceMutation()

    const data = listQuery.data

    const sortedPayables = React.useMemo(() => {
        const list = data?.payables ? [...data.payables] : []
        if (sorting.length === 0) return list
        const { id, desc } = sorting[0]!
        const dir = desc ? -1 : 1
        const key = (p: PayableRow): string | number => {
            if (id === "due") return p.dueDate
            if (id === "supplier") return p.supplierName
            if (id === "amounts") return Number(p.openTotal)
            if (id === "tracks") return Number(p.settledTotal)
            return p.payableAccountId
        }
        return list.sort((a, b) => {
            const ka = key(a)
            const kb = key(b)
            if (typeof ka === "number" && typeof kb === "number") {
                return (ka - kb) * dir
            }
            return String(ka).localeCompare(String(kb), "zh-CN") * dir
        })
    }, [data?.payables, sorting])

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean },
    ) {
        patchSearchParams(
            { router, pathname, searchParams, view },
            patch,
            options,
        )
    }

    // P4：清除=清全部筛选参数并回第 1 页；保留 view（视图类参数）/排序/导航上下文
    // （session/detailId/returnTo/from 等）。语义写进按钮 tooltip（D23）。
    // 参数命名与其它页不同（detailId 对应 preview、session 为核销会话）属历史约定，
    // 为向后兼容保留，不做重命名（D23 记录在案）。
    const hasActiveFilters = Boolean(
        qParam ||
        supplierId ||
        sourceType ||
        (due && due !== "all") ||
        (paymentGate && paymentGate !== "all") ||
        purchaseOrderId ||
        (trackFilter && trackFilter !== "all"),
    )
    function clearFilters() {
        setSearchInput("")
        patchUrl({
            q: null,
            supplierId: null,
            sourceType: null,
            status: null,
            due: null,
            paymentGate: null,
            purchaseOrderId: null,
            track: null,
            page: null,
        })
    }

    // D23：分页变更只写 URL page（第 1 页省略），分页状态由 URL 派生
    const handlePaginationChange = (next: PaginationState) => {
        patchUrl(
            { page: next.pageIndex === 0 ? null : String(next.pageIndex + 1) },
            { replace: true },
        )
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
                    p.sourceDocumentId === purchaseOrderId,
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
                    { replace: true },
                )
            }
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [
        data?.queriedAt,
        fromWorkspace,
        purchaseOrderId,
        supplierId,
        sessionTrack,
    ])

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
            { replace: true },
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
            { replace: true },
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
                        <span className="truncate font-medium">
                            {row.original.supplierName}
                        </span>
                        <span className="shrink-0 text-muted-foreground">
                            ·
                        </span>
                        <span className="truncate text-xs text-muted-foreground">
                            {row.original.sourceTypeLabel} ·{" "}
                            <span className="num">
                                {row.original.sourceDocumentNo}
                            </span>
                        </span>
                    </div>
                ),
            },
            {
                id: "amounts",
                header: "应付（含税）/ 开放（含税）",
                meta: {
                    label: "金额",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="flex items-center justify-end gap-1 text-end text-sm">
                        <MoneyValue value={row.original.grossTotal} />
                        <span className="text-xs text-muted-foreground">
                            / 开放
                        </span>
                        <MoneyValue
                            className="text-xs"
                            value={row.original.openTotal}
                        />
                    </div>
                ),
            },
            {
                id: "tracks",
                header: "已付（净）/ 已收票（净）",
                meta: {
                    label: "进度",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="flex items-center justify-end gap-1.5 text-end text-xs text-muted-foreground">
                        <span>付款</span>{" "}
                        <MoneyValue value={row.original.settledTotal} />
                        <span>/ 收票</span>{" "}
                        <MoneyValue value={row.original.invoicedTotal} />
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
                        row.original.paymentGateSummary.state !==
                            "NOT_APPLICABLE" ? (
                            <span className="text-tiny text-muted-foreground">
                                先款条件{" "}
                                {row.original.paymentGateSummary.state ===
                                "SATISFIED"
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
                            onClick={() =>
                                openPreview(row.original.payableAccountId)
                            }
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
                                    preselectPayableAccountId:
                                        row.original.payableAccountId,
                                    purchaseOrderId:
                                        row.original.sourceType ===
                                        "PURCHASE_ORDER"
                                            ? row.original.sourceDocumentId
                                            : undefined,
                                    returnTo,
                                    fromWorkspace,
                                })
                            }
                            disabled={!data?.canRegisterPayment}
                            title={
                                data?.canRegisterPayment
                                    ? undefined
                                    : "当前无付款登记/核销权限"
                            }
                        >
                            核销付款
                        </Button>
                    </div>
                ),
            },
        ],
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [data?.canRegisterPayment, returnTo, fromWorkspace],
    )

    const paymentColumns = React.useMemo<ColumnDef<PaymentRow>[]>(
        () => [
            {
                id: "doc",
                header: "付款单",
                meta: { label: "付款单", width: "reference" },
                cell: ({ row }) => (
                    <div className="text-sm">
                        <div className="num font-medium">
                            {row.original.paymentNo}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.supplierName}
                        </div>
                    </div>
                ),
            },
            {
                id: "amount",
                header: "金额 / 未分配",
                meta: {
                    label: "金额",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="text-end text-sm">
                        <MoneyValue
                            value={row.original.amount}
                            taxBasis="gross"
                        />
                        <div className="text-xs text-muted-foreground">
                            未分配{" "}
                            <MoneyValue
                                value={row.original.unallocatedAmount}
                            />
                        </div>
                    </div>
                ),
            },
            {
                id: "bank",
                header: "银行引用",
                meta: { label: "银行", width: "default" },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.bankReferenceMasked}
                    </span>
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
                                ? "已确认不可编辑；纠错请冲正"
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
                        {formatDateTime(
                            row.original.paidAt,
                            "full",
                            "passthrough",
                        )}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
                cell: ({ row }) => (
                    <div className="flex flex-wrap justify-end gap-1">
                        {row.original.allowedActions.includes(
                            "CONTINUE_ALLOCATE",
                        ) ? (
                            <Button
                                type="button"
                                size="xs"
                                onClick={() =>
                                    openSession({
                                        track: "payment",
                                        supplierId: row.original.supplierId,
                                        existingPaymentId:
                                            row.original.paymentId,
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
        [returnTo, fromWorkspace],
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
                                {row.original.invoiceCode}-
                                {row.original.invoiceNo}
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
                meta: {
                    label: "金额",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="text-end text-sm">
                        <MoneyValue
                            value={row.original.grossAmount}
                            taxBasis="gross"
                        />
                        <div className="text-xs text-muted-foreground">
                            未分配{" "}
                            <MoneyValue
                                value={row.original.unallocatedAmount}
                            />
                        </div>
                    </div>
                ),
            },
            {
                id: "alloc",
                header: "净已分配",
                meta: {
                    label: "分配",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
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
                        {row.original.allowedActions.includes(
                            "CONTINUE_ALLOCATE",
                        ) ? (
                            <Button
                                type="button"
                                size="xs"
                                onClick={() =>
                                    openSession({
                                        track: "purchase_invoice",
                                        supplierId: row.original.supplierId,
                                        existingInvoiceId:
                                            row.original.invoiceId,
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
                                    setRedInvoiceNo(
                                        `R${row.original.invoiceNo}`,
                                    )
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
        [],
    )

    const unallocatedColumns = React.useMemo<ColumnDef<UnallocatedRow>[]>(
        () => [
            {
                id: "track",
                header: "轨道",
                meta: { label: "轨道", width: "default" },
                cell: ({ row }) => (
                    <Badge
                        variant={
                            row.original.track === "payment"
                                ? "warning"
                                : "info"
                        }
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
                        <div className="num font-medium">
                            {row.original.documentNo}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.supplierName}
                        </div>
                    </div>
                ),
            },
            {
                id: "amount",
                header: "未分配余额",
                meta: {
                    label: "余额",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="text-end">
                        <MoneyValue
                            value={row.original.unallocatedAmount}
                            taxBasis="gross"
                        />
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
                        (p) => p.paymentNo === row.original.documentNo,
                    )
                    const invoice = data?.invoices.find(
                        (p) =>
                            `${p.invoiceCode}-${p.invoiceNo}` ===
                            row.original.documentNo,
                    )
                    const resolved =
                        row.original.track === "payment" ? payment : invoice
                    return (
                        <Button
                            type="button"
                            size="xs"
                            disabled={!resolved}
                            title={
                                resolved
                                    ? undefined
                                    : "未找到原付款/发票，请回到对应视图操作"
                            }
                            onClick={() =>
                                openSession({
                                    track: row.original.track,
                                    supplierId: row.original.supplierId,
                                    existingPaymentId:
                                        row.original.track === "payment"
                                            ? payment?.paymentId
                                            : undefined,
                                    existingInvoiceId:
                                        row.original.track ===
                                        "purchase_invoice"
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
        [data?.payments, data?.invoices],
    )

    if (session) {
        return (
            <PageScaffold>
                <AllocationSession
                    {...session}
                    onClose={closeSession}
                    onGoToInvoiceView={() => {
                        closeSession()
                        patchUrl({ view: "purchase_invoice" })
                    }}
                    onCompleted={(result) => {
                        setLastResult(result)
                    }}
                />
            </PageScaffold>
        )
    }

    if (listQuery.isPending && !data) {
        return (
            <PageScaffold density="compact">
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="grid grid-cols-2 gap-3 md:grid-cols-5">
                    {Array.from({ length: 5 }).map((_, i) => (
                        <div
                            key={i}
                            className="h-20 animate-pulse rounded-lg bg-muted"
                        />
                    ))}
                </div>
                <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (listQuery.isError && !data) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="供应商往来加载失败"
                    error={listQuery.error}
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
            </PageScaffold>
        )
    }

    if (!data) return null

    if (!data.moduleAllowed) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-scope"
                    title="无供应商往来权限"
                    description="权限已收回或未授权。敏感字段与导出结果已清除，不能提交。"
                />
            </PageScaffold>
        )
    }

    if (!data.hasDataScope) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色未配置供应商往来范围"
                    description="不能显示为 0 元应付。请联系管理员配置组织/供应商范围后再查询。"
                />
            </PageScaffold>
        )
    }

    const rows =
        view === "payable"
            ? sortedPayables
            : view === "payment"
              ? data.payments
              : view === "purchase_invoice"
                ? data.invoices
                : trackFilter !== "all"
                  ? data.unallocated.filter((u) => u.track === trackFilter)
                  : data.unallocated

    const pageRows = rows.slice(
        pagination.pageIndex * pagination.pageSize,
        pagination.pageIndex * pagination.pageSize + pagination.pageSize,
    )

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="供应商往来"
                breadcrumbs={[
                    {
                        id: "fin",
                        label: "财务",
                        href: "/finance/supplier-accounts",
                    },
                    { id: "ap", label: "供应商往来", current: true },
                ]}
                metadata={
                    <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
                        <DataFreshness
                            updatedAt={new Date(data.queriedAt).toLocaleString(
                                "zh-CN",
                            )}
                            dateTime={data.queriedAt}
                            label="数据更新于"
                        />
                        <p className="text-xs text-muted-foreground">
                            {data.payablePriorityPolicy.state === "AVAILABLE"
                                ? "混合来源按系统优先级分配"
                                : data.payablePriorityPolicy.state === "MISSING"
                                  ? "混合来源分配规则未配置"
                                  : "混合来源分配规则已更新"}
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
                                variant: "ghost",
                                className:
                                    "text-muted-foreground hover:text-foreground",
                                onClick: () => void listQuery.refetch(),
                            },
                            {
                                actionKey: "register-invoice",
                                label: "登记进项发票",
                                icon: FilePlus2Icon,
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled: !data.canRegisterInvoice,
                                title: data.canRegisterInvoice
                                    ? undefined
                                    : "当前无进项发票登记权限",
                                onClick: () => {
                                    setPickSupplierId(
                                        supplierId ??
                                            data.suppliers[0]?.supplierId ??
                                            "",
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
                                title: data.canRegisterPayment
                                    ? undefined
                                    : "当前无付款登记权限",
                                onClick: () => {
                                    setPickSupplierId(
                                        supplierId ??
                                            data.suppliers[0]?.supplierId ??
                                            "",
                                    )
                                    setPickSupplierOpen("payment")
                                },
                            },
                            {
                                actionKey: "settle",
                                label: "去对账结算",
                                variant: "outline",
                                mobileVisibility: "hide",
                                onClick: () => {
                                    const qs = searchParams.toString()
                                    const selfHref = qs
                                        ? `${pathname}?${qs}`
                                        : pathname
                                    const params = new URLSearchParams()
                                    if (supplierId)
                                        params.set("supplierId", supplierId)
                                    params.set("returnTo", selfHref)
                                    router.push(
                                        `/supplier-api/settlements?${params.toString()}`,
                                    )
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
                        {fromWorkspace
                            ? `来源 ${workspaceLabel(fromWorkspace as WorkspaceId)}`
                            : null}
                        {purchaseOrderId
                            ? ` · 采购单 ${purchaseOrderId}`
                            : null}
                        。完成付款核销后请返回来源页重新校验先款条件；未核销付款不满足先款要求。
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
                <div className="relative">
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
                        reference={
                            lastResult.reference ?? lastResult.operationId
                        }
                        facts={lastResult.facts}
                        actions={
                            lastResult.returnTo &&
                            lastResult.status === "succeeded" ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    render={<Link href={lastResult.returnTo} />}
                                >
                                    返回来源并重新校验先款条件
                                </Button>
                            ) : null
                        }
                    />
                    <Button
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        className="absolute top-2 right-2"
                        aria-label="收起结果"
                        onClick={() => setLastResult(null)}
                    >
                        <XIcon aria-hidden="true" />
                    </Button>
                </div>
            ) : null}

            {/* 指标 toggle 取消语义保留（D23）：再次点击已激活指标即取消该筛选
          （due/paymentGate 置回 all、view 回 payable）；指标/视图/筛选变更均回第 1 页（P6）。 */}
            <MetricStrip>
                <MetricFilterItem
                    label="开放应付"
                    value={<MoneyValue value={data.metrics.openPayableTotal} />}
                    detail="系统口径"
                    active={view === "payable" && !status}
                    onClick={() => {
                        patchUrl({ view: "payable", status: null, page: null })
                    }}
                />
                <MetricFilterItem
                    label="已到期应付"
                    value={
                        <MoneyValue value={data.metrics.overduePayableTotal} />
                    }
                    detail="含逾期开放"
                    active={due === "overdue"}
                    onClick={() => {
                        patchUrl({
                            view: "payable",
                            due: due === "overdue" ? null : "overdue",
                            page: null,
                        })
                    }}
                />
                <MetricFilterItem
                    label="待分配付款"
                    value={
                        <MoneyValue
                            value={data.metrics.unallocatedPaymentTotal}
                        />
                    }
                    detail="付款轨道"
                    active={view === "unallocated" && trackFilter === "payment"}
                    onClick={() => {
                        patchUrl({
                            view: "unallocated",
                            track: "payment",
                            page: null,
                        })
                    }}
                />
                <MetricFilterItem
                    label="待分配进项票"
                    value={
                        <MoneyValue
                            value={data.metrics.unallocatedInvoiceTotal}
                        />
                    }
                    detail="与付款独立"
                    active={
                        view === "unallocated" &&
                        trackFilter === "purchase_invoice"
                    }
                    onClick={() => {
                        patchUrl({
                            view: "unallocated",
                            track: "purchase_invoice",
                            page: null,
                        })
                    }}
                />
                <MetricFilterItem
                    label="先款条件待满足"
                    value={String(data.metrics.prepayGateBlockedCount)}
                    detail="户/单数"
                    active={paymentGate === "unsatisfied"}
                    onClick={() => {
                        patchUrl({
                            view: "payable",
                            paymentGate:
                                paymentGate === "unsatisfied"
                                    ? null
                                    : "unsatisfied",
                            page: null,
                        })
                    }}
                />
            </MetricStrip>

            <Tabs
                value={view}
                onValueChange={(v) => {
                    patchUrl({ view: v, page: null })
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

            <BusinessTableFrame
                title={VIEW_LABEL[view]}
                description={`${stripSummaryCount(data.filterSummary)} · 金额与状态均来自系统最新数据；付款与进项票轨道独立。`}
                toolbar={
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
                                    onChange={(e) =>
                                        setSearchInput(e.target.value)
                                    }
                                    aria-label="搜索供应商往来"
                                />
                            </InputGroup>
                        }
                        filters={
                            <div className="flex flex-wrap items-end gap-2">
                                <div>
                                    <Label className="sr-only">供应商</Label>
                                    <SupplierSearchCombobox
                                        value={supplierId || undefined}
                                        onValueChange={(id) => {
                                            patchUrl({
                                                supplierId: id || null,
                                                page: null,
                                            })
                                        }}
                                        purpose="filter"
                                        className="w-[12rem]"
                                        aria-label="供应商"
                                        placeholder="全部供应商"
                                    />
                                </div>
                                {view === "unallocated" ? (
                                    <div>
                                        <Label className="sr-only">轨道</Label>
                                        <OptionCombobox
                                            value={trackFilter}
                                            onValueChange={(v) => {
                                                patchUrl({
                                                    track:
                                                        v && v !== "all"
                                                            ? v
                                                            : null,
                                                    page: null,
                                                })
                                            }}
                                            options={[
                                                {
                                                    value: "all",
                                                    label: "全部轨道",
                                                },
                                                {
                                                    value: "payment",
                                                    label: "付款",
                                                },
                                                {
                                                    value: "purchase_invoice",
                                                    label: "进项票",
                                                },
                                            ]}
                                            className="w-36"
                                            size="sm"
                                            allowClear={false}
                                            aria-label="轨道"
                                            placeholder="轨道"
                                        />
                                    </div>
                                ) : null}
                                {view === "payable" ? (
                                    <>
                                        <div>
                                            <Label className="sr-only">
                                                来源类型
                                            </Label>
                                            <OptionCombobox
                                                value={sourceType ?? ""}
                                                onValueChange={(v) => {
                                                    patchUrl({
                                                        sourceType: v || null,
                                                        page: null,
                                                    })
                                                }}
                                                options={[
                                                    {
                                                        value: "",
                                                        label: "全部来源",
                                                    },
                                                    {
                                                        value: "PURCHASE_ORDER",
                                                        label: "采购单",
                                                    },
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
                                            <Label className="sr-only">
                                                状态
                                            </Label>
                                            <OptionCombobox
                                                value={status ?? ""}
                                                onValueChange={(v) => {
                                                    patchUrl({
                                                        status: v || null,
                                                        page: null,
                                                    })
                                                }}
                                                options={[
                                                    {
                                                        value: "",
                                                        label: "全部状态",
                                                    },
                                                    {
                                                        value: "OPEN",
                                                        label: "未结",
                                                    },
                                                    {
                                                        value: "PARTIAL",
                                                        label: "部分结清",
                                                    },
                                                    {
                                                        value: "SETTLED",
                                                        label: "已结清",
                                                    },
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
                            hasActiveFilters ? (
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    onClick={clearFilters}
                                    title="清除全部筛选条件，保留当前视图与排序"
                                >
                                    清除筛选
                                </Button>
                            ) : null
                        }
                    />
                }
                table={
                    data.emptyReason === "FILTER_NO_RESULT" ? (
                        <BusinessEmptyState
                            kind="filter"
                            title="当前筛选无结果"
                            description={`没有符合「${stripSummaryCount(data.filterSummary)}」的记录。`}
                            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                            action={
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    className="rounded-lg shadow-none"
                                    onClick={clearFilters}
                                    title="清除全部筛选条件，保留当前视图与排序"
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
                            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                        />
                    ) : (
                        <>
                            {view === "payable" ? (
                                <DataTable
                                    columns={payableColumns}
                                    data={pageRows as PayableRow[]}
                                    getRowId={(r) => r.payableAccountId}
                                    pagination={pagination}
                                    onPaginationChange={handlePaginationChange}
                                    sorting={sorting}
                                    onSortingChange={setSorting}
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
                                    onPaginationChange={handlePaginationChange}
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
                                    onPaginationChange={handlePaginationChange}
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
                                    onPaginationChange={handlePaginationChange}
                                    rowCount={rows.length}
                                    layout="flush"
                                    density="compact"
                                />
                            ) : null}
                        </>
                    )
                }
            />

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
                                        value={
                                            detailQuery.data.payable.grossTotal
                                        }
                                        taxBasis="gross"
                                    />
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>开放应付</DescriptionTerm>
                                <DescriptionDetails>
                                    <MoneyValue
                                        value={
                                            detailQuery.data.payable.openTotal
                                        }
                                    />
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>净已付分配</DescriptionTerm>
                                <DescriptionDetails>
                                    <MoneyValue
                                        value={
                                            detailQuery.data.payable
                                                .settledTotal
                                        }
                                    />
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>净已收票</DescriptionTerm>
                                <DescriptionDetails>
                                    <MoneyValue
                                        value={
                                            detailQuery.data.payable
                                                .invoicedTotal
                                        }
                                    />
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>剩余可收票</DescriptionTerm>
                                <DescriptionDetails>
                                    <MoneyValue
                                        value={
                                            detailQuery.data.payable
                                                .openInvoiceableTotal
                                        }
                                    />
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>状态</DescriptionTerm>
                                <DescriptionDetails>
                                    <BusinessStatusBadge
                                        context="preview"
                                        label={
                                            detailQuery.data.payable.statusLabel
                                        }
                                        tone={
                                            detailQuery.data.payable.statusTone
                                        }
                                    />
                                </DescriptionDetails>
                            </DescriptionItem>
                        </DescriptionList>

                        {detailQuery.data.payable.paymentGateSummary ? (
                            <Alert>
                                <AlertTitle>付款条件（系统校验）</AlertTitle>
                                <AlertDescription>
                                    {
                                        detailQuery.data.payable
                                            .paymentGateSummary.message
                                    }{" "}
                                    · 已核销{" "}
                                    {
                                        detailQuery.data.payable
                                            .paymentGateSummary.allocated
                                    }{" "}
                                    / 门槛{" "}
                                    {
                                        detailQuery.data.payable
                                            .paymentGateSummary.required
                                    }{" "}
                                    · 差额{" "}
                                    {
                                        detailQuery.data.payable
                                            .paymentGateSummary.gap
                                    }
                                </AlertDescription>
                            </Alert>
                        ) : null}

                        <Separator />
                        <div>
                            <h4 className="mb-2 text-sm font-medium">
                                应付分录
                            </h4>
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
                            <h4 className="mb-2 text-sm font-medium">
                                付款分配
                            </h4>
                            {detailQuery.data.paymentAllocations.length ===
                            0 ? (
                                <p className="text-sm text-muted-foreground">
                                    暂无
                                </p>
                            ) : (
                                <ul className="space-y-1 text-sm">
                                    {detailQuery.data.paymentAllocations.map(
                                        (a) => (
                                            <li
                                                key={a.allocationId}
                                                className="flex justify-between"
                                            >
                                                <span>
                                                    {a.action} ·{" "}
                                                    {a.sourceDocumentNo}
                                                </span>
                                                <MoneyValue value={a.amount} />
                                            </li>
                                        ),
                                    )}
                                </ul>
                            )}
                        </div>
                        <div>
                            <h4 className="mb-2 text-sm font-medium">
                                进项票分配
                            </h4>
                            {detailQuery.data.invoiceAllocations.length ===
                            0 ? (
                                <p className="text-sm text-muted-foreground">
                                    暂无
                                </p>
                            ) : (
                                <ul className="space-y-1 text-sm">
                                    {detailQuery.data.invoiceAllocations.map(
                                        (a) => (
                                            <li
                                                key={a.allocationId}
                                                className="flex justify-between"
                                            >
                                                <span>
                                                    {a.action} ·{" "}
                                                    {a.sourceDocumentNo}
                                                </span>
                                                <MoneyValue
                                                    value={a.amountGross}
                                                />
                                            </li>
                                        ),
                                    )}
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
                                        <Link
                                            href={
                                                detailQuery.data.payable
                                                    .sourceHref
                                            }
                                        />
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
                                        preselectPayableAccountId:
                                            p.payableAccountId,
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
                                        preselectPayableAccountId:
                                            p.payableAccountId,
                                    })
                                }}
                            >
                                登记进项发票
                            </Button>
                        </div>
                    </div>
                ) : detailQuery.isError ? (
                    <div className="space-y-3 p-6">
                        <p className="text-sm text-muted-foreground">
                            {getErrorMessage(
                                detailQuery.error,
                                "应付详情加载失败，请重试。",
                            )}
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
                ) : (
                    <p className="text-sm text-muted-foreground">
                        未找到应付详情
                    </p>
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
                            本次核销创建后锁定供应商；不同供应商目标不会进入同一核销池。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-2">
                        <Label>供应商</Label>
                        <SupplierSearchCombobox
                            value={pickSupplierId || undefined}
                            onValueChange={(id) => setPickSupplierId(id ?? "")}
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
                            进入本次核销
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {reverseTarget ? (
                <Dialog open onOpenChange={() => setReverseTarget(null)}>
                    <DialogContent>
                        <DialogHeader>
                            <DialogTitle>
                                {reverseTarget.kind === "payment"
                                    ? "付款冲正"
                                    : "进项红票"}
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
                                    onChange={(e) =>
                                        setReverseReason(e.target.value)
                                    }
                                    placeholder="至少 2 个字"
                                />
                            </div>
                            {reverseTarget.kind === "invoice" ? (
                                <div className="space-y-1">
                                    <Label>红票号码</Label>
                                    <InputGroup>
                                        <InputGroupInput
                                            value={redInvoiceNo}
                                            onChange={(e) =>
                                                setRedInvoiceNo(e.target.value)
                                            }
                                        />
                                    </InputGroup>
                                    {!redInvoiceNo.trim() ? (
                                        <p
                                            className="text-xs text-destructive"
                                            role="alert"
                                        >
                                            红票号码必填；红票将作为独立记录登记。
                                        </p>
                                    ) : null}
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
                                    (reverseTarget.kind === "invoice" &&
                                        !redInvoiceNo.trim()) ||
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
                                            redInvoiceNo: redInvoiceNo.trim(),
                                            idempotencyKey: key,
                                        })
                                    }
                                    setLastResult(res)
                                    setReverseTarget(null)
                                    setReverseReason("")
                                    setRedInvoiceNo("")
                                }}
                            >
                                确认追加反向记录
                            </Button>
                        </DialogFooter>
                    </DialogContent>
                </Dialog>
            ) : null}
        </PageScaffold>
    )
}
