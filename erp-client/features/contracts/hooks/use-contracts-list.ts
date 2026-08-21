"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    computeContractMetrics,
    contractMetricLabel,
    filterContracts,
    type ContractMetricFilter,
} from "@/features/contracts/lib/filter-contracts"
import { sortRows } from "@/features/contracts/lib/contract-list-sort"
import {
    contractsUrlCodec,
    type ContractsUrlState,
} from "@/features/contracts/lib/contracts-url-state"
import type { ContractListRow } from "@/features/contracts/types"

/** 可被单独移除的已生效条件。 */
export type ContractFilterKey =
    | "q"
    | "metric"
    | "customerId"
    | "settlementPartyId"
    | "owner"

export type ContractAppliedChip = Readonly<{
    key: ContractFilterKey
    label: string
}>

/**
 * 合同列表 URL-first 状态（docs/ui-filter-design.md §5）：
 * Applied 在 URL（唯一事实源），Draft 本地受控不触发请求，UI 态（面板展开）本地保存。
 * 关键词与「更多筛选」草稿经显式提交（Enter / 应用全部筛选）一次性写 URL 并回第 1 页。
 */
export function useContractsList(
    rows: readonly ContractListRow[] | undefined,
) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    const allRows = React.useMemo(() => rows ?? [], [rows])

    // Applied：稳定序列化签名派生，避免每次 render 新对象导致重复回填
    const appliedQuery = searchParams.toString()
    const url = React.useMemo(
        () => contractsUrlCodec.parse(new URLSearchParams(appliedQuery)),
        [appliedQuery],
    )
    const {
        q,
        metric,
        page,
        pageSize,
        sort,
        dir,
        customerId,
        settlementPartyId,
        owner,
    } = url

    // Draft：本地受控，变化不请求
    const [searchDraft, setSearchDraft] = React.useState(q ?? "")
    const [settlementPartyIdDraft, setSettlementPartyIdDraft] =
        React.useState<string | null>(settlementPartyId ?? null)
    const [ownerDraft, setOwnerDraft] = React.useState<string | null>(
        owner ?? null,
    )

    const hasStructuredFilters = Boolean(settlementPartyId || owner)

    // UI 态：初始深链带结构化条件时展开；URL 回填不重置展开态
    const [panelOpen, setPanelOpen] = React.useState(hasStructuredFilters)

    const pushUrl = React.useCallback(
        (patch: Partial<ContractsUrlState>) => {
            const next = { ...url, ...patch }
            router.replace(`${pathname}${contractsUrlCodec.build(next)}`, {
                scroll: false,
            })
        },
        [pathname, router, url],
    )

    /** 唯一提交路径：收起态 Enter / 提交箭头与展开态「应用全部筛选」共用。 */
    const applyFilters = React.useCallback(() => {
        pushUrl({
            q: searchDraft.trim() || undefined,
            settlementPartyId: settlementPartyIdDraft ?? undefined,
            owner: ownerDraft ?? undefined,
            page: 1,
        })
        setPanelOpen(false)
    }, [ownerDraft, pushUrl, searchDraft, settlementPartyIdDraft])

    /** 只清「更多筛选」结构化条件；保留关键词、快捷筛选与客户锁定，保持面板展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setSettlementPartyIdDraft(null)
        setOwnerDraft(null)
        pushUrl({
            settlementPartyId: undefined,
            owner: undefined,
            page: 1,
        })
    }, [pushUrl])

    /** 移除单个已生效条件；每个条件都有可移除 chip。 */
    const removeFilter = React.useCallback(
        (key: ContractFilterKey) => {
            switch (key) {
                case "q":
                    setSearchDraft("")
                    pushUrl({ q: undefined, page: 1 })
                    break
                case "metric":
                    pushUrl({ metric: "all", page: 1 })
                    break
                case "customerId":
                    pushUrl({ customerId: undefined, page: 1 })
                    break
                case "settlementPartyId":
                    setSettlementPartyIdDraft(null)
                    pushUrl({ settlementPartyId: undefined, page: 1 })
                    break
                case "owner":
                    setOwnerDraft(null)
                    pushUrl({ owner: undefined, page: 1 })
                    break
            }
        },
        [pushUrl],
    )

    /** 清空全部筛选（含来源锁定与分页）；保留排序等视图/导航参数。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setSettlementPartyIdDraft(null)
        setOwnerDraft(null)
        setPanelOpen(false)
        pushUrl({
            q: undefined,
            metric: "all",
            customerId: undefined,
            settlementPartyId: undefined,
            owner: undefined,
            page: 1,
        })
    }, [pushUrl])

    // URL 回填：仅同步 Draft；面板展开态不在此重置
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(q ?? "")
        }
    }, [q])

    React.useEffect(() => {
        setSettlementPartyIdDraft(settlementPartyId ?? null)
        setOwnerDraft(owner ?? null)
    }, [owner, settlementPartyId])

    // `/` 聚焦搜索：忽略输入框/文本域/弹层（Dialog / Sheet）
    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key !== "/" || event.metaKey || event.ctrlKey) return
            const target = event.target as HTMLElement | null
            if (
                target &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)
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
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [])

    // 派生筛选只读 Applied（URL）
    const filtered = React.useMemo(() => {
        let rowsFiltered = filterContracts(allRows, {
            search: q ?? "",
            metricKey: metric,
            statusFilter: "all",
            settlementPartyId,
            owner,
        })
        if (customerId) {
            rowsFiltered = rowsFiltered.filter(
                (r) => r.customer.customerId === customerId,
            )
        }
        return rowsFiltered
    }, [allRows, customerId, metric, owner, q, settlementPartyId])

    const sorting = React.useMemo<SortingState>(
        () => (sort ? [{ id: sort, desc: dir === "desc" }] : []),
        [dir, sort],
    )

    const sorted = React.useMemo(
        () => sortRows(filtered, sorting),
        [filtered, sorting],
    )

    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: Math.max(0, page - 1), pageSize }),
        [page, pageSize],
    )

    const pageRows = React.useMemo(() => {
        const start = pagination.pageIndex * pagination.pageSize
        return sorted.slice(start, start + pagination.pageSize)
    }, [pagination.pageIndex, pagination.pageSize, sorted])

    const metrics = React.useMemo(
        () => computeContractMetrics(allRows),
        [allRows],
    )

    // 来源锁定与结构化条件的展示名；数据外深链回退为「未知」，仍提供可移除 chip
    const lockedCustomerLabel = React.useMemo(
        () =>
            allRows.find((r) => r.customer.customerId === customerId)?.customer
                .displayName ?? "未知",
        [allRows, customerId],
    )
    const selectedSettlementPartyLabel = React.useMemo(
        () =>
            allRows.find(
                (r) => r.settlementParty.partyId === settlementPartyId,
            )?.settlementParty.displayName ?? "未知",
        [allRows, settlementPartyId],
    )

    /** 全部已生效条件 → chip；查询、摘要、计数、导出只读 Applied。 */
    const appliedChips = React.useMemo<readonly ContractAppliedChip[]>(() => {
        const chips: ContractAppliedChip[] = []
        const trimmedQ = (q ?? "").trim()
        if (trimmedQ) chips.push({ key: "q", label: `搜索：${trimmedQ}` })
        if (metric !== "all") {
            chips.push({
                key: "metric",
                label: `指标：${contractMetricLabel(metric)}`,
            })
        }
        if (customerId) {
            chips.push({ key: "customerId", label: `客户：${lockedCustomerLabel}` })
        }
        if (settlementPartyId) {
            chips.push({
                key: "settlementPartyId",
                label: `结算主体：${selectedSettlementPartyLabel}`,
            })
        }
        if (owner) chips.push({ key: "owner", label: `负责人：${owner}` })
        return chips
    }, [
        customerId,
        lockedCustomerLabel,
        metric,
        owner,
        q,
        selectedSettlementPartyLabel,
        settlementPartyId,
    ])

    const filterDescription = React.useMemo(() => {
        const parts: string[] = []
        if (metric !== "all") parts.push(contractMetricLabel(metric))
        if ((q ?? "").trim()) parts.push(`“${(q ?? "").trim()}”`)
        if (customerId) parts.push(`客户：${lockedCustomerLabel}`)
        if (settlementPartyId) {
            parts.push(`结算主体：${selectedSettlementPartyLabel}`)
        }
        if (owner) parts.push(`负责人：${owner}`)
        return parts.length
            ? `当前筛选：${parts.join(" · ")}`
            : "按将到期优先排序展示当前业务范围内的合同。"
    }, [
        customerId,
        lockedCustomerLabel,
        metric,
        owner,
        q,
        selectedSettlementPartyLabel,
        settlementPartyId,
    ])

    const filterSnapshotLabel = React.useMemo(() => {
        const parts = [
            `指标=${contractMetricLabel(metric)}`,
            (q ?? "").trim() ? `搜索=${(q ?? "").trim()}` : "搜索=空",
            customerId ? `客户=${lockedCustomerLabel}` : null,
            settlementPartyId
                ? `结算主体=${selectedSettlementPartyLabel}`
                : null,
            owner ? `负责人=${owner}` : null,
        ].filter(Boolean)
        return parts.join(" · ")
    }, [
        customerId,
        lockedCustomerLabel,
        metric,
        owner,
        q,
        selectedSettlementPartyLabel,
        settlementPartyId,
    ])

    // 面板字段选项：由当前业务范围行派生，避免额外字典请求
    const settlementPartyOptions = React.useMemo(
        () =>
            [
                ...new Map(
                    allRows.map((r) => [
                        r.settlementParty.partyId,
                        r.settlementParty.displayName,
                    ]),
                ).entries(),
            ]
                .map(([value, label]) => ({ value, label }))
                .sort((a, b) => a.label.localeCompare(b.label, "zh-CN")),
        [allRows],
    )

    const ownerOptions = React.useMemo(
        () =>
            [...new Set(allRows.map((r) => r.ownerLabel))]
                .map((label) => ({ value: label, label }))
                .sort((a, b) => a.label.localeCompare(b.label, "zh-CN")),
        [allRows],
    )

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            pushUrl({ page: next.pageIndex + 1, pageSize: next.pageSize })
        },
        [pushUrl],
    )

    const handleMetricChange = React.useCallback(
        (next: ContractMetricFilter) => {
            pushUrl({ metric: next, page: 1 })
        },
        [pushUrl],
    )

    const handleSortingChange = React.useCallback(
        (next: SortingState) => {
            const head = next[0]
            pushUrl({
                sort: head?.id,
                dir: head ? (head.desc ? "desc" : "asc") : undefined,
                page: 1,
            })
        },
        [pushUrl],
    )

    const isFiltered =
        (q ?? "").trim() !== "" ||
        metric !== "all" ||
        Boolean(customerId) ||
        Boolean(settlementPartyId) ||
        Boolean(owner)

    return {
        url,
        q,
        metric,
        page,
        pageSize,
        sort,
        dir,
        customerId,
        settlementPartyId,
        owner,
        hasStructuredFilters,
        searchDraft,
        setSearchDraft,
        searchInputRef,
        settlementPartyIdDraft,
        setSettlementPartyIdDraft,
        ownerDraft,
        setOwnerDraft,
        panelOpen,
        setPanelOpen,
        filtered,
        sorting,
        sorted,
        pagination,
        pageRows,
        metrics,
        appliedChips,
        filterDescription,
        filterSnapshotLabel,
        settlementPartyOptions,
        ownerOptions,
        isFiltered,
        applyFilters,
        resetMoreFilters,
        removeFilter,
        handleMetricChange,
        handleSortingChange,
        handlePaginationChange,
        clearAllFilters,
    }
}
