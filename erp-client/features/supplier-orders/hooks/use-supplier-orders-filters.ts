"use client"

import * as React from "react"

import { useSupplierSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useSupplierOrdersSearchDraft } from "@/features/supplier-orders/hooks/use-supplier-orders-search-draft"
import { useSupplierOrdersUrlState } from "@/features/supplier-orders/hooks/use-supplier-orders-url-state"
import type { SupplierOrdersUrlPatch } from "@/features/supplier-orders/lib/url-state"
import {
    CANCEL_STATUS_LABEL,
    FULFILLMENT_STATUS_LABEL,
    REFUND_STATUS_LABEL,
    type CancelStatus,
    type RefundStatus,
    type SupplierFulfillmentStatus,
} from "@/features/supplier-orders/types"

/** 可被单独移除的已生效条件；paidRange 把支付时间上下界作为一个条件移除。 */
export type SupplierOrdersFilterKey =
    | "q"
    | "supplierId"
    | "fulfillmentStatuses"
    | "cancelStatuses"
    | "refundStatuses"
    | "aftersalePending"
    | "paidRange"

export type SupplierOrdersAppliedChip = Readonly<{
    key: SupplierOrdersFilterKey
    label: string
}>

/** 支付时间上下界校验；ISO 日期字符串可直接按字典序比较。 */
function paidDateRangeError(from: string, to: string): string | null {
    if (from && to && from > to) {
        return "支付开始日期不能晚于结束日期"
    }
    return null
}

/**
 * 供应商订单列表筛选状态（docs/ui-filter-design.md §5）：
 * - Applied：URL 是唯一事实源；query key / 计数 / 摘要 / 空态只读它；
 * - Draft：本地受控 state，变化不触发请求；
 * - UI：面板展开与校验提示。
 *
 * 快捷筛选（MetricStrip）直接写 URL，草稿由回填 effect 同步。
 */
