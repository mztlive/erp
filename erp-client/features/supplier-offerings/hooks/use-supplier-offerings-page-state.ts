"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"

import {
    buildSupplierOfferingsSearchParams,
    parseSupplierOfferingsSearchParams,
    type SupplierOfferingsUrlState,
} from "@/features/supplier-offerings/lib/url-state"
import {
    SOURCE_TYPE_LABELS,
    type AvailabilityStatus,
    type OfferingSourceType,
} from "@/features/supplier-offerings/types"
import { useSearchDraft } from "@/lib/use-search-draft"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

export type OfferingSourceFilter = OfferingSourceType | "all"
export type AvailabilityStatusFilter = AvailabilityStatus | "all"

/** 可被单独移除的已生效筛选条件。 */
export type SupplierOfferingFilterKey =
    | "skuId"
    | "skuNo"
    | "productNo"
    | "sourceType"
    | "ownerUserIds"
    | "procurementOwnerUserIds"

export type SupplierOfferingAppliedChip = Readonly<{
    key: SupplierOfferingFilterKey
    label: string
}>

function selectedCount(value: string) {
    return value.split(",").filter((part) => part.trim()).length
}

/** 深链与折叠条件用业务编号显示，不暴露内部 ID。 */
export function buildSupplierOfferingAppliedChips(
    urlState: SupplierOfferingsUrlState,
    skuNoLabel?: string | null,
): readonly SupplierOfferingAppliedChip[] {
    const chips: SupplierOfferingAppliedChip[] = []
    if (urlState.skuId) {
        chips.push({
            key: "skuId",
            label: `公司 SKU：${skuNoLabel ?? "已选择"}`,
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
    if (urlState.sourceType) {
        chips.push({
            key: "sourceType",
            label: `登记来源：${SOURCE_TYPE_LABELS[urlState.sourceType]}`,
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
    return chips
}

/**
 * 供应商供给列表页的 URL 状态、筛选草稿与导航补丁。
 *
 * 契约：已生效筛选全部由 URL 派生；草稿只在提交时写入 URL，
 * 后退/前进/清除通过 URL 回填草稿。面板初始关闭，深链不自动打开。
 * 关闭托盘只恢复低频草稿，保留关键词和常驻条件。
 */
export function useSupplierOfferingsPageState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const canFilterSupplier = hasPermission(permissions, "supplier:list")
    const canFilterOwner = hasPermission(permissions, "business_person:list")
    const canFilterProcurementOwner = hasPermission(
        permissions,
        "procurement_person:list",
    )
    const canFilterSku = hasPermission(permissions, "sku:list")
    const sanitizeFilters = React.useCallback(
        (values: SupplierOfferingsUrlState): SupplierOfferingsUrlState => ({
            ...values,
            supplierId: canFilterSupplier ? values.supplierId : undefined,
            skuId: canFilterSku || values.returnTo ? values.skuId : undefined,
            ownerUserIds: canFilterOwner ? values.ownerUserIds : undefined,
            procurementOwnerUserIds: canFilterProcurementOwner
                ? values.procurementOwnerUserIds
                : undefined,
        }),
        [
            canFilterSupplier,
            canFilterSku,
            canFilterOwner,
            canFilterProcurementOwner,
        ],
    )
    const hasUnauthorizedFilters =
        (!canFilterSupplier && searchParams.has("supplierId")) ||
        (!canFilterSku &&
            !searchParams.get("returnTo") &&
            searchParams.has("skuId")) ||
        (!canFilterOwner && searchParams.has("owner_user_ids")) ||
        (!canFilterProcurementOwner &&
            searchParams.has("procurement_owner_user_ids"))
    const urlState = React.useMemo(() => {
        const next = sanitizeFilters(
            parseSupplierOfferingsSearchParams(searchParams),
        )
        return hasUnauthorizedFilters
            ? { ...next, page: 1, scopeVersion: undefined }
            : next
    }, [hasUnauthorizedFilters, sanitizeFilters, searchParams])
    const skuLocked = Boolean(urlState.skuId && urlState.returnTo)
    const taskMode = Boolean(urlState.workItemId)
    const hasClearableFilters = Boolean(
        urlState.q ||
        (!skuLocked && urlState.skuId) ||
        urlState.skuNo ||
        urlState.productNo ||
        urlState.supplierId ||
        urlState.status ||
        urlState.sourceType ||
        urlState.availabilityStatus ||
        urlState.ownerUserIds ||
        urlState.procurementOwnerUserIds,
    )
    const { searchDraft, setSearchDraft } = useSearchDraft(urlState.q, {
        inputRef: searchInputRef,
    })
    useSearchHotkey(searchInputRef)
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
    /** 深链条件显示为已生效标签，不自动打开浮层。 */
    const [filterPanelOpen, setFilterPanelOpen] = React.useState(false)

    /** 合并 URL 补丁并保留未变的导航上下文。 */
    const patchUrl = React.useCallback(
        (patch: Partial<SupplierOfferingsUrlState>) => {
            const next = sanitizeFilters({ ...urlState, ...patch })
            router.replace(
                `${pathname}${buildSupplierOfferingsSearchParams(next)}`,
                { scroll: false },
            )
        },
        [pathname, router, sanitizeFilters, urlState],
    )

    React.useEffect(() => {
        if (account.data && hasUnauthorizedFilters) patchUrl({ page: 1 })
    }, [account.data, hasUnauthorizedFilters, patchUrl])

    /** 一次提交关键词与全部结构化筛选草稿；成功后收起面板（§8.1）。 */
    const applyFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || undefined,
            skuId: skuLocked ? urlState.skuId : skuIdDraft || undefined,
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
            scopeVersion: undefined,
            page: 1,
        })
        setFilterPanelOpen(false)
    }, [
        availabilityStatusDraft,
        ownerUserIdsDraft,
        patchUrl,
        procurementOwnerUserIdsDraft,
        productNoDraft,
        searchDraft,
        skuIdDraft,
        skuLocked,
        skuNoDraft,
        sourceTypeDraft,
        supplierIdDraft,
        urlState.skuId,
    ])

    const removeFilter = React.useCallback(
        (key: SupplierOfferingFilterKey) => {
            if (key === "skuId") setSkuIdDraft(null)
            if (key === "skuNo") setSkuNoDraft("")
            if (key === "productNo") setProductNoDraft("")
            if (key === "sourceType") setSourceTypeDraft("all")
            if (key === "ownerUserIds") setOwnerUserIdsDraft("")
            if (key === "procurementOwnerUserIds")
                setProcurementOwnerUserIdsDraft("")
            patchUrl({ [key]: undefined, page: 1, scopeVersion: undefined })
        },
        [patchUrl],
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
        setFilterPanelOpen(false)
    }, [
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
            scopeVersion: undefined,
            page: 1,
        })
    }, [patchUrl, setSearchDraft, skuLocked])

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
    }, [
        urlState.skuId,
        urlState.skuNo,
        urlState.productNo,
        urlState.supplierId,
        urlState.sourceType,
        urlState.availabilityStatus,
        urlState.ownerUserIds,
        urlState.procurementOwnerUserIds,
    ])

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
            (urlState.procurementOwnerUserIds ?? "")

    return {
        canFilterSupplier,
        canFilterOwner,
        canFilterProcurementOwner,
        canFilterSku,
        hasUnauthorizedFilters,
        urlState,
        skuLocked,
        taskMode,
        hasClearableFilters,
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
        filterPanelOpen,
        setFilterPanelOpen,
        patchUrl,
        applyFilters,
        clearFilters,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        hasPendingChanges,
    }
}
