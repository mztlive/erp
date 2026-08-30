"use client"

import * as React from "react"

import {
    applyResultChange,
    defaultBatchDraft,
    deriveOverall,
    formatQty,
    hasFilledException,
    parseQty,
    type AcceptanceBatchSelection,
} from "@/features/sales-orders/lib/acceptance-model"
import type {
    AcceptanceEligibleFact,
    AcceptanceOverallResult,
} from "@/features/sales-orders/lib/acceptance-types"

export type AcceptanceSelectionApi = ReturnType<typeof useAcceptanceSelection>

export function useAcceptanceSelection() {
    const [selected, setSelected] = React.useState<AcceptanceBatchSelection>(
        () => new Map(),
    )

    const skipFact = React.useCallback((fulfillmentLineId: string) => {
        setSelected((prev) => {
            const next = new Map(prev)
            next.delete(fulfillmentLineId)
            return next
        })
    }, [])

    const selectResult = React.useCallback(
        (fact: AcceptanceEligibleFact, result: AcceptanceOverallResult) => {
            setSelected((prev) => {
                const next = new Map(prev)
                const current =
                    next.get(fact.fulfillmentLineId) ?? defaultBatchDraft(fact)
                next.set(
                    fact.fulfillmentLineId,
                    applyResultChange(current, result),
                )
                return next
            })
        },
        [],
    )

    const updateDraft = React.useCallback(
        (
            fulfillmentLineId: string,
            patch: Partial<{
                qty: string
                exceptionQty: string
                reason: string
            }>,
        ) => {
            setSelected((prev) => {
                const current = prev.get(fulfillmentLineId)
                if (!current) return prev
                const updated = { ...current, ...patch }
                if (
                    patch.qty !== undefined &&
                    updated.result !== "PASS" &&
                    parseQty(updated.exceptionQty) > parseQty(updated.qty)
                ) {
                    updated.exceptionQty = formatQty(updated.qty)
                }
                const next = new Map(prev)
                next.set(fulfillmentLineId, updated)
                return next
            })
        },
        [],
    )

    const replace = React.useCallback((next: AcceptanceBatchSelection) => {
        setSelected(next)
    }, [])

    const reset = React.useCallback(() => {
        setSelected(new Map())
    }, [])

    const overallPreview = React.useMemo(
        () => deriveOverall(selected.values()),
        [selected],
    )

    const hasExceptionResult = React.useMemo(() => {
        for (const draft of selected.values()) {
            if (hasFilledException(draft)) return true
        }
        return false
    }, [selected])

    return {
        selected,
        overallPreview,
        hasExceptionResult,
        selectResult,
        skipFact,
        updateDraft,
        replace,
        reset,
    }
}
