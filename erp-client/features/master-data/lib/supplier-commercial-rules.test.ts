import { describe, expect, it } from "vitest"
import {
    parseSupplierTaxRates,
    supplierTaxPercentages,
} from "@/lib/supplier-tax-rates"
import {
    parsePeriodicTerm,
    periodicPaymentTerm,
    PERIODIC_SETTLEMENTS,
} from "@/lib/supplier-payment-terms"
import { hydrateSupplierEditor } from "./supplier-editor-model"
import type { MasterDataCenterView } from "../types"
import {
    paymentTermCode,
    paymentTermLabel,
    paymentTermMatchesSettlement,
} from "@/lib/business-options"

describe("供应商商务规则", () => {
    it("五种周期的展示事实重新打开编辑器后保留自定义天数", () => {
        for (const period of PERIODIC_SETTLEMENTS) {
            for (const days of [0, 27, 366]) {
                const code = `PERIOD_${period.period}_${days}`
                const label = paymentTermLabel(code)
                expect(paymentTermCode(label)).toBe(code)
                const center: MasterDataCenterView = {
                    resource: "suppliers",
                    name: "示例供应商",
                    stableId: "supplier-1",
                    stableNo: "SUP-1",
                    lockVersion: 1,
                    lifecycleStatus: "ENABLED",
                    lifecycleStatusLabel: "启用",
                    lifecycleTone: "success",
                    revisionTiming: "CURRENT",
                    revisionTimingLabel: "当前",
                    revisionTimeline: [],
                    selectorEligibility: [],
                    sensitiveFields: [],
                    resourceFacts: [],
                    usageSummary: { historicalReferenceCount: 0, note: "" },
                    allowedActions: [],
                    actionBlockers: [],
                    auditEvents: [],
                    sections: [],
                    currentRevision: {
                        revisionId: "revision-1",
                        revisionNo: 1,
                        name: "示例供应商",
                        effectiveFrom: "2026-09-10",
                        changeReason: "新建",
                        actor: "管理员",
                        fields: [
                            { label: "结算方式", value: period.label },
                            { label: "付款条件", value: label },
                        ],
                    },
                }
                const hydrated = hydrateSupplierEditor(center)
                expect(hydrated.paymentTerm).toBe(code)
                expect(hydrated.settlement).toBe(period.value)
                expect(
                    paymentTermMatchesSettlement(
                        hydrated.paymentTerm,
                        hydrated.settlement,
                    ),
                ).toBe(true)
            }
        }
        expect(paymentTermCode("月结，期末后 367 天付款")).toBeUndefined()
    })
    it("空税率、零税率和历史单税率分别保留", () => {
        expect(parseSupplierTaxRates("")).toEqual([])
        expect(parseSupplierTaxRates("0%")).toEqual(["0"])
        expect(parseSupplierTaxRates("13%、9%,13")).toEqual(["0.09", "0.13"])
        expect(supplierTaxPercentages(undefined, "0.13")).toBe("13")
        expect(supplierTaxPercentages([], "0.13")).toBe("")
        expect(() => parseSupplierTaxRates("abc")).toThrow()
        expect(() => parseSupplierTaxRates("100")).toThrow()
    })
    it("周期与自定义付款天数往返一致，并保留旧货到规则", () => {
        expect(periodicPaymentTerm("monthly", "0")).toBe("PERIOD_MONTH_0")
        expect(parsePeriodicTerm("PERIOD_HALF_YEAR_15")?.value).toBe(
            "half_yearly",
        )
        expect(paymentTermMatchesSettlement("PERIOD_MONTH_27", "monthly")).toBe(
            true,
        )
        expect(paymentTermMatchesSettlement("PERIOD_MONTH_27", "weekly")).toBe(
            false,
        )
        expect(paymentTermLabel("PERIOD_MONTH_27")).toBe(
            "月结，期末后 27 天付款",
        )
        expect(paymentTermCode("NET-30")).toBe("POSTPAY_NET30")
        expect(parsePeriodicTerm("PERIOD_MONTH_")).toBeUndefined()
        expect(parsePeriodicTerm("PERIOD_MONTH_367")).toBeUndefined()
    })
})
