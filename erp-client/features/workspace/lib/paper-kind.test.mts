import assert from "node:assert/strict"
import test from "node:test"

import {
    workspacePaperKind,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./paper-kind.ts"

test("sales and voucher orders share the sales paper adapter", () => {
    assert.equal(workspacePaperKind("sales_order"), "sales_order")
    assert.equal(workspacePaperKind("SalesOrder"), "sales_order")
    assert.equal(workspacePaperKind("voucher_sales_order"), "sales_order")
    assert.equal(workspacePaperKind("VoucherSalesOrder"), "sales_order")
})

test("purchase orders have a dedicated paper adapter", () => {
    assert.equal(workspacePaperKind("purchase_order"), "purchase_order")
    assert.equal(workspacePaperKind("PurchaseOrder"), "purchase_order")
})

test("ledger, change and exception objects do not get a paper adapter", () => {
    assert.equal(workspacePaperKind("customer_receipt"), null)
    assert.equal(workspacePaperKind("sales_change_order"), null)
    assert.equal(workspacePaperKind("stock_adjustment"), null)
    assert.equal(workspacePaperKind("import_batch"), null)
})
