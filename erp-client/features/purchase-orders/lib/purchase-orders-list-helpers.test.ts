import { describe, it, expect } from "vitest"

import type { PurchaseOrderListItem } from "@/features/purchase-orders/types"
import {
    buildPurchaseOrdersCsv,
    displayPurchaseOrderNo,
} from "./purchase-orders-list-helpers"

function makeRow(
    overrides: Partial<PurchaseOrderListItem> = {},
): PurchaseOrderListItem {
    return {
        purchaseOrderId: "po_1",
        purchaseNo: "PO-2026-001",
        draftLabel: undefined,
        status: "EFFECTIVE",
        statusLabel: "已生效",
        statusTone: "success",
        reviewStatus: "APPROVED",
        reviewLabel: "已通过",
        salesOrderId: "so_1",
        salesOrderNo: "SO-2026-001",
        supplierId: "sup_1",
        supplierName: "供应商A",
        purchaseType: "PHYSICAL",
        fulfillmentResponsibility: "WAREHOUSE",
        paymentTermCode: "POSTPAY_NET30",
        paymentTermLabel: "货到 30 天",
        ownerName: "张三",
        grossAmount: "1234.50",
        netAmount: "1100.00",
        taxAmount: "134.50",
        costMasked: false,
        paymentProgress: "未付",
        invoiceProgress: "未收",
        fulfillmentProgress: "未开始",
        paymentGate: "NOT_APPLICABLE",
        updatedAt: "2026-08-14T00:00:00.000Z",
        allowedActions: [],
        actionBlockers: [],
        ...overrides,
    }
}

describe("displayPurchaseOrderNo", () => {
    it("优先显示正式单号", () => {
        expect(displayPurchaseOrderNo(makeRow())).toBe("PO-2026-001")
    })

    it("无单号时回退草稿标签", () => {
        expect(
            displayPurchaseOrderNo(
                makeRow({
                    purchaseNo: undefined,
                    draftLabel: "草稿 · abc12345",
                }),
            ),
        ).toBe("草稿 · abc12345")
    })

    it("都缺失时显示占位文案", () => {
        expect(
            displayPurchaseOrderNo(
                makeRow({ purchaseNo: undefined, draftLabel: undefined }),
            ),
        ).toBe("采购单（未编号）")
    })
})

describe("buildPurchaseOrdersCsv", () => {
    it("生成表头与数据行", () => {
        const csv = buildPurchaseOrdersCsv([makeRow()])
        expect(csv).toBe(
            [
                "采购单号,状态,供应商,来源销售单,类型,含税金额,付款,履约,负责人",
                '"PO-2026-001","已生效","供应商A","SO-2026-001","实物","1234.50","未付","未开始","张三"',
            ].join("\n"),
        )
    })

    it("成本隐藏时金额列打码", () => {
        const csv = buildPurchaseOrdersCsv([
            makeRow({ costMasked: true, purchaseNo: "PO-2" }),
        ])
        expect(csv).toContain(
            '"PO-2","已生效","供应商A","SO-2026-001","实物","***"',
        )
    })

    it("字段含引号时正确转义", () => {
        const csv = buildPurchaseOrdersCsv([
            makeRow({ supplierName: '供"应"商' }),
        ])
        expect(csv).toContain('"供""应""商"')
    })

    it("空行数组输出仅表头", () => {
        expect(buildPurchaseOrdersCsv([])).toBe(
            "采购单号,状态,供应商,来源销售单,类型,含税金额,付款,履约,负责人",
        )
    })
})
