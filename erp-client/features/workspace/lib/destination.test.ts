import { expect, test } from "vitest"
import { buildDocumentHref } from "./destination"

const item = {
    businessObjectId: "object-1",
    rootBusinessObjectId: "parent-1",
    workItemId: "task-1",
    queueContextId: "queue-1",
    workItemType: "SALES_INVOICE_EXECUTION",
    businessObjectType: "receivable_account",
}

test.each([
    ["supplier_payment_execution", "W12", "session", "detailId"],
    ["sales_invoice_execution", "W11", "register", "previewId"],
] as const)(
    "%s 查看当前单据保留对象身份但不打开执行表单",
    (handlerKey, destinationWorkspaceId, trigger, objectKey) => {
        const href = buildDocumentHref({
            ...item,
            handlerKey,
            destinationWorkspaceId,
        })
        const url = new URL(href!, "http://erp.test")
        expect(url.searchParams.has(trigger)).toBe(false)
        expect(url.searchParams.get(objectKey)).toBe("object-1")
    },
)

test("供给分配查看销售单，履约查看来源单据，不跳回当前任务", () => {
    expect(
        buildDocumentHref({
            ...item,
            handlerKey: "procurement_order_creation",
            destinationWorkspaceId: "W08",
        }),
    ).toBe("/sales/orders/object-1?from=workspace")
    for (const [businessObjectType, path] of [
        ["delivery", "/sales/orders"],
        ["purchase_receipt", "/procurement/orders"],
        ["electronic_delivery", "/procurement/orders"],
        ["service_fulfillment", "/procurement/orders"],
    ]) {
        expect(
            buildDocumentHref({
                ...item,
                businessObjectType,
                handlerKey: "fulfillment_operation",
                destinationWorkspaceId: "W01",
            }),
        ).toBe(`${path}/parent-1?from=workspace`)
    }
})
