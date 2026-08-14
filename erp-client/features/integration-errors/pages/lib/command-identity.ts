import { newKey } from "./helpers"

export type CommandIdentity = {
    idempotencyKey: string
    operationId: string
}

export type CommandIdentityStore = {
    get: (kind: string, objectId: string) => { key: string } & CommandIdentity
    delete: (key: string) => void
}

export function createCommandIdentityStore(): CommandIdentityStore {
    const identities = new Map<
        string,
        { idempotencyKey: string; operationId: string }
    >()
    return {
        get(kind: string, objectId: string) {
            const key = `${kind}:${objectId}`
            const existing = identities.get(key)
            if (existing) return { key, ...existing }
            const identity = {
                idempotencyKey: newKey(`w29:${kind}:${objectId}`),
                operationId: newKey(`w29:${kind}`),
            }
            identities.set(key, identity)
            return { key, ...identity }
        },
        delete(key: string) {
            identities.delete(key)
        },
    }
}
