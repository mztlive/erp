"use client"

import { useRef, useState } from "react"
import type { UseMutationResult } from "@tanstack/react-query"

import { getErrorMessage, isApiError } from "@/lib/api/errors"
import {
    FormalCommandKeyLedger,
    isCommandOutcomeUnknown,
} from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"

import type { FormalActionResponse } from "../types"

type Permissions = Parameters<typeof hasPermission>[0]

type Recovery = {
    label: string
    permissions: readonly string[]
    message: string
    retry: () => Promise<void>
}

/** 保留原请求的版本、操作号与提交键，直至收到可确认的命令结果。 */
export function useSupplierInvestigationCommands({
    permissions,
    refresh,
}: {
    permissions: Permissions
    refresh: () => Promise<unknown>
}) {
    const [ledger] = useState(() => new FormalCommandKeyLedger((key) => key))
    const inFlight = useRef(false)
    const unresolved = useRef<string | null>(null)
    const [pending, setPending] = useState(false)
    const [recovery, setRecovery] = useState<Recovery | null>(null)
    const [conflict, setConflict] = useState<string | null>(null)

    function wrap<T, Input extends { idempotencyKey: string }>(
        mutation: UseMutationResult<FormalActionResponse<T>, Error, Input>,
        options: {
            kind: string
            label: string
            permissions:
                | readonly string[]
                | ((input: Input) => readonly string[])
            onSettled?: () => void
            onRecovered?: (response: FormalActionResponse<T>) => void
        },
    ): typeof mutation {
        const { kind, label, onSettled, onRecovered } = options
        const requiredFor = (input: Input) =>
            typeof options.permissions === "function"
                ? options.permissions(input)
                : options.permissions

        async function execute(
            input: Input,
            recovering = false,
        ): Promise<FormalActionResponse<T>> {
            const slot = `${kind}:${input.idempotencyKey}`
            if (
                inFlight.current ||
                (recovering
                    ? unresolved.current !== slot
                    : unresolved.current !== null)
            ) {
                return { status: "blocked", message: "请先核对当前操作结果。" }
            }

            const identity = ledger.acquire(slot, input.idempotencyKey, input)
            inFlight.current = true
            setPending(true)
            setConflict(null)

            function retain(message: string) {
                unresolved.current = slot
                ledger.settle(slot, "unknown")
                setRecovery({
                    label,
                    permissions: requiredFor(input),
                    message,
                    retry: async () => {
                        let response: FormalActionResponse<T>
                        try {
                            response = await execute(identity.payload, true)
                        } catch (error) {
                            if (isCommandOutcomeUnknown(error)) return
                            response = {
                                status: "failed",
                                message: getErrorMessage(
                                    error,
                                    "核对失败，请刷新后重试。",
                                ),
                            }
                        }
                        if (response.status !== "unknown" || response.data) {
                            await refresh()
                            onRecovered?.(response)
                        }
                    },
                })
            }

            function settle(outcome: "succeeded" | "failed") {
                unresolved.current = null
                ledger.settle(slot, outcome)
                setRecovery(null)
                onSettled?.()
            }

            try {
                const response = await mutation.mutateAsync(identity.payload)
                // 调查返回可追溯的证据时，本次请求已经落定，供应商业务结果仍可未知。
                if (response.status === "unknown" && !response.data) {
                    retain(response.message)
                } else {
                    settle(
                        response.status === "succeeded"
                            ? "succeeded"
                            : "failed",
                    )
                }
                return response
            } catch (error) {
                if (isCommandOutcomeUnknown(error)) {
                    retain(getErrorMessage(error, "暂未收到处理结果。"))
                } else {
                    settle("failed")
                    if (
                        isApiError(error) &&
                        (error.status === 409 || error.status === 412)
                    ) {
                        setConflict(
                            getErrorMessage(
                                error,
                                "订单或任务已更新，请刷新最新数据。",
                            ),
                        )
                    }
                }
                throw error
            } finally {
                inFlight.current = false
                setPending(false)
            }
        }

        return {
            ...mutation,
            mutateAsync: (input) =>
                requiredFor(input).some((permission) =>
                    hasPermission(permissions, permission),
                )
                    ? execute(input)
                    : Promise.resolve({
                          status: "blocked",
                          message: "当前账号没有此操作权限。",
                      }),
        }
    }

    return {
        wrap,
        pending,
        recovery,
        canRecover:
            recovery?.permissions.some((permission) =>
                hasPermission(permissions, permission),
            ) ?? false,
        conflict,
        clearConflict: () => setConflict(null),
    }
}
