"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { useSupplierAccountsQuery } from "@/features/supplier-payables/hooks/queries"
import {
    parsePreviewKind,
    parseView,
    parseWorkItemId,
} from "@/features/supplier-payables/lib/url-state"
import type {
    AllocationTrack,
    FormalSubmitResult,
    PayableRow,
    ReverseTarget,
    SessionState,
    SupplierAccountsQuery,
} from "@/features/supplier-payables/types"

/**
 * W12 供应商往来 · 页面级状态控制器：URL 参数解析、筛选/分页/排序、
 * 核销会话与预览开关、深链消费。所有筛选/导航状态都以 URL 为准。
 */
export function useSupplierAccountsPage() {
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
    const previewKind = parsePreviewKind(searchParams.get("previewKind"))
    const workItemId = parseWorkItemId(searchParams)
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
    >(previewKind === "payable" ? (detailId ?? null) : null)
    const [previewPaymentId, setPreviewPaymentId] = React.useState<
        string | null
    >(previewKind === "payment" ? (detailId ?? null) : null)
    const [previewRefundId, setPreviewRefundId] = React.useState<string | null>(
        previewKind === "refund" ? (detailId ?? null) : null,
    )
    const [previewReversalId, setPreviewReversalId] = React.useState<
        string | null
    >(previewKind === "reversal" ? (detailId ?? null) : null)
    const [session, setSession] = React.useState<SessionState | null>(null)
    const [pickSupplierOpen, setPickSupplierOpen] =
        React.useState<null | AllocationTrack>(null)
    const [pickSupplierId, setPickSupplierId] = React.useState("")
    const [reverseTarget, setReverseTarget] =
        React.useState<ReverseTarget | null>(null)
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

    const query = React.useMemo<SupplierAccountsQuery>(
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
        setPreviewPaymentId(null)
        setPreviewRefundId(null)
        setPreviewReversalId(null)
        setPreviewPayableId(payableAccountId)
        patchUrl(
            { detailId: payableAccountId, previewKind: null },
            { replace: true },
        )
    }

    /**
     * 打开供应商付款详情，嵌入通用审批区。
     *
     * @param paymentId 付款主键。
     */
    function openPaymentPreview(paymentId: string) {
        setPreviewPayableId(null)
        setPreviewRefundId(null)
        setPreviewReversalId(null)
        setPreviewPaymentId(paymentId)
        patchUrl(
            { detailId: paymentId, previewKind: "payment" },
            { replace: true },
        )
    }

    /**
     * 打开供应商退款详情，嵌入通用审批区。
     *
     * @param refundId 退款主键。
     */
    function openRefundPreview(refundId: string) {
        setPreviewPayableId(null)
        setPreviewPaymentId(null)
        setPreviewReversalId(null)
        setPreviewRefundId(refundId)
        patchUrl(
            { detailId: refundId, previewKind: "refund" },
            { replace: true },
        )
    }

    /**
     * 打开付款冲正详情，嵌入通用审批区。
     *
     * @param reversalId 冲正主键。
     */
    function openReversalPreview(reversalId: string) {
        setPreviewPayableId(null)
        setPreviewPaymentId(null)
        setPreviewRefundId(null)
        setPreviewReversalId(reversalId)
        patchUrl(
            { detailId: reversalId, previewKind: "reversal" },
            { replace: true },
        )
    }

    function closePreview() {
        setPreviewPayableId(null)
        setPreviewPaymentId(null)
        setPreviewRefundId(null)
        setPreviewReversalId(null)
        patchUrl({ detailId: null, previewKind: null }, { replace: true })
    }

    function openSettlements() {
        const qs = searchParams.toString()
        const selfHref = qs ? `${pathname}?${qs}` : pathname
        const params = new URLSearchParams()
        if (supplierId) params.set("supplierId", supplierId)
        params.set("returnTo", selfHref)
        router.push(`/supplier-api/settlements?${params.toString()}`)
    }

    return {
        view,
        supplierId,
        sourceType,
        status,
        due,
        paymentGate,
        purchaseOrderId,
        fromWorkspace,
        returnTo,
        trackFilter,
        searchInput,
        setSearchInput,
        searchInputRef,
        pagination,
        handlePaginationChange,
        sorting,
        setSorting,
        previewPayableId,
        previewPaymentId,
        previewRefundId,
        previewReversalId,
        workItemId,
        openPreview,
        openPaymentPreview,
        openRefundPreview,
        openReversalPreview,
        closePreview,
        session,
        openSession,
        closeSession,
        pickSupplierOpen,
        setPickSupplierOpen,
        pickSupplierId,
        setPickSupplierId,
        reverseTarget,
        setReverseTarget,
        reverseReason,
        setReverseReason,
        redInvoiceNo,
        setRedInvoiceNo,
        lastResult,
        setLastResult,
        hasActiveFilters,
        clearFilters,
        patchUrl,
        listQuery,
        data,
        sortedPayables,
        openSettlements,
    }
}
