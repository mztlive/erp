import { describe, expect, it } from "vitest"

import { makeOperation } from "../hooks/test-data"
import { sourceContextFields } from "./presentation"

describe("sourceContextFields", () => {
    it("omits empty, placeholder and opaque values", () => {
        const operation = makeOperation({
            operationType: "SUPPLIER_DIRECT",
            source: {
                salesOrderId: "10cbccde28ac43639335656a02fd62f4",
                salesOrderNo: "10cbccde28ac43639335656a02fd62f4",
                salesRevisionId: "",
                purchaseNo: "",
                customerLabel: "",
                supplierLabel: "—",
                warehouseLabel: "不涉及仓库",
            },
        })
        expect(sourceContextFields(operation)).toEqual([
            { label: "还剩多少", value: "演示商品 10件", numeric: true },
        ])
        expect(
            sourceContextFields(operation).some(
                (field) => field.label === "销售单" || field.label === "仓库",
            ),
        ).toBe(false)
    })

    it("keeps readable sales order and counterparty", () => {
        const operation = makeOperation({
            source: {
                salesOrderNo: "SO-1",
                customerLabel: "演示客户",
                supplierLabel: "演示供应商",
                purchaseNo: "PO-1",
            },
        })
        expect(
            sourceContextFields(operation, "/sales/orders/so_1").map(
                (field) => field.label,
            ),
        ).toEqual(["销售单", "采购单", "客户", "供应商", "仓库", "还剩多少"])
        expect(sourceContextFields(operation, "/sales/orders/so_1")[0]).toEqual(
            {
                label: "销售单",
                value: "SO-1",
                href: "/sales/orders/so_1",
            },
        )
    })
})
