import { describe, expect, it } from "vitest"

import {
    canCommand,
    canNavigateWithPermission,
    gateCancelSalesOrderApproval,
    gateCommand,
    gateNavigate,
    OPEN_FULFILLMENT_PERMISSIONS,
    permissionBlockedReason,
    SALES_ORDER_DETAIL_PERMISSIONS,
} from "@/features/sales-orders/lib/sales-order-detail-permissions"

describe("sales-order-detail-permissions", () => {
    it("allows navigation when any matching permission is granted", () => {
        expect(
            canNavigateWithPermission(
                ["purchase_order:list"],
                SALES_ORDER_DETAIL_PERMISSIONS.openPurchase,
            ),
        ).toBe(true)
        expect(
            canNavigateWithPermission(
                ["delivery:list"],
                OPEN_FULFILLMENT_PERMISSIONS,
            ),
        ).toBe(true)
        expect(
            canNavigateWithPermission(
                ["sales_order:detail"],
                SALES_ORDER_DETAIL_PERMISSIONS.openPurchase,
            ),
        ).toBe(false)
    })

    it("requires both server allow and casbin for commands", () => {
        expect(
            canCommand(
                true,
                ["sales_change_order:create"],
                SALES_ORDER_DETAIL_PERMISSIONS.startChange,
            ),
        ).toBe(true)
        expect(
            canCommand(
                false,
                ["sales_change_order:create"],
                SALES_ORDER_DETAIL_PERMISSIONS.startChange,
            ),
        ).toBe(false)
        expect(
            canCommand(
                true,
                ["sales_order:detail"],
                SALES_ORDER_DETAIL_PERMISSIONS.startChange,
            ),
        ).toBe(false)
    })

    it("builds navigate gates with Chinese reasons", () => {
        expect(gateNavigate(["*:*"], "openReceivable")).toEqual({
            enabled: true,
        })
        expect(gateNavigate([], "openPurchase")).toEqual({
            enabled: false,
            reason: permissionBlockedReason("openPurchase"),
        })
    })

    it("prefers server blocker when command is business-blocked", () => {
        expect(
            gateCommand(
                false,
                ["sales_change_order:create"],
                "startChange",
                "本单还在确认/审批中",
            ),
        ).toEqual({
            enabled: false,
            reason: "本单还在确认/审批中",
        })
        expect(
            gateCommand(true, ["sales_order:detail"], "startChange"),
        ).toEqual({
            enabled: false,
            reason: permissionBlockedReason("startChange"),
        })
    })
})

describe("gateCancelSalesOrderApproval", () => {
    const awaiting = {
        ownerUserId: "u-sales",
        primaryStatus: { code: "in_approval" },
        approval: { allowedActions: ["CANCEL"] as const },
    }

    it("allows the sales owner with cancel_approval permission", () => {
        expect(
            gateCancelSalesOrderApproval({
                order: awaiting,
                currentUserId: "u-sales",
                granted: ["sales_order:cancel_approval"],
            }),
        ).toEqual({ enabled: true })
    })

    it("blocks non-owners even with permission", () => {
        expect(
            gateCancelSalesOrderApproval({
                order: awaiting,
                currentUserId: "u-other",
                granted: ["*:*"],
            }).reason,
        ).toContain("负责销售本人")
    })
})
