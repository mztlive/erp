import { describe, expect, it } from "vitest"

import { isOpaqueId, splitDetailSections } from "./detail-facts"

const salesSections = [
    { label: "客户", value: "E2E客户56920203" },
    { label: "业务性质", value: "实物及服务" },
    { label: "结算主体", value: "E2E客户56920203" },
    { label: "合同", value: "HT-7456920203" },
    { label: "含税金额", value: "¥100", numeric: true },
    { label: "不含税金额", value: "¥87", numeric: true },
    { label: "税额", value: "¥13", numeric: true },
    { label: "付款条件", value: "货到 15 天" },
    { label: "项目", value: "年节礼包" },
    { label: "提交人", value: "7e9e521afce041b79218edb9a246e974" },
]

describe("splitDetailSections", () => {
    it("puts amounts on the money row with the gross amount first", () => {
        const facts = splitDetailSections(salesSections, "E2E客户56920203")

        expect(facts.amounts.map((section) => section.label)).toEqual([
            "含税金额",
            "不含税金额",
            "税额",
        ])
    })

    it("keeps decision fields open and demotes the rest", () => {
        const facts = splitDetailSections(salesSections, "E2E客户56920203")

        expect(facts.keyFields.map((section) => section.label)).toEqual([
            "业务性质",
            "付款条件",
        ])
        expect(facts.moreFields.map((section) => section.label)).toEqual([
            "结算主体",
            "合同",
            "项目",
        ])
    })

    it("drops the counterparty section already shown in the header", () => {
        const facts = splitDetailSections(salesSections, "E2E客户56920203")

        expect(
            [...facts.keyFields, ...facts.moreFields].some(
                (section) => section.label === "客户",
            ),
        ).toBe(false)
    })

    it("keeps the counterparty section when the header shows something else", () => {
        const facts = splitDetailSections(salesSections, "另一个客户")

        expect(facts.moreFields[0]?.label).toBe("客户")
    })

    it("lifts a resolved submitter name out of the field grid", () => {
        const facts = splitDetailSections(
            [{ label: "提交人", value: "采购1" }],
            undefined,
        )

        expect(facts.submitter).toBe("采购1")
        expect(facts.moreFields).toHaveLength(0)
    })

    it("drops an unresolved submitter id instead of putting an id on screen", () => {
        const facts = splitDetailSections(salesSections, "E2E客户56920203")

        expect(facts.submitter).toBeUndefined()
        expect(
            [...facts.keyFields, ...facts.moreFields].some(
                (section) => section.label === "提交人",
            ),
        ).toBe(false)
    })

    it("drops empty values", () => {
        const facts = splitDetailSections(
            [
                { label: "合同", value: "   " },
                { label: "项目", value: "年节礼包" },
            ],
            undefined,
        )

        expect(facts.moreFields.map((section) => section.label)).toEqual([
            "项目",
        ])
    })
})

describe("isOpaqueId", () => {
    it("detects hex ids and uuids", () => {
        expect(isOpaqueId("7e9e521afce041b79218edb9a246e974")).toBe(true)
        expect(isOpaqueId("7e9e521a-fce0-41b7-9218-edb9a246e974")).toBe(true)
    })

    it("accepts human names and document numbers", () => {
        expect(isOpaqueId("采购1")).toBe(false)
        expect(isOpaqueId("HT-7456920203")).toBe(false)
        expect(isOpaqueId("Zhou Hang")).toBe(false)
    })
})
