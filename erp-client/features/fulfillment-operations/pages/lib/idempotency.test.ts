import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"

import { createIdempotencyKey } from "./idempotency"

let uuidSequence: string[]

beforeEach(() => {
    uuidSequence = ["uuid-1", "uuid-2"]
    vi.stubGlobal("crypto", {
        randomUUID: vi.fn(() => uuidSequence.shift()),
    })
})

afterEach(() => {
    vi.unstubAllGlobals()
})

describe("createIdempotencyKey", () => {
    it("carries the operation, version and action", () => {
        expect(createIdempotencyKey("op_1", 3, "save")).toBe(
            "w09:op_1:3:save:uuid-1",
        )
        expect(createIdempotencyKey("op_1", 3, "post")).toBe(
            "w09:op_1:3:post:uuid-2",
        )
    })

    it("includes the generated request identifier in every key", () => {
        uuidSequence = ["same-seed", "same-seed"]
        const keys = [
            createIdempotencyKey("op_1", 1, "post"),
            createIdempotencyKey("op_1", 1, "post"),
        ]
        expect(keys[0]).toBe("w09:op_1:1:post:same-seed")
        expect(keys[1]).toBe("w09:op_1:1:post:same-seed")
        expect(keys[0]).toBe(keys[1])
    })
})
