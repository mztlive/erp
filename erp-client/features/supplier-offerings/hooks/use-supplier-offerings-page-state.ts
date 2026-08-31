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
    OFFERING_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
    type AvailabilityStatus,
    type OfferingSourceType,
    type OfferingStatus,
} from "@/features/supplier-offerings/types"

export type OfferingStatusFilter = OfferingStatus | "all"
export type OfferingSourceFilter = OfferingSourceType | "all"
export type AvailabilityStatusFilter = AvailabilityStatus | "all"

/** 可被单独移除的已生效筛选条件。 */
export type SupplierOfferingFilterKey =
    | "q"
    | "skuId"
    | "skuNo"
    | "productNo"
    | "supplierId"
    | "status"
    | "sourceType"
    | "availabilityStatus"

export type SupplierOfferingAppliedChip = Readonly<{
    key: SupplierOfferingFilterKey
    label: string
}>

/**
 * 把全部已生效条件派生为可单独移除的 chip（docs/ui-filter-design.md §3.6）。
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
    if (urlState.status) {
        chips.push({
            key: "status",
            label: `关系状态：${OFFERING_STATUS_LABELS[urlState.status]}`,
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
    return chips
}

/**
 * 供应商供给列表页的 URL 状态、筛选草稿与导航补丁。
 *
 * 契约：已生效筛选全部由 URL 派生；草稿只在提交时写入 URL，
 * 后退/前进/清除通过 URL 回填草稿。面板展开态属于 UI 状态，
 * URL 回填只同步草稿，不抢夺当前展开态（docs/ui-filter-design.md §5）。
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
        urlState.availabilityStatus,
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
        urlState.availabilityStatus,
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
    const [statusDraft, setStatusDraft] = React.useState<OfferingStatusFilter>(
        urlState.status ?? "all",
    )
    const [sourceTypeDraft, setSourceTypeDraft] =
        React.useState<OfferingSourceFilter>(urlState.sourceType ?? "all")
    const [availabilityStatusDraft, setAvailabilityStatusDraft] =
        React.useState<AvailabilityStatusFilter>(
            urlState.availabilityStatus ?? "all",
        )
    /** 初始深链带结构化条件时展开；此后展开态只由用户与提交结果控制（§5.5）。 */
    const [filterPanelOpen, setFilterPanelOpen] =
        React.useState(hasStructuredFilters)

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
        patchUrl({
            q: searchDraft.trim() || undefined,
            skuId: skuIdDraft || undefined,
            skuNo: skuNoDraft.trim() || undefined,
            productNo: productNoDraft.trim() || undefined,
            supplierId: supplierIdDraft || undefined,
            status: statusDraft === "all" ? undefined : statusDraft,
            sourceType: sourceTypeDraft === "all" ? undefined : sourceTypeDraft,
            availabilityStatus:
                availabilityStatusDraft === "all"
                    ? undefined
                    : availabilityStatusDraft,
            page: 1,
        })
        setFilterPanelOpen(false)
    }, [
        availabilityStatusDraft,
        patchUrl,
        productNoDraft,
        searchDraft,
        skuIdDraft,
        skuNoDraft,
        sourceTypeDraft,
        statusDraft,
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
            if (key === "status") {
                setStatusDraft("all")
                patchUrl({ status: undefined, page: 1 })
                return
            }
            if (key === "sourceType") {
                setSourceTypeDraft("all")
                patchUrl({ sourceType: undefined, page: 1 })
                return
            }
            setAvailabilityStatusDraft("all")
            patchUrl({ availabilityStatus: undefined, page: 1 })
        },
        [clearSkuLock, patchUrl],
    )

    /**
     * 仅清除「更多筛选」面板内的结构化条件；保留关键词，
     * 商品页带入的 skuId 属于导航上下文一并保留；面板保持展开（§5.6）。
     */
    const resetMoreFilters = React.useCallback(() => {
        setSkuNoDraft("")
        setProductNoDraft("")
        setSupplierIdDraft(null)
        setStatusDraft("all")
        setSourceTypeDraft("all")
        setAvailabilityStatusDraft("all")
        if (!skuLocked) setSkuIdDraft(null)
        patchUrl({
            ...(skuLocked ? {} : { skuId: undefined }),
            skuNo: undefined,
            productNo: undefined,
            supplierId: undefined,
            status: undefined,
            sourceType: undefined,
            availabilityStatus: undefined,
            page: 1,
        })
    }, [patchUrl, skuLocked])

    /**
     * 清空关键词与全部筛选参数并收起面板；商品页带入的 skuId 与 returnTo
     * 属于导航上下文，清除普通筛选时必须保留（W21 合同）。
     */
    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setSkuNoDraft("")
        setProductNoDraft("")
        setSupplierIdDraft(null)
        setStatusDraft("all")
        setSourceTypeDraft("all")
        setAvailabilityStatusDraft("all")
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
        setStatusDraft(urlState.status ?? "all")
        setSourceTypeDraft(urlState.sourceType ?? "all")
        setAvailabilityStatusDraft(urlState.availabilityStatus ?? "all")
    }, [urlState])

    const appliedFilterLabels = [
        urlState.q ? `订货编码包含“${urlState.q}”` : null,
        !skuLocked && urlState.skuId ? "已选择公司 SKU" : null,
        urlState.skuNo ? `SKU 编号包含“${urlState.skuNo}”` : null,
        urlState.productNo ? `SPU 编号包含“${urlState.productNo}”` : null,
        urlState.supplierId ? "已选择供应商" : null,
        urlState.status
            ? `关系状态：${OFFERING_STATUS_LABELS[urlState.status]}`
            : null,
        urlState.sourceType
            ? `登记来源：${SOURCE_TYPE_LABELS[urlState.sourceType]}`
            : null,
        urlState.availabilityStatus
            ? `当前可供：${AVAILABILITY_STATUS_LABELS[urlState.availabilityStatus]}`
            : null,
    ].filter(Boolean)

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
        statusDraft,
        setStatusDraft,
        sourceTypeDraft,
        setSourceTypeDraft,
        availabilityStatusDraft,
        setAvailabilityStatusDraft,
        filterPanelOpen,
        setFilterPanelOpen,
        patchUrl,
        applyFilters,
        clearFilters,
        clearSkuLock,
        removeFilter,
        resetMoreFilters,
        appliedFilterLabels,
    }
}
