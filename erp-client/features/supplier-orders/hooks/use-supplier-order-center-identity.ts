"use client"

import * as React from "react"

export type CommandIdentity = {
    key: string
    operationId: string
    idempotencyKey: string
}

/**
 * 同一「类别 + 对象」的命令在页面生命周期内复用同一任务号；
 * 命令落定后调用方清除，避免误用旧键重复提交。
 */
export function useSupplierOrderCenterCommandIdentity() {
    const identities = React.useRef(
        new Map<string, { operationId: string; idempotencyKey: string }>(),
    )

    const commandIdentity = (
        kind: string,
        objectId: string,
    ): CommandIdentity => {
        const key = `${kind}:${objectId}`
        const existing = identities.current.get(key)
        if (existing) return { key, ...existing }
        const identity = {
            operationId: `w26:${kind}:${crypto.randomUUID()}`,
            idempotencyKey: `w26:${kind}:${crypto.randomUUID()}`,
        }
        identities.current.set(key, identity)
        return { key, ...identity }
    }

    const forgetCommandIdentity = (key: string) => {
        identities.current.delete(key)
    }

    return { commandIdentity, forgetCommandIdentity }
}
