"use client"

import { useEffect, useRef, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import type { RowSelectionState } from "@tanstack/react-table"
import { getErrorMessage } from "@/lib/api"
import type { CandidateFilters } from "../lib/candidate-list-query"
import { paymentRequestKeys, usePaymentCandidateIdsQuery } from "./queries"

export function useCandidateSelection(
    filters: CandidateFilters,
    onApply: (ids: string[]) => void,
) {
    const [selection, setSelection] = useState<RowSelectionState>({})
    const [busy, setBusy] = useState(false)
    const [error, setError] = useState<string | null>(null)
    const matching = usePaymentCandidateIdsQuery(filters)
    const eligible = usePaymentCandidateIdsQuery({})
    const queryClient = useQueryClient()
    const operation = useRef(0)
    const inFlight = useRef(false)
    // 切换分类或离开页面后，迟到的响应不得勾选记录或打开申请弹窗。
    useEffect(
        () => () => {
            operation.current += 1
        },
        [],
    )
    const filterKey = JSON.stringify(filters)
    useEffect(() => {
        operation.current += 1
        inFlight.current = false
        setBusy(false)
        setError(null)
    }, [filterKey])
    const ids = Object.keys(selection).filter((id) => selection[id])
    const select = async (action: "all" | "apply") => {
        if (inFlight.current) return
        inFlight.current = true
        const version = ++operation.current
        setBusy(true)
        setError(null)
        try {
            const result = await (
                action === "all" ? matching : eligible
            ).refetch()
            if (version !== operation.current) return
            if (result.isError) throw result.error
            const available = result.data ?? []
            if (action === "all") {
                setSelection((current) => ({
                    ...current,
                    ...Object.fromEntries(available.map((id) => [id, true])),
                }))
                return
            }
            const valid = new Set(available)
            const selected = ids.filter((id) => valid.has(id))
            if (selected.length !== ids.length) {
                setSelection(
                    Object.fromEntries(selected.map((id) => [id, true])),
                )
                setError(
                    `${ids.length - selected.length} 张采购单已不可申请，已取消勾选。请核对剩余采购单后重新发起申请。`,
                )
                return
            }
            if (!selected.length) return
            await queryClient.invalidateQueries({
                queryKey: paymentRequestKeys.candidates,
                exact: true,
            })
            if (version === operation.current) onApply(selected)
        } catch (cause) {
            if (version === operation.current)
                setError(getErrorMessage(cause, "采购单核对失败，请重试"))
        } finally {
            if (version === operation.current) {
                inFlight.current = false
                setBusy(false)
            }
        }
    }
    const cancelPending = () => {
        operation.current += 1
        inFlight.current = false
        setBusy(false)
        setError(null)
    }
    return { selection, setSelection, ids, busy, error, select, cancelPending }
}
