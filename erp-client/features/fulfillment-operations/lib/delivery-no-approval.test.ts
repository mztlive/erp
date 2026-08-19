import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import { deliveryToOperation } from "@/features/fulfillment-operations/api/documents"
import { formalFromDelivery } from "@/features/fulfillment-operations/api/outcomes"
import type { BackendDelivery } from "@/features/fulfillment-operations/api/documents"
import {
    FULFILLMENT_DELIVERY_APPROVAL_REQUIREMENT,
    FULFILLMENT_DELIVERY_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/fulfillment/delivery-page-proof"
import {
    DELIVERY_APPROVAL_REQUIREMENT,
    DELIVERY_DOCUMENT_TYPE,
    DELIVERY_DTO_HAS_NO_APPROVAL,
    DELIVERY_OBJECT_TYPE,
    DELIVERY_OPERATION_HAS_NO_APPROVAL,
    DELIVERY_OUTCOME_HAS_NO_APPROVAL,
    deliveryActionsExcludeApproval,
    isDeliveryOperation,
    isDeliveryWorkItem,
    stripDeliveryApprovalField,
} from "./delivery-no-approval"

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

function warehouseDeliverySeed(): BackendDelivery {
    return {
        id: "dl-1",
        delivery_no: "FH-2026-001",
        delivery_type: "WAREHOUSE_SHIP",
        sales_order_id: "so-1",
        warehouse_id: "wh-1",
        status: "DRAFT",
        version: 1,
        created_at: 1_700_000_000,
    }
}

function directDeliverySeed(): BackendDelivery {
    return {
        id: "dl-2",
        delivery_no: "FH-2026-002",
        delivery_type: "SUPPLIER_DIRECT",
        sales_order_id: "so-2",
        purchase_order_id: "po-2",
        status: "DRAFT",
        version: 1,
        created_at: 1_700_000_000,
    }
}

describe("DELIVERY_DOCUMENT_TYPE", () => {
    it("uses the contract type and does not alias receipts or electronic deliveries", () => {
        expect(DELIVERY_DOCUMENT_TYPE).toBe("Delivery")
        expect(DELIVERY_OBJECT_TYPE).toBe("delivery")
        expect(DELIVERY_APPROVAL_REQUIREMENT).toBe("NO_APPROVAL")
        expect(DELIVERY_DOCUMENT_TYPE).not.toBe("PurchaseReceipt")
        expect(DELIVERY_DOCUMENT_TYPE).not.toBe("ElectronicDelivery")
        expect(DELIVERY_DOCUMENT_TYPE).not.toBe("CustomerAcceptance")
        expect(DELIVERY_DTO_HAS_NO_APPROVAL).toBe(true)
        expect(DELIVERY_OPERATION_HAS_NO_APPROVAL).toBe(true)
        expect(DELIVERY_OUTCOME_HAS_NO_APPROVAL).toBe(true)
    })
})

describe("isDeliveryWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isDeliveryWorkItem({
                businessObjectType: "Delivery",
            }),
        ).toBe(true)
        expect(
            isDeliveryWorkItem({
                businessObjectType: "delivery",
            }),
        ).toBe(true)
        expect(
            isDeliveryWorkItem({ businessObjectType: "PurchaseReceipt" }),
        ).toBe(false)
        expect(
            isDeliveryWorkItem({
                businessObjectType: "ElectronicDelivery",
            }),
        ).toBe(false)
        expect(isDeliveryWorkItem(undefined)).toBe(false)
    })
})

describe("isDeliveryOperation", () => {
    it("accepts warehouse ship and supplier direct only", () => {
        expect(isDeliveryOperation({ operationType: "WAREHOUSE_SHIP" })).toBe(
            true,
        )
        expect(isDeliveryOperation({ operationType: "SUPPLIER_DIRECT" })).toBe(
            true,
        )
        expect(isDeliveryOperation({ operationType: "RECEIPT" })).toBe(false)
        expect(isDeliveryOperation({ operationType: "ELECTRONIC" })).toBe(false)
        expect(isDeliveryOperation(undefined)).toBe(false)
    })
})

describe("stripDeliveryApprovalField", () => {
    it("drops a stray approval field and leaves delivery facts intact", () => {
        const stripped = stripDeliveryApprovalField({
            id: "dl-1",
            delivery_no: "FH-1",
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT"],
            },
        })
        expect(stripped).toEqual({ id: "dl-1", delivery_no: "FH-1" })
        expect("approval" in stripped).toBe(false)
        expect(stripDeliveryApprovalField({ id: "dl-2" })).toEqual({
            id: "dl-2",
        })
    })
})

describe("deliveryActionsExcludeApproval", () => {
    it("accepts delivery business actions and rejects approval entries", () => {
        expect(
            deliveryActionsExcludeApproval(["VIEW_DETAIL", "SAVE", "POST"]),
        ).toBe(true)
        expect(deliveryActionsExcludeApproval(["APPROVE", "REJECT"])).toBe(
            false,
        )
        expect(deliveryActionsExcludeApproval(["CANCEL"])).toBe(false)
        expect(deliveryActionsExcludeApproval(["UPGRADE_BINDING"])).toBe(false)
        expect(deliveryActionsExcludeApproval(["SUBMIT"])).toBe(false)
    })
})

