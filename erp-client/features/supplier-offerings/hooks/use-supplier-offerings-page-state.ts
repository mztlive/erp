"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import {
    buildSupplierOfferingsSearchParams,
    parseSupplierOfferingsSearchParams,
    type SupplierOfferingsUrlState,
} from "@/features/supplier-offerings/lib/url-state"
import {
    AVAILABILITY_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
    type AvailabilityStatus,
    type OfferingSourceType,
} from "@/features/supplier-offerings/types"

export type OfferingSourceFilter = OfferingSourceType | "all"
export type AvailabilityStatusFilter = AvailabilityStatus | "all"

/** 可被单独移除的已生效筛选条件。 */
export type SupplierOfferingFilterKey =
    | "q"
    | "skuId"
    | "skuNo"
    | "productNo"
    | "supplierId"
    | "sourceType"
    | "availabilityStatus"
    | "ownerUserIds"
    | "procurementOwnerUserIds"
    | "orgUnitIds"

export type SupplierOfferingAppliedChip = Readonly<{
    key: SupplierOfferingFilterKey
    label: string
}>

function selectedCount(value: string) {
    return value.split(",").filter((part) => part.trim()).length
}

/**
 * 把 Tab 以外的已生效条件派生为可单独移除的 chip（docs/ui-filter-design.md §3.6）。
 * 公司 SKU 显示业务编号、供应商显示业务名称，不展示内部 ID（§4.5）；
 * 列表暂无数据时回退为「已选择」。
 */
export function buildSupplierOfferingAppliedChips(
    urlState: SupplierOfferingsUrlState,
    labels: Readonly<{
        skuNoLabel?: string | null
        supplierNameLabel?: string | null
    }>,
): readonly SupplierOfferingAppliedChip[] {
    const chips: SupplierOfferingAppliedChip[] = []
    if (urlState.q) {
        chips.push({ key: "q", label: `搜索：${urlState.q}` })
    }
    if (urlState.skuId) {
        chips.push({
            key: "skuId",
            label: `公司 SKU：${labels.skuNoLabel ?? "已选择"}`,
        })
    }
    if (urlState.skuNo) {
        chips.push({ key: "skuNo", label: `SKU 编号：${urlState.skuNo}` })
    }
    if (urlState.productNo) {
        chips.push({
            key: "productNo",
            label: `SPU 编号：${urlState.productNo}`,
        })
    }
    if (urlState.supplierId) {
        chips.push({
            key: "supplierId",
            label: `供应商：${labels.supplierNameLabel ?? "已选择"}`,
        })
    }
    if (urlState.sourceType) {
        chips.push({
            key: "sourceType",
            label: `登记来源：${SOURCE_TYPE_LABELS[urlState.sourceType]}`,
        })
    }
    if (urlState.availabilityStatus) {
        chips.push({
            key: "availabilityStatus",
            label: `当前可供：${AVAILABILITY_STATUS_LABELS[urlState.availabilityStatus]}`,
        })
    }
    if (urlState.ownerUserIds) {
        chips.push({
            key: "ownerUserIds",
            label: `维护人：已选 ${selectedCount(urlState.ownerUserIds)} 人`,
        })
    }
    if (urlState.procurementOwnerUserIds) {
        chips.push({
            key: "procurementOwnerUserIds",
            label: `采购负责人：已选 ${selectedCount(urlState.procurementOwnerUserIds)} 人`,
        })
    }
    if (urlState.orgUnitIds) {
        chips.push({
            key: "orgUnitIds",
            label: `业务组织：已选 ${selectedCount(urlState.orgUnitIds)} 个${urlState.includeDescendants ? "（含下级）" : ""}`,
        })
    }
    return chips
}

/**
 * 供应商供给列表页的 URL 状态、筛选草稿与导航补丁。
 *
 * 契约：已生效筛选全部由 URL 派生；草稿只在提交时写入 URL，
 * 后退/前进/清除通过 URL 回填草稿。面板初始关闭，深链不自动打开。
 * 取消、关闭、Esc 和外点只恢复低频草稿，保留搜索和常驻草稿。
 */
