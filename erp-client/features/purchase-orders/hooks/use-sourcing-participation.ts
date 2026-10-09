"use client"

import { useCallback, useRef } from "react"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"

/** 暂不分配只关闭提交选择；恢复时沿用原拆分选择和所有已填字段。 */
export function useSourcingParticipation(
    form: PurchaseOrderCreateFormApi,
    scopeKey: string,
) {
    const saved = useRef({
        scopeKey,
        selections: new Map<string, Set<string>>(),
    })
    return useCallback(
        (ids: readonly string[], included: boolean) => {
            if (saved.current.scopeKey !== scopeKey) {
                saved.current = { scopeKey, selections: new Map() }
            }
            const lines = form.state.values.lines
            for (const id of ids) {
                const allocations = lines.flatMap((line, index) =>
                    line.salesOrderLineId === id ? [{ line, index }] : [],
                )
                const active = allocations.filter(({ line }) => line.selected)
                if (included && active.length) continue
                if (!included) {
                    if (!active.length) continue
                    saved.current.selections.set(
                        id,
                        new Set(active.map(({ line }) => line.rowKey)),
                    )
                }
                const previous = saved.current.selections.get(id)
                const canRestore = allocations.some(({ line }) =>
                    previous?.has(line.rowKey),
                )
                for (const { line, index } of allocations) {
                    form.setFieldValue(
                        `lines[${index}].selected`,
                        included && (!canRestore || previous!.has(line.rowKey)),
                    )
                }
                if (included) saved.current.selections.delete(id)
            }
        },
        [form, scopeKey],
    )
}
