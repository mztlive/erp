import { describe, expect, test } from "vitest"

import { businessLabelOrPlaceholder } from "./display-labels"
import {
    missingSourceDocumentNo,
    payablePreviewHref,
    paymentPreviewHref,
    sourceDocumentHref,
} from "./related-documents"

describe("businessLabelOrPlaceholder", () => {
    test("保留并清理真实业务名称", () => {
        expect(
            businessLabelOrPlaceholder(
                " 华东供应商 ",
                "sup-1",
                "供应商名称待补全",
            ),
        ).toBe("华东供应商")
    })

    test("空名称不回退内部 ID", () => {
        expect(
            businessLabelOrPlaceholder(undefined, "sup-1", "供应商名称待补全"),
        ).toBe("供应商名称待补全")
    })

    test("伪装成名称的内部 ID 不上屏", () => {
        expect(
            businessLabelOrPlaceholder(" sup-1 ", "sup-1", "供应商名称待补全"),
        ).toBe("供应商名称待补全")
    })
})

describe("related document hrefs", () => {
    test("采购单与结算单生成对象中心地址", () => {
        expect(sourceDocumentHref("PURCHASE_ORDER", "po-1")).toBe(
            "/procurement/orders/po-1",
        )
        expect(sourceDocumentHref("SUPPLIER_SETTLEMENT", "st-1")).toBe(
            "/supplier-api/settlements/st-1",
        )
        expect(sourceDocumentHref("PURCHASE_ORDER", "  ")).toBeUndefined()
    })

    test("往来预览地址保留视图", () => {
        expect(payablePreviewHref("pa-1")).toBe(
            "/finance/supplier-accounts?view=payable&detailId=pa-1&previewKind=payable",
        )
        expect(paymentPreviewHref("pay-1")).toBe(
            "/finance/supplier-accounts?view=payment&detailId=pay-1&previewKind=payment",
        )
    })

    test("缺失单号占位按来源类型区分", () => {
        expect(missingSourceDocumentNo("PURCHASE_ORDER")).toBe("采购单号待补全")
        expect(missingSourceDocumentNo("SUPPLIER_SETTLEMENT")).toBe(
            "结算单号待补全",
        )
    })
})
