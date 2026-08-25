import { describe, expect, it } from "vitest"

import {
    displayActorName,
    displayUnixSeconds,
    executionStatusTone,
    instanceStatusTone,
    isOpenInstanceStatus,
} from "./display"

describe("isOpenInstanceStatus", () => {
    it("treats only RUNNING and BLOCKED as in-flight", () => {
        expect(isOpenInstanceStatus("RUNNING")).toBe(true)
        expect(isOpenInstanceStatus("BLOCKED")).toBe(true)
        expect(isOpenInstanceStatus("APPROVED")).toBe(false)
        expect(isOpenInstanceStatus("CANCELLED")).toBe(false)
        expect(isOpenInstanceStatus("UNKNOWN")).toBe(false)
        expect(isOpenInstanceStatus(undefined)).toBe(false)
        expect(isOpenInstanceStatus(null)).toBe(false)
    })
})

describe("instanceStatusTone", () => {
    it("maps known instance statuses to badge tones", () => {
        expect(instanceStatusTone("RUNNING")).toBe("warning")
        expect(instanceStatusTone("APPROVED")).toBe("success")
        expect(instanceStatusTone("CANCELLED")).toBe("void")
        expect(instanceStatusTone("BLOCKED")).toBe("destructive")
        expect(instanceStatusTone("UNKNOWN")).toBe("warning")
    })
})

describe("executionStatusTone", () => {
    it("maps known execution statuses to badge tones", () => {
        expect(executionStatusTone("ACTIVE")).toBe("info")
        expect(executionStatusTone("APPROVED")).toBe("success")
        expect(executionStatusTone("REJECTED")).toBe("destructive")
        expect(executionStatusTone("BLOCKED")).toBe("destructive")
        expect(executionStatusTone("CANCELLED")).toBe("void")
        expect(executionStatusTone("SUPERSEDED")).toBe("neutral")
        expect(executionStatusTone("UNKNOWN")).toBe("info")
    })
})

describe("displayActorName", () => {
    it("keeps human names and hides opaque ids", () => {
        expect(displayActorName("李思勇")).toBe("李思勇")
        expect(displayActorName("e9ca600460404aa48a1ff7b333933e3a")).toBe(
            undefined,
        )
        expect(displayActorName("550e8400-e29b-41d4-a716-446655440000")).toBe(
            undefined,
        )
        expect(displayActorName("  ")).toBe(undefined)
    })
})

describe("displayUnixSeconds", () => {
    it("returns undefined for missing values", () => {
        expect(displayUnixSeconds(undefined)).toBe(undefined)
        expect(displayUnixSeconds(0)).toBe(undefined)
    })

    it("formats a unix timestamp", () => {
        const shown = displayUnixSeconds(1_700_000_000)
        expect(shown?.dateTime).toBe("2023-11-14T22:13:20.000Z")
        expect(shown?.label).toBeTruthy()
    })
})
