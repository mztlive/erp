"use client"

import * as React from "react"

import { useConsumptionOrdersUrlState } from "@/features/mall-consumption-orders/hooks/use-consumption-orders-url-state"
import {
    EMPTY_MALL_CONSUMPTION_ORDER_FILTER_DRAFT,
    hasStructuredMallConsumptionFilters,
    toMallConsumptionFilterDraft,
    type MallConsumptionOrderApplied,
    type MallConsumptionOrderFilterDraft,
    type MallConsumptionOrderFilterKey,
} from "@/features/mall-consumption-orders/lib/filters"

/** chip 维度 → URL 参数名（与 lib/filters 的参数清单一致）。 */
const FILTER_PARAM_BY_KEY: Record<MallConsumptionOrderFilterKey, string> = {
    q: "q",
    mall: "mall",
    attributionStatus: "attributionStatus",
    fulfillmentChain: "fulfillmentChain",
    paymentSource: "paymentSource",
    costBasis: "costBasis",
    factTypes: "factType",
    supplierStatuses: "supplierStatus",
    dataSources: "dataSource",
    metric: "metric",
}

/**
 * W25 列表筛选状态（docs/ui-filter-design.md §5 状态模型）：
 * - Applied：URL（唯一事实源），由 useConsumptionOrdersUrlState 解析；
 * - Draft：面板字段本地受控草稿，变化不触发请求；
 * - UI：面板展开态本地 state。
 * 期间（occurredFrom/occurredTo）作为分析期间维度参与草稿与提交，
 * 但「清空全部」和「重置更多条件」都不清除它。
 */
export function useConsumptionOrdersFilters(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const url = useConsumptionOrdersUrlState()

    const applied = React.useMemo<MallConsumptionOrderApplied>(
        () => ({
            q: url.qParam,
            mallId: url.mallId,
            attributionStatus:
                url.attributionStatus as MallConsumptionOrderApplied["attributionStatus"],
            fulfillmentChain:
                url.fulfillmentChain as MallConsumptionOrderApplied["fulfillmentChain"],
            paymentSource:
                url.paymentSource as MallConsumptionOrderApplied["paymentSource"],
            costBasis:
                url.costBasis as MallConsumptionOrderApplied["costBasis"],
            factTypes: url.factTypes,
            supplierStatuses: url.supplierStatuses,
            dataSources: url.dataSources,
            occurredFrom: url.occurredFrom,
            occurredTo: url.occurredTo,
            metric: url.metric,
        }),
        [
            url.attributionStatus,
            url.costBasis,
            url.dataSources,
            url.factTypes,
            url.fulfillmentChain,
            url.mallId,
            url.metric,
            url.occurredFrom,
            url.occurredTo,
            url.paymentSource,
            url.qParam,
            url.supplierStatuses,
        ],
    )

    const [searchDraft, setSearchDraft] = React.useState(applied.q)
    const [filterDraft, setFilterDraft] =
        React.useState<MallConsumptionOrderFilterDraft>(() =>
            toMallConsumptionFilterDraft(applied),
        )
    // 初次带结构化条件的深链进入时展开面板；回填 effect 不重置展开态。
    const [panelOpen, setPanelOpen] = React.useState(
        hasStructuredMallConsumptionFilters(applied),
    )
    const hasStructuredFilters =
        hasStructuredMallConsumptionFilters(applied)

    // URL 回填：关键词草稿只跟随 q，输入框聚焦时不被覆盖。
    // 依赖只用 applied.q：ref 容器身份不稳定，进依赖会让 effect 每次渲染重跑，
    // 覆盖 clearAllFilters / removeFilter 已清空的关键词草稿。
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(applied.q)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [applied.q])

    // URL 回填：结构化草稿整体同步；面板展开态保持用户当前选择。
    React.useEffect(() => {
        setFilterDraft(toMallConsumptionFilterDraft(applied))
    }, [applied])

    /** 收起态 Enter / 尾部箭头与展开态「应用全部筛选」共用同一提交。 */
    const applyFilters = React.useCallback(() => {
        url.replaceParams(
            {
                q: searchDraft.trim() || undefined,
                mall: filterDraft.mallId || undefined,
                attributionStatus:
                    filterDraft.attributionStatus === "all"
                        ? undefined
                        : filterDraft.attributionStatus,
                fulfillmentChain:
                    filterDraft.fulfillmentChain === "all"
                        ? undefined
                        : filterDraft.fulfillmentChain,
                paymentSource:
                    filterDraft.paymentSource === "all"
                        ? undefined
                        : filterDraft.paymentSource,
                costBasis:
                    filterDraft.costBasis === "all"
                        ? undefined
                        : filterDraft.costBasis,
                factType: filterDraft.factTypes.length
                    ? filterDraft.factTypes.join(",")
                    : undefined,
                supplierStatus: filterDraft.supplierStatuses.length
                    ? filterDraft.supplierStatuses.join(",")
                    : undefined,
                dataSource: filterDraft.dataSources.length
                    ? filterDraft.dataSources.join(",")
                    : undefined,
                occurredFrom: filterDraft.occurredFrom || undefined,
                occurredTo: filterDraft.occurredTo || undefined,
            },
            true,
        )
        setPanelOpen(false)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [filterDraft, searchDraft, url.replaceParams])

    /** 移除单个已生效条件；草稿由 URL 回填 effect 同步。 */
    const removeFilter = React.useCallback(
        (key: MallConsumptionOrderFilterKey) => {
            if (key === "q") setSearchDraft("")
            url.replaceParams({ [FILTER_PARAM_BY_KEY[key]]: undefined }, true)
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [url.replaceParams],
    )

    /** 仅清除「更多筛选」；保留关键词、指标快捷筛选与期间，保持面板展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setFilterDraft((current) => ({
            ...EMPTY_MALL_CONSUMPTION_ORDER_FILTER_DRAFT,
            occurredFrom: current.occurredFrom,
            occurredTo: current.occurredTo,
        }))
        url.replaceParams(
            {
                mall: undefined,
                attributionStatus: undefined,
                fulfillmentChain: undefined,
                paymentSource: undefined,
                costBasis: undefined,
                factType: undefined,
                supplierStatus: undefined,
                dataSource: undefined,
            },
            true,
        )
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [url.replaceParams])

    /** 清空全部筛选参数并回第 1 页；保留排序、视图、期间与导航上下文。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setFilterDraft({
            ...EMPTY_MALL_CONSUMPTION_ORDER_FILTER_DRAFT,
            occurredFrom: applied.occurredFrom,
            occurredTo: applied.occurredTo,
        })
        setPanelOpen(false)
        url.replaceParams(
            {
                q: undefined,
                mall: undefined,
                attributionStatus: undefined,
                fulfillmentChain: undefined,
                paymentSource: undefined,
                costBasis: undefined,
                factType: undefined,
                supplierStatus: undefined,
                dataSource: undefined,
                metric: undefined,
            },
            true,
        )
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [applied.occurredFrom, applied.occurredTo, url.replaceParams])

    // `/` 聚焦搜索；输入框、文本域、弹层打开时不抢焦点。
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
    }, [searchInputRef])

    return {
        ...url,
        applied,
        searchDraft,
        setSearchDraft,
        filterDraft,
        setFilterDraft,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
    }
}
