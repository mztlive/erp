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
import {
    DUE_LABEL,
    PAYABLE_STATUS_LABEL,
    PAYMENT_GATE_LABEL,
    SOURCE_TYPE_LABEL,
    TRACK_LABEL,
} from "@/features/supplier-payables/types"
import {
    businessLabelOrPlaceholder,
    MISSING_SUPPLIER_NAME,
} from "@/features/supplier-payables/lib/display-labels"
import { missingSourceDocumentNo } from "@/features/supplier-payables/lib/related-documents"
import type {
    SupplierAppliedChip,
    SupplierFilterKey,
} from "../components/supplier-accounts-toolbar"

/**
 * W12 供应商往来 · 页面级状态控制器：URL 参数解析、筛选 Draft/Applied/UI 三层状态、
 * 分页/排序、核销会话与预览开关、深链消费。所有筛选/导航状态都以 URL 为准。
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
    const draftSessionId = searchParams.get("draftSessionId") ?? undefined
    const detailId = searchParams.get("detailId") ?? undefined
    const previewKind = parsePreviewKind(searchParams.get("previewKind"))
    const workItemId = parseWorkItemId(searchParams)
    const existingInvoiceId = searchParams.get("invoiceId") ?? undefined

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // Applied：URL 非法枚举值一律降级为默认（全部），不传给接口也不进 chip
    const validSourceType =
        sourceType === "PURCHASE_ORDER" || sourceType === "SUPPLIER_SETTLEMENT"
            ? sourceType
            : undefined
    const validStatus =
        status === "OPEN" || status === "PARTIAL" || status === "SETTLED"
            ? status
            : undefined
    const validDue =
        due === "not_due" || due === "due_today" || due === "overdue"
            ? due
            : undefined
    const validPaymentGate =
        paymentGate === "satisfied" || paymentGate === "unsatisfied"
            ? paymentGate
            : undefined
    const trackFilter =
        (searchParams.get("track") as
            | "payment"
            | "purchase_invoice"
            | "all"
            | null) ?? "all"
    const validTrack =
        trackFilter === "payment" || trackFilter === "purchase_invoice"
            ? trackFilter
            : undefined

    // UI：面板展开态只由结构化条件决定初始值；回填不得改写它
    const hasStructuredFilters = Boolean(
        supplierId ||
        validSourceType ||
        validStatus ||
        validDue ||
        validPaymentGate ||
        validTrack,
    )
    const [panelOpen, setPanelOpen] = React.useState(hasStructuredFilters)

    // Draft：受控本地草稿，变化不触发请求
    const [supplierDraft, setSupplierDraft] = React.useState<string | null>(
        supplierId ?? null,
    )
    const [sourceTypeDraft, setSourceTypeDraft] = React.useState<
        "PURCHASE_ORDER" | "SUPPLIER_SETTLEMENT" | "all"
    >(validSourceType ?? "all")
    const [statusDraft, setStatusDraft] = React.useState<
        "OPEN" | "PARTIAL" | "SETTLED" | "all"
    >(validStatus ?? "all")
    const [dueDraft, setDueDraft] = React.useState<
        "not_due" | "due_today" | "overdue" | "all"
    >(validDue ?? "all")
    const [paymentGateDraft, setPaymentGateDraft] = React.useState<
        "satisfied" | "unsatisfied" | "all"
    >(validPaymentGate ?? "all")
    const [trackDraft, setTrackDraft] = React.useState<AllocationTrack | "all">(
        validTrack ?? "all",
    )

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
    const previewPayableId =
        previewKind === "payable" && detailId ? detailId : null
    const previewPaymentId =
        previewKind === "payment" && detailId ? detailId : null
    const previewRefundId =
        previewKind === "refund" && detailId ? detailId : null
    const previewReversalId =
        previewKind === "reversal" && detailId ? detailId : null
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
    const deepLinkHandled = React.useRef(false)

    const query = React.useMemo<SupplierAccountsQuery>(
        () => ({
            view,
            q: qParam.trim() || undefined,
            supplierId,
            sourceType: validSourceType,
            status: validStatus,
            due: validDue,
            paymentGate: validPaymentGate,
            purchaseOrderId,
        }),
        [
            view,
            qParam,
            supplierId,
            validSourceType,
            validStatus,
            validDue,
            validPaymentGate,
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

    const patchUrl = React.useCallback(
        (
            patch: Record<string, string | null | undefined>,
            options?: { replace?: boolean; scroll?: boolean },
        ) => {
            patchSearchParams(
                { router, pathname, searchParams, view },
                patch,
                options,
            )
        },
        [pathname, router, searchParams, view],
    )

    // P4：清除=清全部筛选参数并回第 1 页；保留 view（视图类参数）/排序/导航上下文
    // （session/detailId/returnTo/from 等）。
    const hasActiveFilters = Boolean(
        qParam.trim() ||
        supplierId ||
        validSourceType ||
        validStatus ||
        validDue ||
        validPaymentGate ||
        validTrack ||
        purchaseOrderId,
    )

    /** 单一提交入口：收起态 Enter / 搜索框尾部箭头 / 展开态「应用全部筛选」共用。 */
    const applyFilters = React.useCallback(() => {
        patchUrl(
            {
                q: searchInput.trim() || null,
                supplierId: supplierDraft || null,
                sourceType: sourceTypeDraft === "all" ? null : sourceTypeDraft,
                status: statusDraft === "all" ? null : statusDraft,
                due: dueDraft === "all" ? null : dueDraft,
                paymentGate:
                    paymentGateDraft === "all" ? null : paymentGateDraft,
                track: trackDraft === "all" ? null : trackDraft,
                page: null,
            },
            { replace: true, scroll: false },
        )
        setPanelOpen(false)
    }, [
        // eslint-disable-next-line react-hooks/exhaustive-deps
        patchUrl,
        searchInput,
        supplierDraft,
        sourceTypeDraft,
        statusDraft,
        dueDraft,
        paymentGateDraft,
        trackDraft,
    ])

    /** 仅清除「更多筛选」结构化条件；保留关键词与来源锁定采购单，面板保持展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setSupplierDraft(null)
        setSourceTypeDraft("all")
        setStatusDraft("all")
        setDueDraft("all")
        setPaymentGateDraft("all")
        setTrackDraft("all")
        patchUrl(
            {
                supplierId: null,
                sourceType: null,
                status: null,
                due: null,
                paymentGate: null,
                track: null,
                page: null,
            },
            { replace: true, scroll: false },
        )
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [patchUrl])

    /** 移除单个已生效条件并回填对应草稿。 */
    const removeFilter = React.useCallback(
        (key: SupplierFilterKey) => {
            if (key === "q") setSearchInput("")
            if (key === "supplierId") setSupplierDraft(null)
            if (key === "sourceType") setSourceTypeDraft("all")
            if (key === "status") setStatusDraft("all")
            if (key === "due") setDueDraft("all")
            if (key === "paymentGate") setPaymentGateDraft("all")
            if (key === "track") setTrackDraft("all")
            patchUrl(
                { [key]: null, page: null },
                { replace: true, scroll: false },
            )
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [patchUrl],
    )

    /** 清空全部：草稿、面板、URL 筛选参数与分页同时重置；保留视图/排序/导航上下文。 */
    const clearFilters = React.useCallback(() => {
        setSearchInput("")
        setSupplierDraft(null)
        setSourceTypeDraft("all")
        setStatusDraft("all")
        setDueDraft("all")
        setPaymentGateDraft("all")
        setTrackDraft("all")
        setPanelOpen(false)
        patchUrl(
            {
                q: null,
                supplierId: null,
                sourceType: null,
                status: null,
                due: null,
                paymentGate: null,
                purchaseOrderId: null,
                track: null,
                page: null,
            },
            { replace: true, scroll: false },
        )
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [patchUrl])

    /** 已生效条件全部显性为 chip（含来源锁定 supplierId / purchaseOrderId）。 */
    const appliedChips = React.useMemo<readonly SupplierAppliedChip[]>(() => {
        const chips: SupplierAppliedChip[] = []
        const trimmedQ = qParam.trim()
        if (trimmedQ) chips.push({ key: "q", label: `搜索：${trimmedQ}` })
        if (supplierId) {
            const supplierName = (data?.suppliers ?? []).find(
                (item) => item.supplierId === supplierId,
            )?.supplierName
            chips.push({
                key: "supplierId",
                label: `供应商：${businessLabelOrPlaceholder(
                    supplierName,
                    supplierId,
                    MISSING_SUPPLIER_NAME,
                )}`,
            })
        }
        if (validSourceType) {
            chips.push({
                key: "sourceType",
                label: `来源类型：${SOURCE_TYPE_LABEL[validSourceType]}`,
            })
        }
        if (validStatus) {
            chips.push({
                key: "status",
                label: `状态：${PAYABLE_STATUS_LABEL[validStatus]}`,
            })
        }
        if (validDue) {
            chips.push({ key: "due", label: `到期：${DUE_LABEL[validDue]}` })
        }
        if (validPaymentGate) {
            chips.push({
                key: "paymentGate",
                label: `先款条件：${PAYMENT_GATE_LABEL[validPaymentGate]}`,
            })
        }
        if (validTrack) {
            chips.push({
                key: "track",
                label: `轨道：${TRACK_LABEL[validTrack]}`,
            })
        }
        if (purchaseOrderId) {
            const purchaseOrderNo = (data?.payables ?? []).find(
                (item) => item.sourceDocumentId === purchaseOrderId,
            )?.sourceDocumentNo
            chips.push({
                key: "purchaseOrderId",
                label: `采购单：${businessLabelOrPlaceholder(
                    purchaseOrderNo,
                    purchaseOrderId,
                    missingSourceDocumentNo("PURCHASE_ORDER"),
                )}`,
            })
        }
        return chips
    }, [
        data?.payables,
        data?.suppliers,
        purchaseOrderId,
        qParam,
        supplierId,
        validDue,
        validPaymentGate,
        validSourceType,
        validStatus,
        validTrack,
    ])

    // D23：分页变更只写 URL page（第 1 页省略），分页状态由 URL 派生
    const handlePaginationChange = (next: PaginationState) => {
        patchUrl(
            { page: next.pageIndex === 0 ? null : String(next.pageIndex + 1) },
            { replace: true },
        )
    }

    /** URL 回填：同步草稿；面板展开态不回填重置。 */
    React.useEffect(() => {
        setSearchInput(qParam.trim())
        setSupplierDraft(supplierId ?? null)
        setSourceTypeDraft(validSourceType ?? "all")
        setStatusDraft(validStatus ?? "all")
        setDueDraft(validDue ?? "all")
        setPaymentGateDraft(validPaymentGate ?? "all")
        setTrackDraft(validTrack ?? "all")
    }, [
        qParam,
        supplierId,
        validSourceType,
        validStatus,
        validDue,
        validPaymentGate,
        validTrack,
    ])

    /** `/` 聚焦搜索框；输入框/文本域/弹层打开时不抢焦点。 */
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            ) {
                return
            }
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
            if (
                document.querySelector('[role="dialog"], [data-slot="sheet"]')
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    // Deep-link from W01/W08/W09: open payment session with PO preselected
    React.useEffect(() => {
        if (deepLinkHandled.current) return
        if (!data?.moduleAllowed) return
        if (sessionTrack === "payment" || sessionTrack === "purchase_invoice") {
            if (supplierId) {
                deepLinkHandled.current = true
                setSession({
                    track: sessionTrack,
                    supplierId,
                    draftSessionId,
                    purchaseOrderId,
                    returnTo,
                    fromWorkspace,
                    existingInvoiceId,
                    preselectPayableAccountId:
                        previewKind === "payable" ? detailId : undefined,
                })
                return
            }
        }
        // from=W01/W08/W09 without session: auto open payment if we can resolve supplier
        if (
            (fromWorkspace === "W01" ||
                fromWorkspace === "W08" ||
                fromWorkspace === "W09") &&
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
                draftSessionId: next.draftSessionId ?? null,
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
                draftSessionId: null,
                invoiceId: null,
            },
            { replace: true },
        )
    }

    /**
     * 核销会话主键回写：会话视图加载后携带新生成的 draftSessionId 时，
     * 写入页面状态与 URL，保证查询失效重取复用同一会话（不换主键、不清空用户勾选）。
     */
    function syncSessionId(nextDraftSessionId: string) {
        setSession((prev) => {
            if (!prev || prev.draftSessionId === nextDraftSessionId) return prev
            return { ...prev, draftSessionId: nextDraftSessionId }
        })
        patchUrl({ draftSessionId: nextDraftSessionId }, { replace: true })
    }

    const openPreview = React.useCallback(
        (payableAccountId: string) => {
            patchUrl(
                {
                    detailId: payableAccountId,
                    previewKind: "payable",
                },
                { replace: true },
            )
        },
        [patchUrl],
    )

    /**
     * 打开已登记供应商付款事实详情，并保持付款工作视图。
     *
     * @param paymentId 付款主键。
     */
    const openPaymentPreview = React.useCallback(
        (paymentId: string) => {
            patchUrl(
                {
                    detailId: paymentId,
                    previewKind: "payment",
                    view: "payment",
                },
                { replace: true },
            )
        },
        [patchUrl],
    )

    /**
     * 打开供应商退款详情，嵌入通用审批区。
     *
     * @param refundId 退款主键。
     */
    const openRefundPreview = React.useCallback(
        (refundId: string) => {
            patchUrl(
                {
                    detailId: refundId,
                    previewKind: "refund",
                },
                { replace: true },
            )
        },
        [patchUrl],
    )

    /**
     * 打开付款冲正详情，嵌入通用审批区。
     *
     * @param reversalId 冲正主键。
     */
    const openReversalPreview = React.useCallback(
        (reversalId: string) => {
            patchUrl(
                {
                    detailId: reversalId,
                    previewKind: "reversal",
                },
                { replace: true },
            )
        },
        [patchUrl],
    )

    const closePreview = React.useCallback(() => {
        patchUrl({ detailId: null, previewKind: null }, { replace: true })
    }, [patchUrl])

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
        sourceType: validSourceType,
        status: validStatus,
        due: validDue,
        paymentGate: validPaymentGate,
        purchaseOrderId,
        fromWorkspace,
        returnTo,
        trackFilter,
        searchInput,
        setSearchInput,
        searchInputRef,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters,
        appliedChips,
        applyFilters,
        resetMoreFilters,
        removeFilter,
        supplierDraft,
        setSupplierDraft,
        sourceTypeDraft,
        setSourceTypeDraft,
        statusDraft,
        setStatusDraft,
        dueDraft,
        setDueDraft,
        paymentGateDraft,
        setPaymentGateDraft,
        trackDraft,
        setTrackDraft,
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
        syncSessionId,
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
