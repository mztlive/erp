"use client"

import * as React from "react"

import type { ConnectionsUrlState } from "@/features/supplier-api-connections/lib/url-state"
import {
    CAPABILITY_LABEL,
    CATALOG_LABEL,
    HEALTH_LABEL,
    type CapabilityCode,
    type CatalogFreshnessState,
    type ConnectionEnvironment,
    type ConnectionStatus,
    type HealthResult,
} from "@/features/supplier-api-connections/types"

/** 可被单独移除的已生效条件。 */
export type ConnectionFilterKey =
    | "q"
    | "health"
    | "capability"
    | "catalogFreshness"
    | "supplierId"

export type ConnectionAppliedChip = Readonly<{
    key: ConnectionFilterKey
    label: string
}>

const STATUS_VALUES: readonly ConnectionStatus[] = [
    "ENABLED",
    "DISABLED",
    "FAULTED",
    "PENDING_CONFIG",
]
const HEALTH_VALUES: readonly HealthResult[] = [
    "SUCCESS",
    "FAILED",
    "PARTIAL",
    "UNCHECKED",
    "STALE",
    "AUTH_FAILED",
    "UNKNOWN",
]
const CATALOG_FRESHNESS_VALUES: readonly CatalogFreshnessState[] = [
    "FRESH",
    "STALE",
    "RUNNING",
    "FAILED",
    "NEVER",
]

/** 非法枚举值在解析时降级为默认（docs/ui-filter-design.md §6.1）。 */
function parseStatus(raw: string | undefined): ConnectionStatus | undefined {
    const value = raw?.trim()
    return (STATUS_VALUES as readonly string[]).includes(value ?? "")
        ? (value as ConnectionStatus)
        : undefined
}

function parseMulti(
    raw: string | undefined,
    values: readonly string[],
): string[] {
    if (!raw) return []
    return raw
        .split(",")
        .map((value) => value.trim())
        .filter((value) => values.includes(value))
}

export type ConnectionAppliedFilters = {
    q?: string
    status?: ConnectionStatus
    health: HealthResult[]
    capability?: CapabilityCode
    catalogFreshness: CatalogFreshnessState[]
    supplierId?: string
}

/** URL 是 Applied 状态唯一事实源；解析时丢弃非法枚举值。 */
export function parseConnectionAppliedFilters(
    urlState: ConnectionsUrlState,
): ConnectionAppliedFilters {
    const q = urlState.q?.trim() || undefined
    const status = parseStatus(urlState.status)
    const health = parseMulti(urlState.health, HEALTH_VALUES) as HealthResult[]
    const capability = (
        Object.keys(CAPABILITY_LABEL) as readonly string[]
    ).includes(urlState.capability?.trim() ?? "")
        ? (urlState.capability!.trim() as CapabilityCode)
        : undefined
    const catalogFreshness = parseMulti(
        urlState.catalogFreshness,
        CATALOG_FRESHNESS_VALUES,
    ) as CatalogFreshnessState[]
    const supplierId = urlState.supplierId?.trim() || undefined
    return { q, status, health, capability, catalogFreshness, supplierId }
}

/**
 * 连接列表筛选状态：Applied 由 URL 派生，Draft 本地受控，面板展开属于 UI 态
 * （docs/ui-filter-design.md §5）。environment 属视图类参数（W20 §6），
 * 由主行快捷筛选直接写 URL，不被「清除全部」清除。
 */
