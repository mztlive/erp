"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"

import { parseMetric } from "@/features/product-publications/lib/parse-metric"
import type { PublicationStatus } from "@/features/product-publications/types"
import { PUBLICATION_STATUS_LABEL } from "@/features/product-publications/types"

/** 发送状态快捷值（URL 与接口共用；"all" 表示不限）。 */
export type PublicationDeliveryStatusSelection =
    | "all"
    | "pending_confirm"
    | "failed"
    | "handoff"
    | "acked"

/** 可被单独移除的已生效条件。 */
export type PublicationFilterKey =
    | "q"
    | "mall"
    | "publicationStatus"
    | "deliveryStatus"
    | "metric"
    | "skuId"
    | "supplierOfferingRevisionId"

const DELIVERY_STATUS_VALUES: readonly PublicationDeliveryStatusSelection[] = [
    "pending_confirm",
    "failed",
    "handoff",
    "acked",
]

const PUBLICATION_STATUS_VALUES = Object.keys(
    PUBLICATION_STATUS_LABEL,
) as readonly PublicationStatus[]

type PublicationAppliedFilters = {
    q: string
    skuId?: string
    supplierOfferingRevisionId?: string
    mall?: string
    publicationStatus: PublicationStatus | "all"
    deliveryStatus: PublicationDeliveryStatusSelection
    metric: string
    page: number
}

/** URL 中的非法枚举值降级为默认值，不能继续传给接口。 */
function parsePublicationStatus(
    raw: string | null,
): PublicationStatus | "all" {
    return PUBLICATION_STATUS_VALUES.some((value) => value === raw)
        ? (raw as PublicationStatus)
        : "all"
}

function parseDeliveryStatus(
    raw: string | null,
): PublicationDeliveryStatusSelection {
    return DELIVERY_STATUS_VALUES.some((value) => value === raw)
        ? (raw as PublicationDeliveryStatusSelection)
        : "all"
}

function parsePublicationFilters(
    params: URLSearchParams,
): PublicationAppliedFilters {
    return {
        q: params.get("q")?.trim() ?? "",
        skuId: params.get("skuId")?.trim() || undefined,
        supplierOfferingRevisionId:
            params.get("supplierOfferingRevisionId")?.trim() || undefined,
        mall: params.get("mall")?.trim() || undefined,
        publicationStatus: parsePublicationStatus(
            params.get("publicationStatus"),
        ),
        deliveryStatus: parseDeliveryStatus(params.get("deliveryStatus")),
        metric: parseMetric(params.get("metric")),
        page: Math.max(1, Number(params.get("page") ?? "1") || 1),
    }
}

/** 是否存在已生效的「更多筛选」结构化条件。 */
function hasStructuredPublicationFilters(
    applied: PublicationAppliedFilters,
): boolean {
    return Boolean(
        applied.mall ||
            applied.publicationStatus !== "all" ||
            applied.deliveryStatus !== "all",
    )
}

/** 筛选参数清单：「清空全部」只清这些与分页，不误删排序/视图/导航上下文。 */
const FILTER_PARAM_KEYS = [
    "q",
    "skuId",
    "supplierOfferingRevisionId",
    "mall",
    "publicationStatus",
    "deliveryStatus",
    "metric",
] as const

/**
 * 列表页筛选三层状态：
 * Applied 只来自 URL（唯一事实源）；Draft 是本地受控草稿（不触发请求）；
 * 面板展开等 UI 态为本地 state。搜索提交与「更多筛选」共用一个 applyFilters。
 */
