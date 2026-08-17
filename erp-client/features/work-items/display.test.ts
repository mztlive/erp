import { describe, expect, it } from "vitest"

import {
    displayImpactSummary,
    displayOwnerName,
    displayReasonLabel,
    queueResponsibilityLabel,
} from "./display"

describe("displayReasonLabel", () => {
    it("maps known internal reason codes to Chinese", () => {
        expect(
            displayReasonLabel({
                reasonCode: "procurement_confirmation_dispatched",
                reasonLabel: "procurement confirmation dispatched",
            }),
        ).toBe("销售已提交，需要采购确认能否供货")
    })

    it("hides leftover English reason codes", () => {
        expect(
            displayReasonLabel({
                reasonCode: "UNKNOWN_EVENT",
                reasonLabel: "unknown event",
            }),
        ).toBe("需要你处理")
    })
})

describe("displayImpactSummary", () => {
    it("rejects template echoes and mechanism copy", () => {
        expect(
            displayImpactSummary({
                impactSummary: "采购二次确认：销售提交 1",
                workItemTypeLabel: "采购二次确认",
            }),
        ).toBe("不确认则销售单不能生效")
        expect(
            displayImpactSummary({
                impactSummary: "请打开业务对象核对影响。",
            }),
        ).toBe("不处理将卡住后续业务，请进入对应页面核对。")
        expect(
            displayImpactSummary({
                impactSummary: "采购单 PO-20260817-9a550b 待财务审核",
                workItemType: "PURCHASE_ORDER_REVIEW",
            }),
        ).toBe("不审核则不能形成应付、不能付款")
    })

    it("maps purchase review reason codes", () => {
        expect(
            displayReasonLabel({
                reasonCode: "purchase_order_review_dispatched",
                reasonLabel: "purchase order review dispatched",
            }),
        ).toBe("采购已提交，需要核对成本、进项税和付款条件")
    })
})

describe("queueResponsibilityLabel", () => {
    it("uses 由你处理 for the current user", () => {
        expect(
            queueResponsibilityLabel({
                assignmentMode: "DIRECT",
                ownerUser: { id: "me", displayName: "周航" },
                viewerUserId: "me",
            }),
        ).toBe("由你处理")
    })

    it("never shows 当前处理人", () => {
        expect(displayOwnerName("当前处理人")).toBe("处理人待确认")
        expect(
            queueResponsibilityLabel({
                assignmentMode: "POOL",
                ownerUser: { id: "u1", displayName: "当前处理人" },
            }),
        ).toBe("处理人待确认")
    })
})
