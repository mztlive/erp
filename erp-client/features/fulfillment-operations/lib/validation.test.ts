import { describe, expect, it } from "vitest"

import { clientValidation, confirmDescription } from "./validation"
import type {
    FulfillmentDraft,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { makeOperation } from "@/features/fulfillment-operations/pages/hooks/test-data"

describe("confirmDescription", () => {
    it("keeps supplier direct to one sentence", () => {
        const draft: FulfillmentDraft = {
            type: "SUPPLIER_DIRECT",
            carrier: "顺丰速运",
            trackingNo: "SF1",
            shippedAt: "2026-08-28T09:31:00",
            lines: [
                {
                    salesOrderLineId: "sol_1",
                    purchaseLineSalesAllocationId: "alloc_1",
                    quantity: "1",
                },
            ],
        }
        expect(confirmDescription(draft)).toBe(
            "供应商直发给客户，不走自有仓库。确认后不能改。",
        )
    })

    it("mentions receipt quantities without a second remaining line", () => {
        const draft: FulfillmentDraft = {
            type: "RECEIPT",
            warehouseId: "wh_1",
            warehouseLabel: "中心仓",
            occurredAt: "2026-08-14T09:00:00.000Z",
            lines: [
                {
                    purchaseRevisionLineId: "prl_1",
                    receivedQuantity: "10",
                    qualifiedQuantity: "8",
                    rejectedQuantity: "2",
                    qualityResult: "PARTIAL",
                },
            ],
        }
        expect(confirmDescription(draft)).toBe(
            "合格 8 入库存并留货，不合格 2 不入库。确认后不能改。",
        )
    })

    it("mentions service evidence in the confirm sentence", () => {
        expect(confirmDescription(serviceDraft())).toBe(
            "服务结果：成功。已附现场图片凭证，不动库存。确认后不能改。",
        )
    })
})

describe("clientValidation type alignment", () => {
    it("rejects a leftover receipt draft on a supplier-direct job", () => {
        const operation = {
            ...makeOperation({
                operationId: "dlv_direct_1",
                operationType: "SUPPLIER_DIRECT",
            }),
            operationType: "SUPPLIER_DIRECT" as const,
            draft: {
                type: "SUPPLIER_DIRECT" as const,
                carrier: "",
                trackingNo: "",
                shippedAt: "2026-08-31T11:00",
                lines: [],
            },
        }
        const issues = clientValidation(operation, {
            type: "RECEIPT",
            warehouseId: "",
            warehouseLabel: "",
            occurredAt: "",
            lines: [],
        })
        expect(issues).toEqual([
            {
                id: "type-mismatch",
                label: "单据类型",
                message: "这条草稿和当前单据对不上",
            },
        ])
    })
})

describe("clientValidation service evidence", () => {
    it("blocks confirm when the fulfillment result is missing", () => {
        const issues = clientValidation(
            serviceOperation(),
            serviceDraft({ result: "" }),
        )
        expect(issues.map((issue) => issue.id)).toContain("svc-result")
    })

    it("blocks confirm when the site photo is missing", () => {
        const issues = clientValidation(serviceOperation(), serviceDraft())
        expect(issues.map((issue) => issue.id)).toContain("svc-evidence")
    })

    it("accepts a jpeg site photo", () => {
        const issues = clientValidation(
            serviceOperation(),
            serviceDraft({
                evidenceAttachmentId: "pending-file:service-evidence",
                evidenceFile: new File(["ok"], "site.jpg", {
                    type: "image/jpeg",
                }),
            }),
        )
        expect(issues.map((issue) => issue.id)).not.toContain("svc-evidence")
        expect(issues.map((issue) => issue.id)).not.toContain(
            "svc-evidence-type",
        )
    })
})

/**
 * 构造可确认的线下服务草稿，缺省不含图片凭证。
 *
 * @param overrides 覆盖字段。
 * @returns 线下服务草稿。
 */
function serviceDraft(
    overrides: Partial<Extract<FulfillmentDraft, { type: "SERVICE" }>> = {},
): Extract<FulfillmentDraft, { type: "SERVICE" }> {
    return {
        type: "SERVICE",
        startedAt: "2026-08-28T09:00:00",
        endedAt: "2026-08-28T11:00:00",
        serviceLocation: "客户现场",
        result: "SUCCESS",
        completionNote: "上门安装完成",
        evidenceAttachmentId: "",
        lines: [
            {
                salesOrderLineId: "sol_1",
                purchaseLineSalesAllocationId: "alloc_1",
                quantity: "1",
            },
        ],
        ...overrides,
    }
}

/**
 * 构造与 [serviceDraft] 配套的线下服务工作单。
 *
 * @returns 作业类型为 SERVICE 的工作单。
 */
function serviceOperation(): FulfillmentOperation {
    return {
        ...makeOperation({ operationType: "SERVICE" }),
        operationType: "SERVICE",
        draft: serviceDraft(),
        lines: [
            {
                lineId: "line_1",
                salesOrderLineId: "sol_1",
                purchaseLineSalesAllocationId: "alloc_1",
                itemName: "上门安装",
                skuCode: "SKU-S1",
                unitCode: "次",
                orderedQuantity: "1",
                remainingQuantity: "1",
            },
        ],
    }
}
