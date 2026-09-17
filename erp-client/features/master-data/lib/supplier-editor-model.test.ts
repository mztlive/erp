import { describe, expect, it } from "vitest"

import {
    createSupplierEditorDefaults,
    hydrateSupplierEditor,
    type SupplierEditorFormValues,
    validateSupplierEditorFields,
} from "./supplier-editor-model"
import type { MasterDataCenterView } from "../types"

const validValues = (): SupplierEditorFormValues => ({
    ...createSupplierEditorDefaults(true),
    name: "云桦有礼",
    company: "云桦有礼有限公司",
    signingEntity: "party-signing",
    paymentEntity: "party-payment",
    settlement: "pay_after_use",
    paymentTerm: "POSTPAY_NET30",
    maintainerUserId: "buyer-1",
    capabilityOwnerUserId: "buyer-1",
})

describe("hydrateSupplierEditor", () => {
    it("reads capability owner from center revision fields, not list keyFacts", () => {
        const center = {
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
            resourceFacts: [{ label: "能力负责人", value: "—" }],
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
                fields: [{ label: "能力负责人", value: "buyer-2" }],
            },
        } as MasterDataCenterView
        expect(hydrateSupplierEditor(center).capabilityOwnerUserId).toBe(
            "buyer-2",
        )
    })
})

describe("validateSupplierEditorFields", () => {
    it("accepts a concrete term matching its settlement mode", () => {
        expect(validateSupplierEditorFields(validValues())).toBeNull()
    })

    it("requires a concrete payment term", () => {
        expect(
            validateSupplierEditorFields({
                ...validValues(),
                paymentTerm: "先用后付",
            }),
        ).toBe("请选择具体付款条件")
    })

    it("rejects a payment term from another settlement mode", () => {
        expect(
            validateSupplierEditorFields({
                ...validValues(),
                paymentTerm: "PREPAY_30",
            }),
        ).toBe("结算方式与付款条件不一致，请重新选择")
    })
})