export function useSupplierOfferingsPageState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    /** 稳定序列化签名派生 Applied 状态，避免每次渲染重复回填（§5.4）。 */
    const appliedQuery = searchParams.toString()
    const urlState = React.useMemo(
        () =>
            parseSupplierOfferingsSearchParams(
                new URLSearchParams(appliedQuery),
            ),
        [appliedQuery],
    )
    const skuLocked = Boolean(urlState.skuId && urlState.returnTo)
    const taskMode = Boolean(urlState.workItemId)
    const hasStructuredFilters = Boolean(
        (!skuLocked && urlState.skuId) ||
        urlState.skuNo ||
        urlState.productNo ||
        urlState.supplierId ||
        urlState.status ||
        urlState.sourceType ||
        urlState.availabilityStatus ||
        urlState.ownerUserIds ||
        urlState.procurementOwnerUserIds ||
        urlState.orgUnitIds ||
        urlState.includeDescendants,
    )
    /** 已生效筛选包含来源锁定条件：查询消费的全部参数都计入（§12.6）。 */
    const hasFilters = Boolean(
        urlState.q ||
        urlState.skuId ||
        urlState.skuNo ||
        urlState.productNo ||
        urlState.supplierId ||
        urlState.status ||
        urlState.sourceType ||
        urlState.availabilityStatus ||
        urlState.ownerUserIds ||
        urlState.procurementOwnerUserIds ||
        urlState.orgUnitIds ||
        urlState.includeDescendants,
    )
    const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")
    const [skuIdDraft, setSkuIdDraft] = React.useState<string | null>(
        urlState.skuId ?? null,
    )
    const [skuNoDraft, setSkuNoDraft] = React.useState(urlState.skuNo ?? "")
    const [productNoDraft, setProductNoDraft] = React.useState(
        urlState.productNo ?? "",
    )
    const [supplierIdDraft, setSupplierIdDraft] = React.useState<string | null>(
        urlState.supplierId ?? null,
    )
    const [sourceTypeDraft, setSourceTypeDraft] =
        React.useState<OfferingSourceFilter>(urlState.sourceType ?? "all")
    const [availabilityStatusDraft, setAvailabilityStatusDraft] =
        React.useState<AvailabilityStatusFilter>(
            urlState.availabilityStatus ?? "all",
        )
    const [ownerUserIdsDraft, setOwnerUserIdsDraft] = React.useState(
        urlState.ownerUserIds ?? "",
    )
    const [procurementOwnerUserIdsDraft, setProcurementOwnerUserIdsDraft] =
        React.useState(urlState.procurementOwnerUserIds ?? "")
    const [orgUnitIdsDraft, setOrgUnitIdsDraft] = React.useState(
        urlState.orgUnitIds ?? "",
    )
    const [includeDescendantsDraft, setIncludeDescendantsDraft] =
        React.useState(Boolean(urlState.includeDescendants))
    /** 深链条件显示为已生效标签，不自动打开浮层。 */
    const [filterPanelOpen, setFilterPanelOpen] = React.useState(false)

    /** 合并 URL 补丁并保留未变的导航上下文。 */
    const patchUrl = React.useCallback(
        (patch: Partial<SupplierOfferingsUrlState>) => {
            const next = { ...urlState, ...patch }
            router.replace(
                `${pathname}${buildSupplierOfferingsSearchParams(next)}`,
                { scroll: false },
            )
        },
        [pathname, router, urlState],
    )

    /** 一次提交关键词与全部结构化筛选草稿；成功后收起面板（§8.1）。 */
    const applyFilters = React.useCallback(() => {
        const orgUnitIds = orgUnitIdsDraft.trim()
        if (!orgUnitIds) setIncludeDescendantsDraft(false)
        patchUrl({
            q: searchDraft.trim() || undefined,
            skuId: skuIdDraft || undefined,
            skuNo: skuNoDraft.trim() || undefined,
            productNo: productNoDraft.trim() || undefined,
            supplierId: supplierIdDraft || undefined,
            sourceType: sourceTypeDraft === "all" ? undefined : sourceTypeDraft,
            availabilityStatus:
                availabilityStatusDraft === "all"
                    ? undefined
                    : availabilityStatusDraft,
            ownerUserIds: ownerUserIdsDraft.trim() || undefined,
            procurementOwnerUserIds:
                procurementOwnerUserIdsDraft.trim() || undefined,
            orgUnitIds: orgUnitIds || undefined,
            includeDescendants:
                orgUnitIds && includeDescendantsDraft ? true : undefined,
            scopeVersion: undefined,
            page: 1,
        })
        setFilterPanelOpen(false)
    }, [
        availabilityStatusDraft,
        includeDescendantsDraft,
        orgUnitIdsDraft,
        ownerUserIdsDraft,
        patchUrl,
        procurementOwnerUserIdsDraft,
        productNoDraft,
        searchDraft,
        skuIdDraft,
        skuNoDraft,
        sourceTypeDraft,
        supplierIdDraft,
    ])

    /** 仅移除商品页带入的公司 SKU 限定。 */
    const clearSkuLock = React.useCallback(() => {
        setSkuIdDraft(null)
        patchUrl({ skuId: undefined, page: 1 })
    }, [patchUrl])

    /** 移除单个已生效条件；chip 的 × 只移除自己的条件（§8.1）。 */
    const removeFilter = React.useCallback(
        (key: SupplierOfferingFilterKey) => {
            if (key === "q") {
                setSearchDraft("")
                patchUrl({ q: undefined, page: 1 })
                return
            }
            if (key === "skuId") {
                clearSkuLock()
                return
            }
            if (key === "skuNo") {
                setSkuNoDraft("")
                patchUrl({ skuNo: undefined, page: 1 })
                return
            }
            if (key === "productNo") {
                setProductNoDraft("")
                patchUrl({ productNo: undefined, page: 1 })
                return
            }
            if (key === "supplierId") {
                setSupplierIdDraft(null)
                patchUrl({ supplierId: undefined, page: 1 })
                return
            }
            if (key === "sourceType") {
                setSourceTypeDraft("all")
                patchUrl({
                    sourceType: undefined,
                    page: 1,
                    scopeVersion: undefined,
                })
                return
            }
            if (key === "ownerUserIds") {
                setOwnerUserIdsDraft("")
                patchUrl({
                    ownerUserIds: undefined,
                    page: 1,
                    scopeVersion: undefined,
                })
                return
            }
            if (key === "procurementOwnerUserIds") {
                setProcurementOwnerUserIdsDraft("")
                patchUrl({
                    procurementOwnerUserIds: undefined,
                    page: 1,
                    scopeVersion: undefined,
                })
                return
            }
            if (key === "orgUnitIds") {
                setOrgUnitIdsDraft("")
                setIncludeDescendantsDraft(false)
                patchUrl({
                    orgUnitIds: undefined,
                    includeDescendants: undefined,
                    page: 1,
                    scopeVersion: undefined,
                })
                return
            }
            setAvailabilityStatusDraft("all")
            patchUrl({
                availabilityStatus: undefined,
                page: 1,
                scopeVersion: undefined,
            })
        },
        [clearSkuLock, patchUrl],
    )

    /**
     * 仅重置低频草稿；保留关键词、常驻条件与已生效结果。
     * 商品页带入的 skuId 属于导航上下文，不在此清除。
     */
    const resetMoreFilters = React.useCallback(() => {
        setSkuNoDraft("")
        setProductNoDraft("")
        setSourceTypeDraft("all")
        setOwnerUserIdsDraft("")
        setProcurementOwnerUserIdsDraft("")
        setOrgUnitIdsDraft("")
        setIncludeDescendantsDraft(false)
        if (!skuLocked) setSkuIdDraft(null)
    }, [skuLocked])

    /** 关闭面板时只撤销低频草稿；保留搜索、供应商和当前可供。 */
    const cancelMoreFilters = React.useCallback(() => {
        setSkuIdDraft(urlState.skuId ?? null)
        setSkuNoDraft(urlState.skuNo ?? "")
        setProductNoDraft(urlState.productNo ?? "")
        setSourceTypeDraft(urlState.sourceType ?? "all")
        setOwnerUserIdsDraft(urlState.ownerUserIds ?? "")
        setProcurementOwnerUserIdsDraft(urlState.procurementOwnerUserIds ?? "")
        setOrgUnitIdsDraft(urlState.orgUnitIds ?? "")
        setIncludeDescendantsDraft(Boolean(urlState.includeDescendants))
        setFilterPanelOpen(false)
    }, [
        urlState.includeDescendants,
        urlState.orgUnitIds,
        urlState.ownerUserIds,
        urlState.procurementOwnerUserIds,
        urlState.productNo,
        urlState.skuId,
        urlState.skuNo,
        urlState.sourceType,
    ])

    /**
     * 清空关键词与全部筛选参数并收起面板；商品页带入的 skuId 与 returnTo
     * 属于导航上下文，清除普通筛选时必须保留（W21 合同）。
     */
    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setSkuNoDraft("")
        setProductNoDraft("")
        setSupplierIdDraft(null)
        setSourceTypeDraft("all")
        setAvailabilityStatusDraft("all")
        setOwnerUserIdsDraft("")
        setProcurementOwnerUserIdsDraft("")
        setOrgUnitIdsDraft("")
        setIncludeDescendantsDraft(false)
        setFilterPanelOpen(false)
        if (!skuLocked) setSkuIdDraft(null)
        patchUrl({
            q: undefined,
            ...(skuLocked ? {} : { skuId: undefined }),
            skuNo: undefined,
            productNo: undefined,
            supplierId: undefined,
            status: undefined,
            sourceType: undefined,
            availabilityStatus: undefined,
            ownerUserIds: undefined,
            procurementOwnerUserIds: undefined,
            orgUnitIds: undefined,
            includeDescendants: undefined,
            scopeVersion: undefined,
            page: 1,
        })
    }, [patchUrl, skuLocked])

    // `/` 聚焦搜索框；Dialog / Sheet 打开时不得聚焦背景搜索框（§3.2、§14.4）。
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key === "/" &&
                !(event.target instanceof HTMLInputElement) &&
                !(event.target instanceof HTMLTextAreaElement)
            ) {
                if (
                    document.querySelector(
                        '[role="dialog"], [data-slot="sheet"]',
                    )
                ) {
                    return
                }
                event.preventDefault()
                searchInputRef.current?.focus()
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    // URL 回填关键词草稿（后退/前进/刷新/清除）；正在编辑时做焦点保护（§5.4）。
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(urlState.q ?? "")
        }
    }, [urlState.q, searchInputRef])

    // URL 回填结构化草稿；展开态由初始值、用户操作与提交结果管理（§5.4、§5.5）。
    React.useEffect(() => {
        setSkuIdDraft(urlState.skuId ?? null)
        setSkuNoDraft(urlState.skuNo ?? "")
        setProductNoDraft(urlState.productNo ?? "")
        setSupplierIdDraft(urlState.supplierId ?? null)
        setSourceTypeDraft(urlState.sourceType ?? "all")
        setAvailabilityStatusDraft(urlState.availabilityStatus ?? "all")
        setOwnerUserIdsDraft(urlState.ownerUserIds ?? "")
        setProcurementOwnerUserIdsDraft(urlState.procurementOwnerUserIds ?? "")
        setOrgUnitIdsDraft(urlState.orgUnitIds ?? "")
        setIncludeDescendantsDraft(Boolean(urlState.includeDescendants))
    }, [urlState])

    const appliedFilterLabels = [
        urlState.q ? `订货编码包含“${urlState.q}”` : null,
        !skuLocked && urlState.skuId ? "已选择公司 SKU" : null,
        urlState.skuNo ? `SKU 编号包含“${urlState.skuNo}”` : null,
        urlState.productNo ? `SPU 编号包含“${urlState.productNo}”` : null,
        urlState.supplierId ? "已选择供应商" : null,
        urlState.sourceType
            ? `登记来源：${SOURCE_TYPE_LABELS[urlState.sourceType]}`
            : null,
        urlState.availabilityStatus
            ? `当前可供：${AVAILABILITY_STATUS_LABELS[urlState.availabilityStatus]}`
            : null,
        urlState.ownerUserIds
            ? `维护人：已选 ${selectedCount(urlState.ownerUserIds)} 人`
            : null,
        urlState.procurementOwnerUserIds
            ? `采购负责人：已选 ${selectedCount(urlState.procurementOwnerUserIds)} 人`
            : null,
        urlState.orgUnitIds
            ? `业务组织：已选 ${selectedCount(urlState.orgUnitIds)} 个${urlState.includeDescendants ? "（含下级）" : ""}`
            : null,
    ].filter(Boolean)

    const hasPendingChanges =
        searchDraft.trim() !== (urlState.q ?? "") ||
        skuIdDraft !== (urlState.skuId ?? null) ||
        skuNoDraft.trim() !== (urlState.skuNo ?? "") ||
        productNoDraft.trim() !== (urlState.productNo ?? "") ||
        supplierIdDraft !== (urlState.supplierId ?? null) ||
        sourceTypeDraft !== (urlState.sourceType ?? "all") ||
        availabilityStatusDraft !== (urlState.availabilityStatus ?? "all") ||
        ownerUserIdsDraft !== (urlState.ownerUserIds ?? "") ||
        procurementOwnerUserIdsDraft !==
            (urlState.procurementOwnerUserIds ?? "") ||
        orgUnitIdsDraft !== (urlState.orgUnitIds ?? "") ||
        includeDescendantsDraft !== Boolean(urlState.includeDescendants)

    return {
        urlState,
        skuLocked,
        taskMode,
        hasStructuredFilters,
        hasFilters,
        searchInputRef,
        searchDraft,
        setSearchDraft,
        skuIdDraft,
        setSkuIdDraft,
        skuNoDraft,
        setSkuNoDraft,
        productNoDraft,
        setProductNoDraft,
        supplierIdDraft,
        setSupplierIdDraft,
        sourceTypeDraft,
        setSourceTypeDraft,
        availabilityStatusDraft,
        setAvailabilityStatusDraft,
        ownerUserIdsDraft,
        setOwnerUserIdsDraft,
        procurementOwnerUserIdsDraft,
        setProcurementOwnerUserIdsDraft,
        orgUnitIdsDraft,
        setOrgUnitIdsDraft,
        includeDescendantsDraft,
        setIncludeDescendantsDraft,
        filterPanelOpen,
        setFilterPanelOpen,
        patchUrl,
        applyFilters,
        clearFilters,
        clearSkuLock,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        appliedFilterLabels,
        hasPendingChanges,
    }
}