export function usePublicationListFilters() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const appliedQuery = searchParams.toString()
    const applied = React.useMemo(
        () => parsePublicationFilters(new URLSearchParams(appliedQuery)),
        [appliedQuery],
    )

    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [searchDraft, setSearchDraft] = React.useState(applied.q)
    const [mallDraft, setMallDraft] = React.useState<string | null>(
        applied.mall ?? null,
    )
    const [publicationStatusDraft, setPublicationStatusDraft] =
        React.useState<PublicationStatus | "all">(applied.publicationStatus)
    const [deliveryStatusDraft, setDeliveryStatusDraft] =
        React.useState<PublicationDeliveryStatusSelection>(
            applied.deliveryStatus,
        )
    // 深链带结构化条件时展开面板；之后的 URL 回填只同步草稿，不抢夺展开态
    const [panelOpen, setPanelOpen] = React.useState(
        hasStructuredPublicationFilters(applied),
    )
    // 本地分页大小：只影响查询页大小，不写 URL
    const [pageSize, setPageSize] = React.useState(20)

    const patchUrl = React.useCallback(
        (patch: Record<string, string | null>) => {
            const next = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "") next.delete(key)
                else next.set(key, value)
            }
            const query = next.toString()
            router.replace(query ? `${pathname}?${query}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    /** 以当前 URL 快照合并补丁；undefined / "all" 视为删除（指标条等外部快捷动作）。 */
    const replaceParams = React.useCallback(
        (patch: Record<string, string | undefined>) => {
            const next = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (!value || value === "all") next.delete(key)
                else next.set(key, value)
            }
            next.delete("page")
            const query = next.toString()
            router.replace(query ? `${pathname}?${query}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    /** 单一提交入口：收起态 Enter 与展开态「应用全部筛选」都走这里。 */
    const applyFilters = React.useCallback(() => {
        const patch: Record<string, string | null> = {
            q: searchDraft.trim() || null,
            mall: mallDraft,
            publicationStatus:
                publicationStatusDraft === "all"
                    ? null
                    : publicationStatusDraft,
            deliveryStatus:
                deliveryStatusDraft === "all" ? null : deliveryStatusDraft,
            page: null,
        }
        // 结构化条件与指标快捷互斥：提交更多筛选时清除指标，避免空结果陷阱
        if (
            mallDraft != null ||
            publicationStatusDraft !== "all" ||
            deliveryStatusDraft !== "all"
        ) {
            patch.metric = null
        }
        patchUrl(patch)
        setPanelOpen(false)
    }, [
        deliveryStatusDraft,
        mallDraft,
        patchUrl,
        publicationStatusDraft,
        searchDraft,
    ])

    /** 移除单个已生效条件；来源锁定（SKU / 固定供给）按来源整体移除。 */
    const removeFilter = React.useCallback(
        (key: PublicationFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "mall") setMallDraft(null)
            if (key === "publicationStatus") setPublicationStatusDraft("all")
            if (key === "deliveryStatus") setDeliveryStatusDraft("all")
            if (key === "skuId" || key === "supplierOfferingRevisionId") {
                patchUrl({
                    skuId: null,
                    supplierOfferingRevisionId: null,
                    page: null,
                })
                return
            }
            patchUrl({ [key]: null, page: null })
        },
        [patchUrl],
    )

    /** 只清「更多筛选」；保留关键词、来源锁定与指标快捷，保持面板展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setMallDraft(null)
        setPublicationStatusDraft("all")
        setDeliveryStatusDraft("all")
        patchUrl({
            mall: null,
            publicationStatus: null,
            deliveryStatus: null,
            page: null,
        })
    }, [patchUrl])

    /** 清空全部：草稿、面板、URL 筛选参数与分页一次重置；保留排序/视图/导航上下文。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setMallDraft(null)
        setPublicationStatusDraft("all")
        setDeliveryStatusDraft("all")
        setPanelOpen(false)
        const patch: Record<string, string | null> = { page: null }
        for (const key of FILTER_PARAM_KEYS) patch[key] = null
        patchUrl(patch)
    }, [patchUrl])

    // URL 回填：只同步草稿；不重置面板展开态（§5.4 / §5.5）
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(applied.q)
        }
        setMallDraft(applied.mall ?? null)
        setPublicationStatusDraft(applied.publicationStatus)
        setDeliveryStatusDraft(applied.deliveryStatus)
    }, [applied])

    // `/` 聚焦搜索：忽略输入框、文本域与弹层（Dialog / Sheet）
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

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            setPageSize(next.pageSize)
            patchUrl({
                page: next.pageIndex <= 0 ? null : String(next.pageIndex + 1),
            })
        },
        [patchUrl],
    )

    const hasActiveFilters = Boolean(
        applied.q ||
            applied.mall ||
            applied.skuId ||
            applied.supplierOfferingRevisionId ||
            applied.publicationStatus !== "all" ||
            applied.deliveryStatus !== "all" ||
            applied.metric !== "all",
    )

    return {
        qParam: applied.q,
        skuId: applied.skuId,
        supplierOfferingRevisionId: applied.supplierOfferingRevisionId,
        mallId: applied.mall,
        publicationStatus: applied.publicationStatus,
        deliveryStatus: applied.deliveryStatus,
        metric: applied.metric,
        page: applied.page,
        pageSize,
        searchInputRef,
        searchDraft,
        setSearchDraft,
        mallDraft,
        setMallDraft,
        publicationStatusDraft,
        setPublicationStatusDraft,
        deliveryStatusDraft,
        setDeliveryStatusDraft,
        panelOpen,
        setPanelOpen,
        hasActiveFilters,
        hasStructuredFilters: hasStructuredPublicationFilters(applied),
        applyFilters,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
        replaceParams,
        handlePaginationChange,
    }
}
