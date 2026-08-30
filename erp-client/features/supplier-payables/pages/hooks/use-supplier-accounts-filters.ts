"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
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
    type PayableRow,
    type SupplierAccountsQuery,
    type SupplierAccountsView,
} from "@/features/supplier-payables/types"
import { compareDecimal } from "@/lib/fixed-decimal"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
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
    const track =
        trackParam === "payment" || trackParam === "purchase_invoice"
            ? trackParam
            : undefined

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const hasStructuredFilters = Boolean(
        supplierId || sourceType || status || due || paymentGate || track,
    )
    const [panelOpen, setPanelOpen] = React.useState(hasStructuredFilters)
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
    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex, pageSize: 20 }),
        [pageIndex],
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

    const sortedPayables = React.useMemo(() => {
        const list = data?.payables ? [...data.payables] : []
        if (sorting.length === 0) return list
        const { id, desc } = sorting[0]!
        const direction = desc ? -1 : 1
        if (id === "amounts" || id === "tracks") {
            return list.sort(
                (left, right) =>
                    compareDecimal(
                        id === "amounts" ? left.openTotal : left.settledTotal,
                        id === "amounts" ? right.openTotal : right.settledTotal,
                        2,
                    ) * direction,
            )
        }
        const key = (payable: PayableRow): string => {
            if (id === "due") return payable.dueDate
            if (id === "supplier") return payable.supplierName
            return payable.payableAccountId
        }
        return list.sort(
            (left, right) =>
                key(left).localeCompare(key(right), "zh-CN") * direction,
        )
    }, [data?.payables, sorting])

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
    const switchView = React.useCallback(
        (nextView: SupplierAccountsView) => {
            patchUrl(patchForViewChange(nextView), { replace: true })
        },
        [patchUrl],
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
    }, [patchUrl])
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
                label: `轨道：${TRACK_LABEL[track]}`,
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
            patchUrl(
                {
                    page:
                        next.pageIndex === 0
                            ? null
                            : String(next.pageIndex + 1),
                },
                { replace: true },
            )
        },
        [patchUrl],
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
            if (
                target?.tagName === "INPUT" ||
                target?.tagName === "TEXTAREA" ||
                target?.tagName === "SELECT" ||
                target?.isContentEditable ||
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

    return {
        view,
        supplierId,
        sourceType,
        status,
        due,
        paymentGate,
        purchaseOrderId,
        trackFilter: track ?? "all",
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
        hasActiveFilters,
        clearFilters,
        patchUrl,
        switchView,
        listQuery,
        data,
        sortedPayables,
    }
}
