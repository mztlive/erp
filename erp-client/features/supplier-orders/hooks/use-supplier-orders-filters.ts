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
    | "ownerUserIds"
    | "handlerUserIds"
    | "orgUnitIds"

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

function sameCodeList(
    left: readonly string[],
    right: readonly string[] | undefined,
): boolean {
    const other = right ?? []
    return (
        left.length === other.length &&
        left.every((value, index) => value === other[index])
    )
}

/**
 * 供应商订单列表筛选状态（docs/ui-filter-design.md §5）：
 * - Applied：URL 是唯一事实源；query key / 计数 / 摘要 / 空态只读它；
 * - Draft：本地受控 state，变化不触发请求；
 * - UI：面板展开与校验提示。
 *
 * 视图切换与前进后退后，草稿由回填 effect 同步。
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
        url.paidTo ||
        url.ownerUserIds ||
        url.handlerUserIds ||
        url.orgUnitIds,
    )
    /** 结构化条件：不含 q。 */
    const hasStructuredFilters = Boolean(
        url.supplierId ||
        url.fulfillmentStatuses?.length ||
        url.cancelStatuses?.length ||
        url.refundStatuses?.length ||
        url.aftersalePending ||
        url.paidFrom ||
        url.paidTo ||
        url.ownerUserIds ||
        url.handlerUserIds ||
        url.orgUnitIds,
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
    const [aftersalePendingDraft, setAftersalePendingDraft] = React.useState(
        Boolean(url.aftersalePending),
    )
    const [paidFromDraft, setPaidFromDraft] = React.useState(url.paidFrom ?? "")
    const [paidToDraft, setPaidToDraft] = React.useState(url.paidTo ?? "")
    const [ownerUserIdsDraft, setOwnerUserIdsDraft] = React.useState(
        url.ownerUserIds ?? "",
    )
    const [handlerUserIdsDraft, setHandlerUserIdsDraft] = React.useState(
        url.handlerUserIds ?? "",
    )
    const [orgUnitIdsDraft, setOrgUnitIdsDraft] = React.useState(
        url.orgUnitIds ?? "",
    )
    const [includeDescendantsDraft, setIncludeDescendantsDraft] =
        React.useState(Boolean(url.includeDescendants))

    /** 深链条件显示为标签，不自动打开浮层。 */
    const [panelOpen, setPanelOpen] = React.useState(false)
    const [filterError, setFilterError] = React.useState<string | null>(null)

    // 供应商名称解析（chip 文案用业务名称，不展示内部 ID）；无 supplierId 时不请求。
    const supplierSelectorQuery = useSupplierSelectorQuery(
        { query: "", purpose: "filter" },
        url.supplierId,
    )
    const selectedSupplierName =
        supplierSelectorQuery.selected.data?.supplierName

    /** 收起态 Enter / 搜索框尾部提交 / 主行「查询」共用同一提交。 */
    const applyFilters = React.useCallback(() => {
        const from = paidFromDraft.trim()
        const to = paidToDraft.trim()
        const nextError = paidDateRangeError(from, to)
        setFilterError(nextError)
        if (nextError) {
            setPanelOpen(true)
            return
        }
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
            aftersalePending: aftersalePendingDraft || undefined,
            paidFrom: from || undefined,
            paidTo: to || undefined,
            ownerUserIds: ownerUserIdsDraft.trim() || undefined,
            handlerUserIds: handlerUserIdsDraft.trim() || undefined,
            orgUnitIds: orgUnitIdsDraft.trim() || undefined,
            includeDescendants:
                orgUnitIdsDraft.trim() && includeDescendantsDraft
                    ? true
                    : undefined,
            page: 1,
        })
        setPanelOpen(false)
    }, [
        aftersalePendingDraft,
        cancelStatusesDraft,
        fulfillmentStatusesDraft,
        handlerUserIdsDraft,
        includeDescendantsDraft,
        orgUnitIdsDraft,
        ownerUserIdsDraft,
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
            if (key === "aftersalePending") setAftersalePendingDraft(false)
            if (key === "ownerUserIds") setOwnerUserIdsDraft("")
            if (key === "handlerUserIds") setHandlerUserIdsDraft("")
            if (key === "orgUnitIds") {
                setOrgUnitIdsDraft("")
                setIncludeDescendantsDraft(false)
            }
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
            else if (key === "ownerUserIds") patch.ownerUserIds = undefined
            else if (key === "handlerUserIds") patch.handlerUserIds = undefined
            else if (key === "orgUnitIds") {
                patch.orgUnitIds = undefined
                patch.includeDescendants = undefined
            } else if (key === "paidRange") {
                patch.paidFrom = undefined
                patch.paidTo = undefined
            }
            updateUrl(patch)
        },
        [setSearchDraft, updateUrl],
    )

    /** 取消、关闭、Esc 和外点只恢复低频草稿，保留搜索、供应商和履约状态。 */
    const cancelMoreFilters = React.useCallback(() => {
        setCancelStatusesDraft(url.cancelStatuses ?? [])
        setRefundStatusesDraft(url.refundStatuses ?? [])
        setAftersalePendingDraft(Boolean(url.aftersalePending))
        setPaidFromDraft(url.paidFrom ?? "")
        setPaidToDraft(url.paidTo ?? "")
        setOwnerUserIdsDraft(url.ownerUserIds ?? "")
        setHandlerUserIdsDraft(url.handlerUserIds ?? "")
        setOrgUnitIdsDraft(url.orgUnitIds ?? "")
        setIncludeDescendantsDraft(Boolean(url.includeDescendants))
        setFilterError(null)
        setPanelOpen(false)
    }, [
        url.aftersalePending,
        url.cancelStatuses,
        url.handlerUserIds,
        url.includeDescendants,
        url.orgUnitIds,
        url.ownerUserIds,
        url.paidFrom,
        url.paidTo,
        url.refundStatuses,
    ])

    /** 只清低频草稿；保留关键词、供应商、履约状态与当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setCancelStatusesDraft([])
        setRefundStatusesDraft([])
        setAftersalePendingDraft(false)
        setPaidFromDraft("")
        setPaidToDraft("")
        setOwnerUserIdsDraft("")
        setHandlerUserIdsDraft("")
        setOrgUnitIdsDraft("")
        setIncludeDescendantsDraft(false)
        setFilterError(null)
    }, [])

    const hasPendingChanges =
        searchDraft.trim() !== (url.q ?? "").trim() ||
        supplierIdDraft !== (url.supplierId ?? null) ||
        !sameCodeList(fulfillmentStatusesDraft, url.fulfillmentStatuses) ||
        !sameCodeList(cancelStatusesDraft, url.cancelStatuses) ||
        !sameCodeList(refundStatusesDraft, url.refundStatuses) ||
        aftersalePendingDraft !== Boolean(url.aftersalePending) ||
        paidFromDraft.trim() !== (url.paidFrom ?? "") ||
        paidToDraft.trim() !== (url.paidTo ?? "") ||
        ownerUserIdsDraft.trim() !== (url.ownerUserIds ?? "") ||
        handlerUserIdsDraft.trim() !== (url.handlerUserIds ?? "") ||
        orgUnitIdsDraft.trim() !== (url.orgUnitIds ?? "") ||
        includeDescendantsDraft !== Boolean(url.includeDescendants)

    /** 清除全部：同时重置草稿、错误、面板与 URL 筛选参数；保留视图/排序/分页大小/导航上下文。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setSupplierIdDraft(null)
        setFulfillmentStatusesDraft([])
        setCancelStatusesDraft([])
        setRefundStatusesDraft([])
        setAftersalePendingDraft(false)
        setPaidFromDraft("")
        setPaidToDraft("")
        setOwnerUserIdsDraft("")
        setHandlerUserIdsDraft("")
        setOrgUnitIdsDraft("")
        setIncludeDescendantsDraft(false)
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
            ownerUserIds: undefined,
            handlerUserIds: undefined,
            orgUnitIds: undefined,
            includeDescendants: undefined,
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
        if (url.ownerUserIds) {
            chips.push({ key: "ownerUserIds", label: "跟进人已筛选" })
        }
        if (url.handlerUserIds) {
            chips.push({ key: "handlerUserIds", label: "异常处理人已筛选" })
        }
        if (url.orgUnitIds) {
            chips.push({
                key: "orgUnitIds",
                label: url.includeDescendants
                    ? `业务组织（含下级）：${url.orgUnitIds}`
                    : `业务组织：${url.orgUnitIds}`,
            })
        }
        return chips
    }, [
        selectedSupplierName,
        url.aftersalePending,
        url.cancelStatuses,
        url.fulfillmentStatuses,
        url.handlerUserIds,
        url.includeDescendants,
        url.orgUnitIds,
        url.ownerUserIds,
        url.paidFrom,
        url.paidTo,
        url.q,
        url.refundStatuses,
        url.supplierId,
    ])

    // URL 回填：外部变化（前进后退 / 刷新）同步结构化草稿；
    // 面板展开态不受回填影响（提交成功后不会因回填再次强制展开）。
    React.useEffect(() => {
        setSupplierIdDraft(url.supplierId ?? null)
        setFulfillmentStatusesDraft(url.fulfillmentStatuses ?? [])
        setCancelStatusesDraft(url.cancelStatuses ?? [])
        setRefundStatusesDraft(url.refundStatuses ?? [])
        setAftersalePendingDraft(Boolean(url.aftersalePending))
        setPaidFromDraft(url.paidFrom ?? "")
        setPaidToDraft(url.paidTo ?? "")
        setOwnerUserIdsDraft(url.ownerUserIds ?? "")
        setHandlerUserIdsDraft(url.handlerUserIds ?? "")
        setOrgUnitIdsDraft(url.orgUnitIds ?? "")
        setIncludeDescendantsDraft(Boolean(url.includeDescendants))
        setFilterError(null)
    }, [
        url.aftersalePending,
        url.cancelStatuses,
        url.fulfillmentStatuses,
        url.handlerUserIds,
        url.includeDescendants,
        url.orgUnitIds,
        url.ownerUserIds,
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
        hasPendingChanges,
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
        aftersalePendingDraft,
        setAftersalePendingDraft,
        paidFromDraft,
        setPaidFromDraft,
        paidToDraft,
        setPaidToDraft,
        ownerUserIdsDraft,
        setOwnerUserIdsDraft,
        handlerUserIdsDraft,
        setHandlerUserIdsDraft,
        orgUnitIdsDraft,
        setOrgUnitIdsDraft,
        includeDescendantsDraft,
        setIncludeDescendantsDraft,
        panelOpen,
        setPanelOpen,
        filterError,
        setFilterError,
        appliedChips,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearAllFilters,
    }
}
