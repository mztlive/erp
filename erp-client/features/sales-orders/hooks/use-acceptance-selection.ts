"use client"

import * as React from "react"

import {
    defaultBatchDraft,
    deriveOverall,
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

    const toggleFact = React.useCallback(
        (fact: AcceptanceEligibleFact, checked: boolean) => {
            setSelected((prev) => {
                const next = new Map(prev)
                if (checked)
                    next.set(fact.fulfillmentLineId, defaultBatchDraft(fact))
                else next.delete(fact.fulfillmentLineId)
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
                result: AcceptanceOverallResult
                exceptionQty: string
                reason: string
            }>,
        ) => {
            setSelected((prev) => {
                const current = prev.get(fulfillmentLineId)
                if (!current) return prev
                const next = new Map(prev)
                const result = patch.result ?? current.result
                next.set(fulfillmentLineId, {
                    ...current,
                    ...patch,
                    exceptionQty:
                        result === "PASS"
                            ? "0"
                            : (patch.exceptionQty ?? current.exceptionQty),
                    reason:
                        result === "PASS"
                            ? ""
                            : (patch.reason ?? current.reason),
                })
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

    const hasExceptionResult =
        overallPreview === "SHORT" ||
        overallPreview === "REJECT" ||
        overallPreview === "SERVICE_FAIL"

    return {
        selected,
        overallPreview,
        hasExceptionResult,
        toggleFact,
        updateDraft,
        replace,
        reset,
    }
}
