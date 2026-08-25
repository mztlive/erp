import { describe, expect, it } from "vitest"

import {
    applyDueDateToLines,
    createEmptyLine,
} from "@/features/sales-orders/lib/sales-order-create-model"

describe("applyDueDateToLines", () => {
    it("writes the same due date onto every line", () => {
        const first = {
            ...createEmptyLine("physical_service"),
            dueDate: "2026-01-01",
        }
        const second = createEmptyLine("physical_service")
        const next = applyDueDateToLines([first, second], "2026-08-25")
        expect(next).toHaveLength(2)
        expect(next[0]?.dueDate).toBe("2026-08-25")
        expect(next[1]?.dueDate).toBe("2026-08-25")
        expect(next[0]?.rowKey).toBe(first.rowKey)
        expect(next[1]?.rowKey).toBe(second.rowKey)
        expect(first.dueDate).toBe("2026-01-01")
        expect(second.dueDate).toBe("")
    })

    it("keeps the original object when the date is already the same", () => {
        const line = {
            ...createEmptyLine("physical_service"),
            dueDate: "2026-08-25",
        }
        const next = applyDueDateToLines([line], "2026-08-25")
        expect(next[0]).toBe(line)
    })

    it("does not change lines when the date is empty", () => {
        const line = {
            ...createEmptyLine("physical_service"),
            dueDate: "2026-01-01",
        }
        const next = applyDueDateToLines([line], "  ")
        expect(next).toEqual([line])
        expect(next[0]?.dueDate).toBe("2026-01-01")
    })
})