export function useSupplierOrdersFilters(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const { url, returnTo, updateUrl } = useSupplierOrdersUrlState()

    const hasActiveFilters = Boolean(
        url.q ||
        url.supplierId ||
        url.fulfillmentStatuses?.length ||
        url.cancelStatuses?.length ||
        url.refundStatuses?.length ||
        url.aftersalePending ||
        url.paidFrom ||
        url.paidTo,
    )
    /** 「已启用」与初始展开只认结构化条件，不含 q 与快捷筛选。 */
    const hasStructuredFilters = Boolean(
        url.supplierId ||
        url.fulfillmentStatuses?.length ||
        url.cancelStatuses?.length ||
        url.refundStatuses?.length ||
        url.paidFrom ||
        url.paidTo,
    )

    const { searchDraft, setSearchDraft } = useSupplierOrdersSearchDraft({
        q: url.q,
        searchInputRef,
    })
    const [supplierIdDraft, setSupplierIdDraft] = React.useState<string | null>(
        url.supplierId ?? null,
    )
    const [fulfillmentStatusesDraft, setFulfillmentStatusesDraft] =
        React.useState<SupplierFulfillmentStatus[]>(
            url.fulfillmentStatuses ?? [],
        )
    const [cancelStatusesDraft, setCancelStatusesDraft] = React.useState<
        CancelStatus[]
    >(url.cancelStatuses ?? [])
    const [refundStatusesDraft, setRefundStatusesDraft] = React.useState<
        RefundStatus[]
    >(url.refundStatuses ?? [])
    const [paidFromDraft, setPaidFromDraft] = React.useState(url.paidFrom ?? "")
    const [paidToDraft, setPaidToDraft] = React.useState(url.paidTo ?? "")

    const [panelOpen, setPanelOpen] = React.useState(hasStructuredFilters)
    const [filterError, setFilterError] = React.useState<string | null>(null)

    // 供应商名称解析（chip 文案用业务名称，不展示内部 ID）；无 supplierId 时不请求。
    const supplierSelectorQuery = useSupplierSelectorQuery(
        { query: "", purpose: "filter" },
        url.supplierId,
    )
    const selectedSupplierName =
        supplierSelectorQuery.selected.data?.supplierName

    /** 收起态 Enter / 搜索框尾部提交 / 面板「应用全部筛选」共用同一提交。 */
    const applyFilters = React.useCallback(() => {
        const from = paidFromDraft.trim()
        const to = paidToDraft.trim()
        const nextError = paidDateRangeError(from, to)
        setFilterError(nextError)
        if (nextError) return
        updateUrl({
            q: searchDraft.trim() || undefined,
            supplierId: supplierIdDraft ?? undefined,
            fulfillmentStatuses:
                fulfillmentStatusesDraft.length > 0
                    ? fulfillmentStatusesDraft
                    : undefined,
            cancelStatuses:
                cancelStatusesDraft.length > 0
                    ? cancelStatusesDraft
                    : undefined,
            refundStatuses:
                refundStatusesDraft.length > 0
                    ? refundStatusesDraft
                    : undefined,
            paidFrom: from || undefined,
            paidTo: to || undefined,
            page: 1,
        })
        setPanelOpen(false)
    }, [
        cancelStatusesDraft,
        fulfillmentStatusesDraft,
        paidFromDraft,
        paidToDraft,
        refundStatusesDraft,
        searchDraft,
        supplierIdDraft,
        updateUrl,
    ])

    /** 移除单个已生效条件；支付时间上下界作为一个条件一起移除。 */
    const removeFilter = React.useCallback(
        (key: SupplierOrdersFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "supplierId") setSupplierIdDraft(null)
            if (key === "fulfillmentStatuses") setFulfillmentStatusesDraft([])
            if (key === "cancelStatuses") setCancelStatusesDraft([])
            if (key === "refundStatuses") setRefundStatusesDraft([])
            if (key === "paidRange") {
                setPaidFromDraft("")
                setPaidToDraft("")
                setFilterError(null)
            }
            const patch: SupplierOrdersUrlPatch = { page: 1 }
            if (key === "q") patch.q = undefined
            else if (key === "supplierId") patch.supplierId = undefined
            else if (key === "fulfillmentStatuses")
                patch.fulfillmentStatuses = undefined
            else if (key === "cancelStatuses") patch.cancelStatuses = undefined
            else if (key === "refundStatuses") patch.refundStatuses = undefined
            else if (key === "aftersalePending")
                patch.aftersalePending = undefined
            else if (key === "paidRange") {
                patch.paidFrom = undefined
                patch.paidTo = undefined
            }
            updateUrl(patch)
        },
        [setSearchDraft, updateUrl],
    )

    /** 仅清除结构化条件；保留关键词与快捷筛选，面板保持展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setSupplierIdDraft(null)
        setFulfillmentStatusesDraft([])
        setCancelStatusesDraft([])
        setRefundStatusesDraft([])
        setPaidFromDraft("")
        setPaidToDraft("")
        setFilterError(null)
        updateUrl({
            supplierId: undefined,
            fulfillmentStatuses: undefined,
            cancelStatuses: undefined,
            refundStatuses: undefined,
            paidFrom: undefined,
            paidTo: undefined,
            page: 1,
        })
    }, [updateUrl])

    /** 清空全部：同时重置草稿、错误、面板与 URL 筛选参数；保留视图/排序/分页大小/导航上下文。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setSupplierIdDraft(null)
        setFulfillmentStatusesDraft([])
        setCancelStatusesDraft([])
        setRefundStatusesDraft([])
        setPaidFromDraft("")
        setPaidToDraft("")
        setFilterError(null)
        setPanelOpen(false)
        updateUrl({
            q: undefined,
            supplierId: undefined,
            fulfillmentStatuses: undefined,
            cancelStatuses: undefined,
            refundStatuses: undefined,
            aftersalePending: undefined,
            paidFrom: undefined,
            paidTo: undefined,
            page: 1,
        })
    }, [setSearchDraft, updateUrl])

    /** 已生效条件全部显性化为可移除 chip，无隐形查询参数。 */
    const appliedChips = React.useMemo<
        readonly SupplierOrdersAppliedChip[]
    >(() => {
        const chips: SupplierOrdersAppliedChip[] = []
        if (url.q?.trim()) {
            chips.push({ key: "q", label: `搜索：${url.q.trim()}` })
        }
        if (url.supplierId) {
            chips.push({
                key: "supplierId",
                label: `供应商：${selectedSupplierName ?? url.supplierId}`,
            })
        }
        if (url.fulfillmentStatuses?.length) {
            chips.push({
                key: "fulfillmentStatuses",
                label: `履约状态：${url.fulfillmentStatuses
                    .map((s) => FULFILLMENT_STATUS_LABEL[s])
                    .join("、")}`,
            })
        }
        if (url.cancelStatuses?.length) {
            chips.push({
                key: "cancelStatuses",
                label: `取消状态：${url.cancelStatuses
                    .map((s) => CANCEL_STATUS_LABEL[s])
                    .join("、")}`,
            })
        }
        if (url.refundStatuses?.length) {
            chips.push({
                key: "refundStatuses",
                label: `退款状态：${url.refundStatuses
                    .map((s) => REFUND_STATUS_LABEL[s])
                    .join("、")}`,
            })
        }
        if (url.aftersalePending) {
            chips.push({ key: "aftersalePending", label: "售后待处理" })
        }
        if (url.paidFrom || url.paidTo) {
            chips.push({
                key: "paidRange",
                label: `支付时间：${url.paidFrom ?? "不限"} 至 ${url.paidTo ?? "不限"}`,
            })
        }
        return chips
    }, [
        selectedSupplierName,
        url.aftersalePending,
        url.cancelStatuses,
        url.fulfillmentStatuses,
        url.paidFrom,
        url.paidTo,
        url.q,
        url.refundStatuses,
        url.supplierId,
    ])

    // URL 回填：外部变化（快捷筛选 / 前进后退 / 刷新）同步结构化草稿；
    // 面板展开态不受回填影响（提交成功后不会因回填再次强制展开）。
    React.useEffect(() => {
        setSupplierIdDraft(url.supplierId ?? null)
        setFulfillmentStatusesDraft(url.fulfillmentStatuses ?? [])
        setCancelStatusesDraft(url.cancelStatuses ?? [])
        setRefundStatusesDraft(url.refundStatuses ?? [])
        setPaidFromDraft(url.paidFrom ?? "")
        setPaidToDraft(url.paidTo ?? "")
        setFilterError(null)
    }, [
        url.cancelStatuses,
        url.fulfillmentStatuses,
        url.paidFrom,
        url.paidTo,
        url.refundStatuses,
        url.supplierId,
    ])

    return {
        url,
        returnTo,
        updateUrl,
        hasActiveFilters,
        hasStructuredFilters,
        searchDraft,
        setSearchDraft,
        supplierIdDraft,
        setSupplierIdDraft,
        fulfillmentStatusesDraft,
        setFulfillmentStatusesDraft,
        cancelStatusesDraft,
        setCancelStatusesDraft,
        refundStatusesDraft,
        setRefundStatusesDraft,
        paidFromDraft,
        setPaidFromDraft,
        paidToDraft,
        setPaidToDraft,
        panelOpen,
        setPanelOpen,
        filterError,
        setFilterError,
        appliedChips,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
    }
}
