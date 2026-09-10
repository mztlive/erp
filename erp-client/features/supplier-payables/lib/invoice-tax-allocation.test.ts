import { describe, expect, it } from "vitest"
import { invoiceTaxAllocations } from "./invoice-tax-allocation"

describe("混合税率发票核销", () => {
    it("两笔采购分别分配实际税额，整票净额和税额守恒", () => {
        const lines = invoiceTaxAllocations(
            [
                { payableAccountId: "a", amount: "109", taxAmount: "9" },
                { payableAccountId: "b", amount: "113", taxAmount: "13" },
            ],
            "22",
        )
        expect(lines.map((line) => line.allocated_net_amount)).toEqual([
            "100.00",
            "100.00",
        ])
        expect(lines.map((line) => line.allocated_tax_amount)).toEqual([
            "9",
            "13",
        ])
    })
    it("单笔核销沿用整票税額", () => {
        expect(
            invoiceTaxAllocations(
                [{ payableAccountId: "a", amount: "113" }],
                "13",
            )[0].allocated_net_amount,
        ).toBe("100.00")
    })
    it("缺少分配税额、超额及合计不一致均拒绝", () => {
        expect(() =>
            invoiceTaxAllocations(
                [
                    { payableAccountId: "a", amount: "100" },
                    { payableAccountId: "b", amount: "100" },
                ],
                "20",
            ),
        ).toThrow("填写")
        expect(() =>
            invoiceTaxAllocations(
                [{ payableAccountId: "a", amount: "10" }],
                "11",
            ),
        ).toThrow("介于")
        expect(() =>
            invoiceTaxAllocations(
                [
                    { payableAccountId: "a", amount: "100", taxAmount: "9" },
                    { payableAccountId: "b", amount: "100", taxAmount: "9" },
                ],
                "20",
            ),
        ).toThrow("合计")
    })
})
