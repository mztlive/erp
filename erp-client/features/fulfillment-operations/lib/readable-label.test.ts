import { describe, expect, it } from "vitest"

import {
    displayText,
    formatRemainingLines,
    isOpaqueId,
    lineItemTitle,
} from "./readable-label"

describe("readable fulfillment labels", () => {
    it("treats hex ids and prefixed uuid numbers as opaque", () => {
        expect(isOpaqueId("10cbccde28ac43639335656a02fd62f4")).toBe(true)
        expect(isOpaqueId("DLV-4024b6046fd64028984c1f25d52a81c4")).toBe(true)
        expect(isOpaqueId("PO-d7fed4a4e94843a6867d366c8597e03a")).toBe(true)
        expect(isOpaqueId("SO20260826-000001")).toBe(false)
        expect(isOpaqueId("XS20260826152351")).toBe(false)
        expect(isOpaqueId("演示客户")).toBe(false)
    })

    it("drops empty and opaque display values", () => {
        expect(displayText("10cbccde28ac43639335656a02fd62f4")).toBe("")
        expect(displayText("—")).toBe("")
        expect(displayText(" 华东纸业 ")).toBe("华东纸业")
    })

    it("only prints remaining quantities when the item name is readable", () => {
        expect(
            formatRemainingLines([
                {
                    itemName: "",
                    remainingQuantity: "1",
                    unitCode: "",
                },
                {
                    itemName: "礼盒",
                    remainingQuantity: "2",
                    unitCode: "件",
                },
            ]),
        ).toBe("礼盒 2件")
    })

    it("falls back to 明细 n instead of a row id", () => {
        expect(lineItemTitle("", 0)).toBe("明细 1")
        expect(lineItemTitle("礼盒", 0)).toBe("礼盒")
    })
})