export function useConnectionListFilters(
    urlState: ConnectionsUrlState,
    patchUrl: (patch: Partial<ConnectionsUrlState>) => void,
) {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    const applied = React.useMemo(
        () => parseConnectionAppliedFilters(urlState),
        [urlState],
    )
    const hasStructuredFilters = Boolean(
        applied.status ||
        applied.health.length > 0 ||
        applied.capability ||
        applied.catalogFreshness.length > 0 ||
        applied.supplierId,
    )
    const hasFilters = Boolean(applied.q || hasStructuredFilters)

    const [searchDraft, setSearchDraft] = React.useState(applied.q ?? "")
    const [healthDraft, setHealthDraft] = React.useState<string[]>(
        applied.health,
    )
    const [capabilityDraft, setCapabilityDraft] = React.useState<string>(
        applied.capability ?? "",
    )
    const [catalogFreshnessDraft, setCatalogFreshnessDraft] = React.useState<
        string[]
    >(applied.catalogFreshness)
    const [supplierIdDraft, setSupplierIdDraft] = React.useState<string | null>(
        applied.supplierId ?? null,
    )
    /** 深链条件显示为标签，不自动打开浮层。 */
    const [filterPanelOpen, setFilterPanelOpen] = React.useState(false)

    /** 一次提交关键词与全部结构化筛选草稿；成功后收起面板（§8.1）。 */
    const applyFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || undefined,
            health: healthDraft.length > 0 ? healthDraft.join(",") : undefined,
            capability: capabilityDraft || undefined,
            catalogFreshness:
                catalogFreshnessDraft.length > 0
                    ? catalogFreshnessDraft.join(",")
                    : undefined,
            supplierId: supplierIdDraft || undefined,
            page: 1,
        })
        setFilterPanelOpen(false)
    }, [
        capabilityDraft,
        catalogFreshnessDraft,
        healthDraft,
        patchUrl,
        searchDraft,
        supplierIdDraft,
    ])

    /** 环境是视图类参数：主行快捷筛选直接写 URL（W20 §6）。 */
    const applyEnvironment = React.useCallback(
        (next: ConnectionEnvironment | "ALL") => {
            patchUrl({ environment: next, page: 1 })
        },
        [patchUrl],
    )

    /** 移除单个已生效条件；chip 的 × 只移除自己的条件（§8.1）。 */
    const removeFilter = React.useCallback(
        (key: ConnectionFilterKey) => {
            if (key === "q") {
                setSearchDraft("")
                patchUrl({ q: undefined, page: 1 })
                return
            }
            if (key === "health") {
                setHealthDraft([])
                patchUrl({ health: undefined, page: 1 })
                return
            }
            if (key === "capability") {
                setCapabilityDraft("")
                patchUrl({ capability: undefined, page: 1 })
                return
            }
            if (key === "catalogFreshness") {
                setCatalogFreshnessDraft([])
                patchUrl({ catalogFreshness: undefined, page: 1 })
                return
            }
            setSupplierIdDraft(null)
            patchUrl({ supplierId: undefined, page: 1 })
        },
        [patchUrl],
    )

    /** 只清低频草稿；保留关键词、供应商、状态、环境与已生效结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setHealthDraft([])
        setCapabilityDraft("")
        setCatalogFreshnessDraft([])
    }, [])

    /** 取消、关闭、Esc 和外点只恢复低频草稿，保留搜索和供应商。 */
    const cancelMoreFilters = React.useCallback(() => {
        setHealthDraft(applied.health)
        setCapabilityDraft(applied.capability ?? "")
        setCatalogFreshnessDraft(applied.catalogFreshness)
        setFilterPanelOpen(false)
    }, [applied])

    /**
     * 清空关键词与全部筛选参数并收起面板；environment 属视图类参数保留
     * （W20 §6），pageSize 与连接/导航上下文一并保留（§5.6）。
     */
    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setHealthDraft([])
        setCapabilityDraft("")
        setCatalogFreshnessDraft([])
        setSupplierIdDraft(null)
        setFilterPanelOpen(false)
        patchUrl({
            q: undefined,
            status: undefined,
            health: undefined,
            capability: undefined,
            catalogFreshness: undefined,
            supplierId: undefined,
            page: 1,
        })
    }, [patchUrl])

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
            setSearchDraft(applied.q ?? "")
        }
    }, [applied.q, searchInputRef])

    // URL 回填结构化草稿；不触碰面板展开态（§5.4、§5.5）。
    React.useEffect(() => {
        setHealthDraft(applied.health)
        setCapabilityDraft(applied.capability ?? "")
        setCatalogFreshnessDraft(applied.catalogFreshness)
        setSupplierIdDraft(applied.supplierId ?? null)
    }, [applied])

    /** 表头人读筛选摘要；只读 Applied（§2.1、§14.3）。 */
    const appliedFilterLabels = [
        applied.q ? `搜索“${applied.q}”` : null,
        applied.health.length > 0
            ? `健康：${applied.health
                  .map((value) => HEALTH_LABEL[value])
                  .join("、")}`
            : null,
        applied.capability
            ? `能力：${CAPABILITY_LABEL[applied.capability]}`
            : null,
        applied.catalogFreshness.length > 0
            ? `目录更新时间：${applied.catalogFreshness
                  .map((value) => CATALOG_LABEL[value])
                  .join("、")}`
            : null,
        applied.supplierId ? "已选择供应商" : null,
    ].filter((label): label is string => label !== null)

    const hasPendingChanges =
        searchDraft.trim() !== (applied.q ?? "") ||
        [...healthDraft].sort().join(",") !==
            [...applied.health].sort().join(",") ||
        capabilityDraft !== (applied.capability ?? "") ||
        [...catalogFreshnessDraft].sort().join(",") !==
            [...applied.catalogFreshness].sort().join(",") ||
        supplierIdDraft !== (applied.supplierId ?? null)

    return {
        searchInputRef,
        applied,
        hasStructuredFilters,
        hasFilters,
        searchDraft,
        setSearchDraft,
        healthDraft,
        setHealthDraft,
        capabilityDraft,
        setCapabilityDraft,
        catalogFreshnessDraft,
        setCatalogFreshnessDraft,
        supplierIdDraft,
        setSupplierIdDraft,
        filterPanelOpen,
        setFilterPanelOpen,
        applyFilters,
        applyEnvironment,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearFilters,
        appliedFilterLabels,
        hasPendingChanges,
    }
}

/**
 * 全部已生效条件派生为可移除 chip；供应商展示业务名称，不展示内部 ID（§4.5）。
 */
export function buildConnectionAppliedChips(
    urlState: ConnectionsUrlState,
    supplierNameLabel?: string,
): readonly ConnectionAppliedChip[] {
    const applied = parseConnectionAppliedFilters(urlState)
    const chips: ConnectionAppliedChip[] = []
    if (applied.q) {
        chips.push({ key: "q", label: `搜索：${applied.q}` })
    }
    if (applied.health.length > 0) {
        chips.push({
            key: "health",
            label: `健康：${applied.health
                .map((value) => HEALTH_LABEL[value])
                .join("、")}`,
        })
    }
    if (applied.capability) {
        chips.push({
            key: "capability",
            label: `能力：${CAPABILITY_LABEL[applied.capability]}`,
        })
    }
    if (applied.catalogFreshness.length > 0) {
        chips.push({
            key: "catalogFreshness",
            label: `目录更新时间：${applied.catalogFreshness
                .map((value) => CATALOG_LABEL[value])
                .join("、")}`,
        })
    }
    if (applied.supplierId) {
        chips.push({
            key: "supplierId",
            label: `供应商：${supplierNameLabel ?? "已选择"}`,
        })
    }
    return chips
}
