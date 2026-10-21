"use client"

import * as React from "react"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "@/lib/inquiry-local"
import type { CreatePurchaseOrdersFromSourcingInput } from "../types"
import type { PurchaseOrderCreateFormValues } from "../lib/purchase-order-create-form-types"
import type { createPurchaseOrdersFromSourcing } from "../api/purchase-order-commands"

export type SavedSourcingCommand = {
    hasUnknown?: boolean
    input: CreatePurchaseOrdersFromSourcingInput
    salesOrderNo: string
    formValues: PurchaseOrderCreateFormValues
}
type SourcingResult = Awaited<
    ReturnType<typeof createPurchaseOrdersFromSourcing>
>

export function sourcingDraftKey(
    userId: string,
    salesOrderId: string,
    workItemId: string,
): string {
    return inquiryStorageKey(
        userId,
        `sourcing:${salesOrderId}:${workItemId}:draft`,
    )
}

/** 原输入先落本地再发送；未知结果按账号、销售和任务恢复，不依赖剩余创建依据。 */
export function useSourcingCommand(
    userId: string,
    submit: (
        input: CreatePurchaseOrdersFromSourcingInput,
    ) => Promise<SourcingResult>,
) {
    const [pending, setPending] = React.useState<SavedSourcingCommand | null>(
        null,
    )
    const [ready, setReady] = React.useState(false)
    const pendingRef = React.useRef<SavedSourcingCommand | null>(null)
    const indexKey = inquiryStorageKey(userId, "sourcing:pending-index")
    const commandKey = (input: CreatePurchaseOrdersFromSourcingInput) =>
        inquiryStorageKey(
            userId,
            `sourcing:${input.salesOrderId}:${input.workItemId}:command`,
        )
    React.useEffect(() => {
        if (!userId) return
        const key = readInquiryLocalValue<string>(indexKey)
        const restored = key
            ? readInquiryLocalValue<SavedSourcingCommand>(key)
            : undefined
        const uncertain = restored ? { ...restored, hasUnknown: true } : null
        pendingRef.current = uncertain
        setPending(uncertain)
        setReady(true)
    }, [indexKey, userId])
    return {
        pending,
        ready,
        async run(prepared?: SavedSourcingCommand) {
            if (!userId || !ready)
                throw new Error("正在恢复上次供给分配，请稍候。")
            const savedKey = readInquiryLocalValue<string>(indexKey)
            const stored = savedKey
                ? readInquiryLocalValue<SavedSourcingCommand>(savedKey)
                : undefined
            const original =
                pendingRef.current ??
                (stored ? { ...stored, hasUnknown: true } : prepared)
            if (!original) throw new Error("没有需要恢复的供给分配。")
            const key = commandKey(original.input)
            writeInquiryLocalValue(key, original)
            writeInquiryLocalValue(indexKey, key)
            pendingRef.current = original
            setPending(original)
            let result: SourcingResult
            try {
                result = await submit(original.input)
            } catch (error) {
                const uncertain = { ...original, hasUnknown: true }
                pendingRef.current = uncertain
                setPending(uncertain)
                try {
                    writeInquiryLocalValue(key, uncertain)
                } catch {
                    /* 已持有的原命令仍可恢复。 */
                }
                throw error
            }
            if (
                result.status === "succeeded" ||
                (result.status === "failed" && !original.hasUnknown)
            ) {
                try {
                    localStorage.removeItem(key)
                    localStorage.removeItem(indexKey)
                } catch {
                    /* 清理失败不改变已确认业务结果，刷新后仍可安全回放原命令。 */
                }
                pendingRef.current = null
                setPending(null)
                if (result.status === "succeeded")
                    try {
                        localStorage.removeItem(
                            sourcingDraftKey(
                                userId,
                                original.input.salesOrderId,
                                original.input.workItemId,
                            ),
                        )
                    } catch {
                        /* 本地输入清理失败不改变业务成功。 */
                    }
            } else {
                const uncertain = { ...original, hasUnknown: true }
                pendingRef.current = uncertain
                setPending(uncertain)
                try {
                    writeInquiryLocalValue(key, uncertain)
                } catch {
                    /* 首次发送前的原记录仍可恢复。 */
                }
            }
            return { original, result }
        },
    }
}
