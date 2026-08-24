import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import { electronicToOperation } from "@/features/fulfillment-operations/api/documents"
import { formalFromElectronic } from "@/features/fulfillment-operations/api/outcomes"
import type { BackendElectronicDelivery } from "@/features/fulfillment-operations/api/documents"
import {
    FULFILLMENT_ELECTRONIC_DELIVERY_APPROVAL_REQUIREMENT,
    FULFILLMENT_ELECTRONIC_DELIVERY_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/fulfillment/electronic-delivery-page-proof"
import {
    ELECTRONIC_DELIVERY_APPROVAL_REQUIREMENT,
    ELECTRONIC_DELIVERY_DOCUMENT_TYPE,
    ELECTRONIC_DELIVERY_DTO_HAS_NO_APPROVAL,
    ELECTRONIC_DELIVERY_OBJECT_TYPE,
    ELECTRONIC_DELIVERY_OPERATION_HAS_NO_APPROVAL,
    ELECTRONIC_DELIVERY_OUTCOME_HAS_NO_APPROVAL,
    electronicDeliveryActionsExcludeApproval,
    isElectronicDeliveryOperation,
    isElectronicDeliveryWorkItem,
    stripElectronicDeliveryApprovalField,
} from "./electronic-delivery-no-approval"

const here = dirname(fileURLToPath(import.meta.url))
const featureRoot = join(here, "..")
const pagePath = join(here, "../../../app/(workspace)/fulfillment/page.tsx")

const APPROVAL_ZONE_TOKENS = [
    "DefinitionBindingCard",
    "DecisionDialog",
    "ReassignDialog",
    "CancelApprovalDialog",
    "SubmissionRouteConfirmation",
    "RuntimeSummary",
    "ExecutionHistory",
    "ApprovalActionBar",
    "UpgradeBindingDialog",
    "ResumeApproverDialog",
] as const

const BPM_INTERNAL_TOKENS = [
    "ProcessKind",
    "SubjectRef",
    "TransitionPlan",
] as const

function sourceMentionsApprovalZone(source: string): boolean {
    return APPROVAL_ZONE_TOKENS.some((token) => source.includes(token))
}

function sourceMentionsBpmInternals(source: string): boolean {
    return BPM_INTERNAL_TOKENS.some((token) => source.includes(token))
}

function readFeature(relativePath: string): string {
    return readFileSync(join(featureRoot, relativePath), "utf8")
}

function electronicSeed(): BackendElectronicDelivery {
    return {
        id: "ed-1",
        fulfillment_no: "DZ-2026-001",
        sales_order_line_id: "sol-1",
        purchase_order_id: "po-1",
        purchase_line_sales_allocation_id: "alloc-1",
        quantity: "5",
        result: "SUCCESS",
        status: "DRAFT",
        occurred_at: 1_700_000_000,
        recorded_at: 1_700_000_000,
        version: 1,
    }
}

describe("ELECTRONIC_DELIVERY_DOCUMENT_TYPE", () => {
    it("uses the contract type and does not alias receipts or warehouse deliveries", () => {
        expect(ELECTRONIC_DELIVERY_DOCUMENT_TYPE).toBe("ElectronicDelivery")
        expect(ELECTRONIC_DELIVERY_OBJECT_TYPE).toBe("electronic_delivery")
        expect(ELECTRONIC_DELIVERY_APPROVAL_REQUIREMENT).toBe("NO_APPROVAL")
        expect(ELECTRONIC_DELIVERY_DOCUMENT_TYPE).not.toBe("PurchaseReceipt")
        expect(ELECTRONIC_DELIVERY_DOCUMENT_TYPE).not.toBe("Delivery")
        expect(ELECTRONIC_DELIVERY_DOCUMENT_TYPE).not.toBe("ServiceFulfillment")
        expect(ELECTRONIC_DELIVERY_DTO_HAS_NO_APPROVAL).toBe(true)
        expect(ELECTRONIC_DELIVERY_OPERATION_HAS_NO_APPROVAL).toBe(true)
        expect(ELECTRONIC_DELIVERY_OUTCOME_HAS_NO_APPROVAL).toBe(true)
    })
})

describe("isElectronicDeliveryWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isElectronicDeliveryWorkItem({
                businessObjectType: "ElectronicDelivery",
            }),
        ).toBe(true)
        expect(
            isElectronicDeliveryWorkItem({
                businessObjectType: "electronic_delivery",
            }),
        ).toBe(true)
        expect(
            isElectronicDeliveryWorkItem({
                businessObjectType: "PurchaseReceipt",
            }),
        ).toBe(false)
        expect(
            isElectronicDeliveryWorkItem({
                businessObjectType: "Delivery",
            }),
        ).toBe(false)
        expect(isElectronicDeliveryWorkItem(undefined)).toBe(false)
    })
})

