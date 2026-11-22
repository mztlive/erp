"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import { useSupplierAccountsQuery } from "@/features/supplier-payables/hooks/queries"
import {
    businessLabelOrPlaceholder,
    MISSING_SUPPLIER_NAME,
} from "@/features/supplier-payables/lib/display-labels"
import { missingSourceDocumentNo } from "@/features/supplier-payables/lib/related-documents"
import {
    parseView,
    patchForViewChange,
} from "@/features/supplier-payables/lib/url-state"
import {
    DUE_LABEL,
    PAYABLE_STATUS_LABEL,
    PAYMENT_GATE_LABEL,
    SOURCE_TYPE_LABEL,
    TRACK_LABEL,
    type AllocationTrack,
    type SupplierAccountsQuery,
    type SupplierAccountsView,
} from "@/features/supplier-payables/types"
import { compareDecimal } from "@/lib/fixed-decimal"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import type {
    SupplierAppliedChip,
    SupplierFilterKey,
} from "../components/supplier-accounts-toolbar"

export type SupplierAccountsPatchUrl = (
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean; scroll?: boolean },
) => void

/** W12 列表筛选、分页、排序和 URL 回填。 */
export function useSupplierAccountsFilters() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const view = parseView(searchParams.get("view"))
    const qParam = searchParams.get("q") ?? ""
    const supplierId = searchParams.get("supplierId") ?? undefined
    const sourceTypeParam = searchParams.get("sourceType")
    const statusParam = searchParams.get("status")
    const dueParam = searchParams.get("due")
    const paymentGateParam = searchParams.get("paymentGate")
    const purchaseOrderId = searchParams.get("purchaseOrderId") ?? undefined
    const trackParam = searchParams.get("track") ?? "all"

    const sourceType =
        sourceTypeParam === "PURCHASE_ORDER" ||
        sourceTypeParam === "SUPPLIER_SETTLEMENT"
            ? sourceTypeParam
            : undefined
    const status =
        statusParam === "OPEN" ||
        statusParam === "PARTIAL" ||
        statusParam === "SETTLED"
            ? statusParam
            : undefined
    const due =
        dueParam === "not_due" ||
        dueParam === "due_today" ||
        dueParam === "overdue"
            ? dueParam
            : undefined
    const paymentGate =
        paymentGateParam === "satisfied" || paymentGateParam === "unsatisfied"
            ? paymentGateParam
            : undefined
    const track: AllocationTrack | undefined =
        trackParam === "payment" || trackParam === "purchase_invoice"
            ? trackParam
            : undefined

    const trackFilter: AllocationTrack | "all" = track ?? "all"

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [panelOpen, setPanelOpen] = React.useState(false)
    const [supplierDraft, setSupplierDraft] = React.useState<string | null>(
        supplierId ?? null,
    )
    const [sourceTypeDraft, setSourceTypeDraft] = React.useState<
        "PURCHASE_ORDER" | "SUPPLIER_SETTLEMENT" | "all"
    >(sourceType ?? "all")
    const [statusDraft, setStatusDraft] = React.useState<
        "OPEN" | "PARTIAL" | "SETTLED" | "all"
    >(status ?? "all")
    const [dueDraft, setDueDraft] = React.useState<
        "not_due" | "due_today" | "overdue" | "all"
    >(due ?? "all")
    const [paymentGateDraft, setPaymentGateDraft] = React.useState<
        "satisfied" | "unsatisfied" | "all"
    >(paymentGate ?? "all")
    const [trackDraft, setTrackDraft] = React.useState<AllocationTrack | "all">(
        track ?? "all",
    )
    const [sorting, setSorting] = React.useState<SortingState>([])

    const pageParam = searchParams.get("page")
    const pageIndex = React.useMemo(() => {
        const page = pageParam ? Number.parseInt(pageParam, 10) : 1
        return Number.isFinite(page) && page >= 1 ? page - 1 : 0
    }, [pageParam])
    const [pageSize, setPageSize] = React.useState(20)
    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex, pageSize }),
        [pageIndex, pageSize],
    )

    const query = React.useMemo<SupplierAccountsQuery>(
        () => ({
            view,
            q: qParam.trim() || undefined,
            supplierId,
            sourceType,
            status,
            due,
            paymentGate,
            purchaseOrderId,
        }),
        [
            due,
            paymentGate,
            purchaseOrderId,
            qParam,
            sourceType,
            status,
            supplierId,
            view,
        ],
    )
    const listQuery = useSupplierAccountsQuery(query)
    const data = listQuery.data
    const hasDueDates =
        data?.payables.some(
            (payable) => payable.dueDate && payable.dueState !== "unknown",
        ) ?? false
    const dueFilterUnavailable = Boolean(
        view === "payable" && due && data?.payables.length && !hasDueDates,
    )

    const sortedPayables = React.useMemo(() => {
        const list = (data?.payables ?? []).filter((payable) => {
            if (due && hasDueDates && payable.dueState !== due) return false
            if (!paymentGate) return true
            const summary = payable.paymentGateSummary?.state
            if (summary) {
                return (
                    summary ===
                    (paymentGate === "satisfied" ? "SATISFIED" : "BLOCKED")
                )
            }
            const guidance = payable.paymentGuidance
            if (!guidance?.prepayGate || guidance.prepaymentGap == null)
                return false
            const satisfied =
                compareDecimal(guidance.prepaymentGap, "0", 2) === 0
            return satisfied === (paymentGate === "satisfied")
        })
        if (sorting.length === 0) return list
        return list.sort((left, right) => {
            for (const { id, desc } of sorting) {
                const direction = desc ? -1 : 1
                if (id === "supplier") {
                    const order = left.supplierName.localeCompare(
                        right.supplierName,
                        "zh-CN",
                    )
                    if (order) return order * direction
                } else if (
                    id === "openTotal" ||
                    id === "grossTotal" ||
                    id === "settledTotal" ||
                    id === "invoicedTotal"
                ) {
                    const a = left[id]
                    const b = right[id]
                    if (a == null && b != null) return 1
                    if (a != null && b == null) return -1
                    if (a != null && b != null) {
                        const order = compareDecimal(a, b, 2)
                        if (order) return order * direction
                    }
                }
            }
            return 0
        })
    }, [data?.payables, due, hasDueDates, paymentGate, sorting])

    const patchUrl = React.useCallback<SupplierAccountsPatchUrl>(
        (patch, options) => {
            patchSearchParams(
                { router, pathname, searchParams, view },
                patch,
                options,
            )
        },
        [pathname, router, searchParams, view],
    )
    const hasActiveFilters = Boolean(
        qParam.trim() ||
        supplierId ||
        sourceType ||
        status ||
        due ||
        paymentGate ||
        track ||
        purchaseOrderId,
    )
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
        dueDraft,
        patchUrl,
        paymentGateDraft,
        searchInput,
        sourceTypeDraft,
        statusDraft,
        supplierDraft,
        trackDraft,
    ])
    /** 只清当前视图的低频草稿；保留搜索、供应商与常驻筛选。 */
    const resetMoreFilters = React.useCallback(() => {
        if (view === "payable") {
            setSourceTypeDraft("all")
            setDueDraft("all")
            setPaymentGateDraft("all")
        }
    }, [view])

    /** 关闭托盘只恢复低频草稿，保留搜索、供应商与常驻筛选。 */
    const cancelMoreFilters = React.useCallback(() => {
        if (view === "payable") {
            setSourceTypeDraft(sourceType ?? "all")
            setDueDraft(due ?? "all")
            setPaymentGateDraft(paymentGate ?? "all")
        }
        setPanelOpen(false)
    }, [due, paymentGate, sourceType, view])

    const switchView = React.useCallback(
        (nextView: SupplierAccountsView) => {
            if (nextView !== view) cancelMoreFilters()
            patchUrl(patchForViewChange(nextView), {
                replace: true,
                scroll: false,
            })
        },
        [cancelMoreFilters, patchUrl, view],
    )

    const hasPendingChanges =
        searchInput.trim() !== qParam.trim() ||
        supplierDraft !== (supplierId ?? null) ||
        sourceTypeDraft !== (sourceType ?? "all") ||
        statusDraft !== (status ?? "all") ||
        dueDraft !== (due ?? "all") ||
        paymentGateDraft !== (paymentGate ?? "all") ||
        trackDraft !== (track ?? "all")
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
        [patchUrl],
    )
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
    }, [patchUrl])

    const appliedChips = React.useMemo<readonly SupplierAppliedChip[]>(() => {
        const chips: SupplierAppliedChip[] = []
        const queryText = qParam.trim()
        if (queryText) chips.push({ key: "q", label: `搜索：${queryText}` })
        if (supplierId) {
            const supplierName = data?.suppliers.find(
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
        if (sourceType) {
            chips.push({
                key: "sourceType",
                label: `来源类型：${SOURCE_TYPE_LABEL[sourceType]}`,
            })
        }
        if (status) {
            chips.push({
                key: "status",
                label: `状态：${PAYABLE_STATUS_LABEL[status]}`,
            })
        }
        if (due) chips.push({ key: "due", label: `到期：${DUE_LABEL[due]}` })
        if (paymentGate) {
            chips.push({
                key: "paymentGate",
                label: `先款条件：${PAYMENT_GATE_LABEL[paymentGate]}`,
            })
        }
        if (track) {
            chips.push({
                key: "track",
                label: `单据类型：${TRACK_LABEL[track]}`,
            })
        }
        if (purchaseOrderId) {
            const purchaseOrderNo = data?.payables.find(
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
        due,
        paymentGate,
        purchaseOrderId,
        qParam,
        sourceType,
        status,
        supplierId,
        track,
    ])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            if (next.pageSize !== pageSize) {
                setPageSize(next.pageSize)
                if (pageIndex !== 0)
                    patchUrl({ page: null }, { replace: true, scroll: false })
                return
            }
            patchUrl(
                {
                    page:
                        next.pageIndex === 0
                            ? null
                            : String(next.pageIndex + 1),
                },
                { replace: true, scroll: false },
            )
        },
        [pageIndex, pageSize, patchUrl],
    )

    React.useEffect(() => {
        setSearchInput(qParam.trim())
        setSupplierDraft(supplierId ?? null)
        setSourceTypeDraft(sourceType ?? "all")
        setStatusDraft(status ?? "all")
        setDueDraft(due ?? "all")
        setPaymentGateDraft(paymentGate ?? "all")
        setTrackDraft(track ?? "all")
    }, [due, paymentGate, qParam, sourceType, status, supplierId, track])

    useSearchHotkey(searchInputRef)

    return {
        view,
        supplierId,
        sourceType,
        status,
        due,
        paymentGate,
        purchaseOrderId,
        trackFilter,
        hasDueDates,
        dueFilterUnavailable,
        searchInput,
        setSearchInput,
        searchInputRef,
        panelOpen: view === "payable" && panelOpen,
        setPanelOpen,
        appliedChips,
        applyFilters,
        resetMoreFilters,
        cancelMoreFilters,
        hasPendingChanges,
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
        hasActiveFilters,
        clearFilters,
        patchUrl,
        switchView,
        listQuery,
        data,
        sortedPayables,
    }
}
