"use client"

import * as React from "react"

import {
    autoFillLineResult,
    buildFactIndex,
    deriveOverall,
    emptyLineResult,
    type AcceptanceFactSelection,
    type LineResultState,
} from "@/features/sales-orders/lib/acceptance-model"
import type {
    AcceptanceDraftLine,
    AcceptanceEligibleFact,
    AcceptanceSalesLineGroup,
} from "@/features/sales-orders/lib/acceptance-types"

/**
 * 验收工作台的来源选择与行结果状态机。
 * 状态转换逻辑与拆分前完全一致（含嵌套 updater 的联动更新）。
 */
export type AcceptanceSelectionApi = ReturnType<typeof useAcceptanceSelection>

export function useAcceptanceSelection() {
    const [selected, setSelected] = React.useState<AcceptanceFactSelection>(
        () => new Map(),
    )
    const [lineResults, setLineResults] = React.useState<
        Map<string, LineResultState>
    >(() => new Map())

    const toggleFact = React.useCallback(
        (fact: AcceptanceEligibleFact, checked: boolean) => {
            setSelected((prev) => {
                const next = new Map(prev)
                if (checked) {
                    next.set(fact.fulfillmentLineId, {
                        fact,
                        qty: fact.eligibleQuantity,
                    })
                } else {
                    next.delete(fact.fulfillmentLineId)
                }
                setLineResults((prevResults) => {
                    const results = new Map(prevResults)
                    results.set(
                        fact.salesOrderLineId,
                        autoFillLineResult(
                            fact.salesOrderLineId,
                            next,
                            results.get(fact.salesOrderLineId),
                        ),
                    )
                    // 清理已无分配的行
                    for (const lineId of results.keys()) {
                        let has = false
                        for (const entry of next.values()) {
                            if (entry.fact.salesOrderLineId === lineId)
                                has = true
                        }
                        if (!has) results.delete(lineId)
                    }
                    return results
                })
                return next
            })
        },
        [],
    )

    const setAllocQty = React.useCallback(
        (fulfillmentLineId: string, qty: string) => {
            setSelected((prev) => {
                const entry = prev.get(fulfillmentLineId)
                if (!entry) return prev
                const next = new Map(prev)
                next.set(fulfillmentLineId, { ...entry, qty })
                setLineResults((prevResults) => {
                    const results = new Map(prevResults)
                    results.set(
                        entry.fact.salesOrderLineId,
                        autoFillLineResult(
                            entry.fact.salesOrderLineId,
                            next,
                            results.get(entry.fact.salesOrderLineId),
                        ),
                    )
                    return results
                })
                return next
            })
        },
        [],
    )

    const updateLineResult = React.useCallback(
        (salesOrderLineId: string, patch: Partial<LineResultState>) => {
            setLineResults((prev) => {
                const next = new Map(prev)
                const current = next.get(salesOrderLineId) ?? emptyLineResult()
                next.set(salesOrderLineId, {
                    ...current,
                    ...patch,
                    acceptedManual:
                        patch.acceptedQuantity !== undefined
                            ? true
                            : current.acceptedManual,
                })
                return next
            })
        },
        [],
    )

    /** 从草稿重建来源与行结果（刷新后恢复 session-state 内容）。 */
    const restoreDraft = React.useCallback(
        (
            draftLines: AcceptanceDraftLine[],
            salesLines: AcceptanceSalesLineGroup[],
        ) => {
            const nextSelected: AcceptanceFactSelection = new Map()
            const nextResults = new Map<string, LineResultState>()
            const factIndex = buildFactIndex(salesLines)
            // 草稿中的来源可能因 remainingOnly 被隐藏——仍尝试从历史全量恢复需要重取
            for (const line of draftLines) {
                nextResults.set(line.salesOrderLineId, {
                    acceptedQuantity: line.acceptedQuantity,
                    shortQuantity: line.shortQuantity,
                    rejectedQuantity: line.rejectedQuantity,
                    reason: line.reason,
                    serviceFail: line.serviceFail ?? false,
                    acceptedManual: true,
                })
                for (const alloc of line.allocations) {
                    const fact = factIndex.get(alloc.fulfillmentLineId)
                    if (fact) {
                        nextSelected.set(alloc.fulfillmentLineId, {
                            fact,
                            qty: alloc.allocatedQuantity,
                        })
                    }
                }
            }
            setSelected(nextSelected)
            setLineResults(nextResults)
        },
        [],
    )

    const reset = React.useCallback(() => {
        setSelected(new Map())
        setLineResults(new Map())
    }, [])

    const selectedLines = React.useMemo(
        () =>
            [...selected.values()].reduce<Map<string, AcceptanceEligibleFact[]>>(
                (map, entry) => {
                    const list = map.get(entry.fact.salesOrderLineId) ?? []
                    list.push(entry.fact)
                    map.set(entry.fact.salesOrderLineId, list)
                    return map
                },
                new Map(),
            ),
        [selected],
    )

    const overallPreview = React.useMemo(
        () => deriveOverall([...lineResults.values()]),
        [lineResults],
    )

    const hasExceptionResult =
        overallPreview === "SHORT" ||
        overallPreview === "REJECT" ||
        overallPreview === "SERVICE_FAIL"

    return {
        selected,
        lineResults,
        selectedLines,
        overallPreview,
        hasExceptionResult,
        toggleFact,
        setAllocQty,
        updateLineResult,
        restoreDraft,
        reset,
    }
}