describe("isElectronicDeliveryOperation", () => {
    it("accepts electronic delivery only", () => {
        expect(
            isElectronicDeliveryOperation({ operationType: "ELECTRONIC" }),
        ).toBe(true)
        expect(
            isElectronicDeliveryOperation({ operationType: "WAREHOUSE_SHIP" }),
        ).toBe(false)
        expect(
            isElectronicDeliveryOperation({ operationType: "RECEIPT" }),
        ).toBe(false)
        expect(
            isElectronicDeliveryOperation({ operationType: "SERVICE" }),
        ).toBe(false)
        expect(isElectronicDeliveryOperation(undefined)).toBe(false)
    })
})

describe("stripElectronicDeliveryApprovalField", () => {
    it("drops a stray approval field and leaves electronic delivery facts intact", () => {
        const stripped = stripElectronicDeliveryApprovalField({
            id: "ed-1",
            fulfillment_no: "DZ-1",
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT"],
            },
        })
        expect(stripped).toEqual({ id: "ed-1", fulfillment_no: "DZ-1" })
        expect("approval" in stripped).toBe(false)
        expect(stripElectronicDeliveryApprovalField({ id: "ed-2" })).toEqual({
            id: "ed-2",
        })
    })
})

describe("electronicDeliveryActionsExcludeApproval", () => {
    it("accepts electronic delivery business actions and rejects approval entries", () => {
        expect(
            electronicDeliveryActionsExcludeApproval([
                "VIEW_DETAIL",
                "POST",
                "CONFIRM",
            ]),
        ).toBe(true)
        expect(
            electronicDeliveryActionsExcludeApproval(["APPROVE", "REJECT"]),
        ).toBe(false)
        expect(electronicDeliveryActionsExcludeApproval(["CANCEL"])).toBe(false)
        expect(
            electronicDeliveryActionsExcludeApproval(["UPGRADE_BINDING"]),
        ).toBe(false)
        expect(electronicDeliveryActionsExcludeApproval(["SUBMIT"])).toBe(false)
    })
})

describe("electronicToOperation", () => {
    it("maps an electronic delivery without an approval projection", () => {
        const operation = electronicToOperation(electronicSeed())
        expect(operation.operationType).toBe("ELECTRONIC")
        expect(operation.summary).toBe("DZ-2026-001")
        expect("approval" in operation).toBe(false)
        expect(
            electronicDeliveryActionsExcludeApproval(
                operation.actionBlockers.map((blocker) => blocker.action),
            ),
        ).toBe(true)
    })

    it("strips a stray approval field instead of rendering a binding", () => {
        const operation = electronicToOperation({
            ...electronicSeed(),
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT", "APPROVE"],
            },
        } as BackendElectronicDelivery & { approval: unknown })
        expect("approval" in operation).toBe(false)
        expect(operation.operationType).toBe("ELECTRONIC")
        expect(operation.summary).toBe("DZ-2026-001")
    })
})

