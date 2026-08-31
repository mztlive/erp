import { describe, expect, it } from "vitest"

import { toAutomationIdSegment } from "@/lib/automation-id"

describe("toAutomationIdSegment", () => {
    it.each([
        [" Sales_Order / ABC 001 ", "sales-order-abc-001"],
        ["--Foo__bar..", "foo-bar"],
        ["Crème brûlée", "creme-brulee"],
        [42, "42"],
        [true, "true"],
        [{ toString: () => "SKU#A/B" }, "sku-a-b"],
    ])("把 %j 清洗为 kebab-safe 片段", (value, expected) => {
        expect(toAutomationIdSegment(value)).toBe(expected)
    })

    it.each([null, undefined, "", "___", "中文", Object.create(null)])(
        "空清洗结果回退为 item",
        (value) => {
            expect(toAutomationIdSegment(value)).toBe("item")
        },
    )
})