describe("deliveryToOperation", () => {
    it("maps a warehouse delivery without an approval projection", () => {
        const operation = deliveryToOperation(warehouseDeliverySeed())
        expect(operation.operationType).toBe("WAREHOUSE_SHIP")
        expect(operation.summary).toBe("FH-2026-001")
        expect("approval" in operation).toBe(false)
        expect(
            deliveryActionsExcludeApproval(
                operation.actionBlockers.map((blocker) => blocker.action),
            ),
        ).toBe(true)
    })

    it("maps a supplier-direct delivery without an approval projection", () => {
        const operation = deliveryToOperation(directDeliverySeed())
        expect(operation.operationType).toBe("SUPPLIER_DIRECT")
        expect(operation.summary).toBe("FH-2026-002")
        expect("approval" in operation).toBe(false)
    })

    it("strips a stray approval field instead of rendering a binding", () => {
        const operation = deliveryToOperation({
            ...warehouseDeliverySeed(),
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT", "APPROVE"],
            },
        } as BackendDelivery & { approval: unknown })
        expect("approval" in operation).toBe(false)
        expect(operation.operationType).toBe("WAREHOUSE_SHIP")
    })
})

describe("formalFromDelivery", () => {
    it("projects a posted warehouse delivery without an approval zone", () => {
        const outcome = formalFromDelivery(
            {
                ...warehouseDeliverySeed(),
                status: "SHIPPED",
                shipped_at: 1_700_000_100,
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["APPROVE"],
                },
            } as BackendDelivery & { approval: unknown },
            {
                type: "WAREHOUSE_SHIP",
                warehouseId: "wh-1",
                warehouseLabel: "中心仓",
                carrier: "顺丰",
                trackingNo: "SF-1",
                shippedAt: "2026-08-14T09:00",
                lines: [],
            },
            "dl-1",
        )
        expect(outcome.factType).toBe("DELIVERY")
        expect(outcome.factNo).toBe("FH-2026-001")
        expect("approval" in outcome).toBe(false)
    })

    it("projects a posted supplier-direct delivery without an approval zone", () => {
        const outcome = formalFromDelivery(
            {
                ...directDeliverySeed(),
                status: "SHIPPED",
                shipped_at: 1_700_000_100,
            },
            {
                type: "SUPPLIER_DIRECT",
                carrier: "顺丰",
                trackingNo: "SF-2",
                shippedAt: "2026-08-14T09:00",
                lines: [],
            },
            "dl-2",
        )
        expect(outcome.factType).toBe("DELIVERY")
        expect(outcome.operationType).toBe("SUPPLIER_DIRECT")
        expect("approval" in outcome).toBe(false)
    })
})

describe("delivery source paths omit the approval zone", () => {
    it("keeps delivery-only files free of approval components", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("components/forms/fulfillment-ship-form.tsx"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(
                readFeature("components/forms/fulfillment-direct-form.tsx"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(
                readFeature("lib/delivery-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("lib/delivery-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("components/forms/fulfillment-ship-form.tsx"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("components/forms/fulfillment-direct-form.tsx"),
            ),
        ).toBe(false)
    })

    it("does not embed the approval zone inside the delivery create result", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("pages/components/fulfillment-result-panel.tsx"),
            ),
        ).toBe(false)
        expect(sourceMentionsApprovalZone(readFeature("api/outcomes.ts"))).toBe(
            false,
        )
    })

    it("routes delivery submit confirmation through the business dialog, not an approval zone", () => {
        const source = readFeature("pages/fulfillment-operations-page.tsx")
        expect(source).toContain("FormalActionConfirmDialog")
        expect(source).toContain("OPERATION_CONFIRM_TITLE")
        expect(sourceMentionsApprovalZone(source)).toBe(false)
        expect(sourceMentionsBpmInternals(source)).toBe(false)
        expect(source).toContain("Delivery 为 NO_APPROVAL")
    })

    it("does not dispatch warehouse-ship drafts to an approval form", () => {
        const source = readFeature(
            "components/forms/fulfillment-draft-form.tsx",
        )
        const start = source.indexOf('case "WAREHOUSE_SHIP"')
        const end = source.indexOf('case "SUPPLIER_DIRECT"')
        expect(start).toBeGreaterThan(-1)
        expect(end).toBeGreaterThan(start)
        const shipBranch = source.slice(start, end)
        expect(shipBranch).toContain("FulfillmentShipForm")
        expect(sourceMentionsApprovalZone(shipBranch)).toBe(false)
    })

    it("does not dispatch supplier-direct drafts to an approval form", () => {
        const source = readFeature(
            "components/forms/fulfillment-draft-form.tsx",
        )
        const start = source.indexOf('case "SUPPLIER_DIRECT"')
        const end = source.indexOf('case "ELECTRONIC"')
        expect(start).toBeGreaterThan(-1)
        expect(end).toBeGreaterThan(start)
        const directBranch = source.slice(start, end)
        expect(directBranch).toContain("FulfillmentDirectForm")
        expect(sourceMentionsApprovalZone(directBranch)).toBe(false)
    })
})

describe("fulfillment page delivery proof", () => {
    it("declares NO_APPROVAL and does not wire delivery approval actions", () => {
        expect(FULFILLMENT_DELIVERY_APPROVAL_REQUIREMENT).toBe("NO_APPROVAL")
        expect(FULFILLMENT_DELIVERY_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining([
                "选择流程",
                "通过",
                "撤回审批",
                "改派当前审批人",
            ]),
        )
        const pageSource = readFileSync(pagePath, "utf8")
        expect(pageSource).toContain("FulfillmentOperationsPage")
        expect(pageSource).toContain("Delivery 为 NO_APPROVAL")
        expect(sourceMentionsApprovalZone(pageSource)).toBe(false)
        expect(sourceMentionsBpmInternals(pageSource)).toBe(false)
        for (const label of FULFILLMENT_DELIVERY_FORBIDDEN_ACTIONS) {
            expect(pageSource).not.toContain(label)
        }
    })
})