describe("formalFromElectronic", () => {
    it("projects a confirmed electronic delivery without an approval zone", () => {
        const outcome = formalFromElectronic(
            {
                ...electronicSeed(),
                status: "CONFIRMED",
                result: "SUCCESS",
                occurred_at: 1_700_000_100,
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["APPROVE"],
                },
            } as BackendElectronicDelivery & { approval: unknown },
            {
                type: "ELECTRONIC",
                occurredAt: "2026-08-14T09:00",
                recipientMasked: "138****0001",
                result: "SUCCESS",
                lines: [
                    {
                        salesOrderLineId: "sol-1",
                        purchaseLineSalesAllocationId: "alloc-1",
                        quantity: "5",
                    },
                ],
            },
            "ed-1",
        )
        expect(outcome.factType).toBe("ELECTRONIC_DELIVERY")
        expect(outcome.factNo).toBe("DZ-2026-001")
        expect(outcome.operationType).toBe("ELECTRONIC")
        expect("approval" in outcome).toBe(false)
    })

    it("projects a failed electronic delivery without an approval zone", () => {
        const outcome = formalFromElectronic(
            {
                ...electronicSeed(),
                status: "FAILED",
                result: "FAILED",
            },
            {
                type: "ELECTRONIC",
                occurredAt: "2026-08-14T09:00",
                recipientMasked: "138****0001",
                result: "FAILED",
                lines: [
                    {
                        salesOrderLineId: "sol-1",
                        purchaseLineSalesAllocationId: "alloc-1",
                        quantity: "5",
                    },
                ],
            },
            "ed-1",
        )
        expect(outcome.formalStatus).toBe("FAILED")
        expect(outcome.acceptanceRequired).toBe(false)
        expect("approval" in outcome).toBe(false)
    })
})

describe("electronic delivery source paths omit the approval zone", () => {
    it("keeps electronic-delivery-only files free of approval components", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("components/forms/fulfillment-electronic-form.tsx"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(
                readFeature("lib/electronic-delivery-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("lib/electronic-delivery-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("components/forms/fulfillment-electronic-form.tsx"),
            ),
        ).toBe(false)
    })

    it("does not embed the approval zone inside the electronic delivery create result", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("pages/components/fulfillment-result-panel.tsx"),
            ),
        ).toBe(false)
        expect(sourceMentionsApprovalZone(readFeature("api/outcomes.ts"))).toBe(
            false,
        )
    })

    it("routes electronic delivery submit confirmation through the business dialog, not an approval zone", () => {
        const source = [
            readFeature(
                "pages/components/fulfillment-operations-workspace.tsx",
            ),
            readFeature("pages/components/fulfillment-result-panel.tsx"),
        ].join("\n")
        expect(source).toContain("FormalActionConfirmDialog")
        expect(source).toContain("OPERATION_CONFIRM_TITLE")
        expect(sourceMentionsApprovalZone(source)).toBe(false)
        expect(sourceMentionsBpmInternals(source)).toBe(false)
        expect(source).toContain("ElectronicDelivery 为 NO_APPROVAL")
    })

    it("does not dispatch ELECTRONIC drafts to an approval form", () => {
        const source = readFeature(
            "components/forms/fulfillment-draft-form.tsx",
        )
        const start = source.indexOf('case "ELECTRONIC"')
        const end = source.indexOf('case "SERVICE"')
        expect(start).toBeGreaterThan(-1)
        expect(end).toBeGreaterThan(start)
        const electronicBranch = source.slice(start, end)
        expect(electronicBranch).toContain("FulfillmentElectronicForm")
        expect(sourceMentionsApprovalZone(electronicBranch)).toBe(false)
        expect(sourceMentionsBpmInternals(electronicBranch)).toBe(false)
    })
})

describe("fulfillment page electronic delivery proof", () => {
    it("declares NO_APPROVAL and does not wire electronic delivery approval actions", () => {
        expect(FULFILLMENT_ELECTRONIC_DELIVERY_APPROVAL_REQUIREMENT).toBe(
            "NO_APPROVAL",
        )
        expect(FULFILLMENT_ELECTRONIC_DELIVERY_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining([
                "选择流程",
                "通过",
                "撤回审批",
                "改派当前审批人",
            ]),
        )
        const pageSource = readFileSync(pagePath, "utf8")
        expect(pageSource).toContain("FulfillmentOperationsPage")
        expect(pageSource).toContain("ElectronicDelivery 为 NO_APPROVAL")
        expect(sourceMentionsApprovalZone(pageSource)).toBe(false)
        expect(sourceMentionsBpmInternals(pageSource)).toBe(false)
        for (const label of FULFILLMENT_ELECTRONIC_DELIVERY_FORBIDDEN_ACTIONS) {
            expect(pageSource).not.toContain(label)
        }
    })
})
