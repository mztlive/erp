import { beforeEach, describe, expect, it, vi } from "vitest"

import { commandIdempotencyKey } from "@/features/import-opening/lib/command-idempotency"

let uuidCounter = 0

beforeEach(() => {
    uuidCounter = 0
    const realCrypto = globalThis.crypto
    const uuidStub = {
        randomUUID: () => {
            uuidCounter += 1
            return `uuid-${uuidCounter}`
        },
    }
    vi.stubGlobal(
        "crypto",
        Object.assign(Object.create(Object.getPrototypeOf(realCrypto)), {
            ...realCrypto,
            ...uuidStub,
        }),
    )
})

describe("commandIdempotencyKey", () => {
    it("reuses the key for the same identity", () => {
        const keys = new Map<string, string>()
        const first = commandIdempotencyKey(keys, "w1:PROCESS:v1")
        const second = commandIdempotencyKey(keys, "w1:PROCESS:v1")
        expect(first).toBe("w18:uuid-1")
        expect(second).toBe(first)
        expect(uuidCounter).toBe(1)
    })

    it("mints distinct keys for distinct identities", () => {
        const keys = new Map<string, string>()
        const first = commandIdempotencyKey(keys, "w1:A")
        const second = commandIdempotencyKey(keys, "w1:B")
        expect(first).toMatch(/^w18:/)
        expect(second).toMatch(/^w18:/)
        expect(first).not.toBe(second)
    })

    it("scopes the key with the w18 prefix", () => {
        const keys = new Map<string, string>()
        expect(commandIdempotencyKey(keys, "any")).toMatch(/^w18:uuid-\d+$/)
    })
})
