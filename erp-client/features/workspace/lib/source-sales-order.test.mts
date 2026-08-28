import assert from "node:assert/strict"
import test from "node:test"

import {
    findSourceSalesOrder,
    linkedDocumentHref,
    linkedDocumentPaperKind,
    ORIGINAL_SUPPLIER_PAYMENT_LABEL,
    SOURCE_SALES_ORDER_LABEL,
    sourceSalesOrderHref,
    withSourceSalesOrder,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./source-sales-order.ts"

test("finds the source sales order number and routing id", () => {
    assert.equal(findSourceSalesOrder([]), null)
    assert.deepEqual(
        findSourceSalesOrder([
            { label: "供应商", value: "华东纸业" },
            {
                label: SOURCE_SALES_ORDER_LABEL,
                value: " SO-1 ",
                objectId: " so-1 ",
            },
        ]),
        { orderNo: "SO-1", objectId: "so-1" },
    )
})

test("source sales order href stays on the sales workspace and can return", () => {
    const href = sourceSalesOrderHref("so/1", "/workspace?family=finance")
    const url = new URL(href, "https://erp.test")
    assert.equal(url.pathname, "/sales/orders/so%2F1")
    assert.equal(url.searchParams.get("from"), "workspace")
    assert.equal(url.searchParams.get("returnTo"), "/workspace?family=finance")
})

test("only 来源销售单 uses the sales paper adapter", () => {
    assert.equal(
        linkedDocumentPaperKind(SOURCE_SALES_ORDER_LABEL),
        "sales_order",
    )
    assert.equal(linkedDocumentPaperKind("来源采购单"), null)
    assert.equal(linkedDocumentHref("来源采购单", "po-1"), null)
    assert.ok(linkedDocumentHref(SOURCE_SALES_ORDER_LABEL, "so-1"))
    const paymentHref = linkedDocumentHref(
        ORIGINAL_SUPPLIER_PAYMENT_LABEL,
        "payment-1",
    )
    assert.ok(paymentHref)
    const paymentUrl = new URL(paymentHref, "https://erp.test")
    assert.equal(paymentUrl.pathname, "/finance/supplier-accounts")
    assert.equal(paymentUrl.searchParams.get("view"), "payment")
    assert.equal(paymentUrl.searchParams.get("detailId"), "payment-1")
    assert.equal(paymentUrl.searchParams.get("previewKind"), "payment")
})

test("injects or upgrades the source sales order section", () => {
    assert.deepEqual(
        withSourceSalesOrder([{ label: "付款条件", value: "先款 30%" }], {
            orderNo: "SO-1",
            objectId: "so-1",
        }),
        [
            {
                label: SOURCE_SALES_ORDER_LABEL,
                value: "SO-1",
                objectId: "so-1",
            },
            { label: "付款条件", value: "先款 30%" },
        ],
    )
    assert.deepEqual(
        withSourceSalesOrder(
            [
                {
                    label: SOURCE_SALES_ORDER_LABEL,
                    value: "SO-1",
                },
            ],
            { orderNo: "SO-1", objectId: "so-1" },
        ),
        [
            {
                label: SOURCE_SALES_ORDER_LABEL,
                value: "SO-1",
                objectId: "so-1",
            },
        ],
    )
})
