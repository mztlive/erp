"use client"

import * as React from "react"

export type CommandIdentity = {
    key: string
    idempotencyKey: string
    operationId: string
}

/**
 * 页面会话内命令身份生成器：同一 (kind, objectId) 复用同一对
 * 幂等键 / 操作号，成功回调后清除，下次重试生成新身份。
 */
export function useCommandIdentities() {
    const identities = React.useRef(
        new Map<string, { idempotencyKey: string; operationId: string }>(),
    )

    const commandIdentity = React.useCallback(
        (kind: string, objectId: string): CommandIdentity => {
            const key = `${kind}:${objectId}`
            const existing = identities.current.get(key)
            if (existing) return { key, ...existing }
            const identity = {
                idempotencyKey: `w17:${kind}:${objectId}:${crypto.randomUUID()}`,
                operationId: `w17:${kind}:${crypto.randomUUID()}`,
            }
            identities.current.set(key, identity)
            return { key, ...identity }
        },
        [],
    )

    const clearIdentity = React.useCallback((key: string) => {
        identities.current.delete(key)
    }, [])

    return { commandIdentity, clearIdentity }
}
