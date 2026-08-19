import type { AnchorHTMLAttributes, ReactNode } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { FormalActionConfirmDialog } from "@/components/business"
import { FULFILLMENT_DELIVERY_FORBIDDEN_ACTIONS } from "@/app/(workspace)/fulfillment/delivery-page-proof"
import { FulfillmentDirectForm } from "./forms/fulfillment-direct-form"
import { FulfillmentShipForm } from "./forms/fulfillment-ship-form"
import { FulfillmentResultPanel } from "@/features/fulfillment-operations/pages/components/fulfillment-result-panel"
import { FulfillmentWorkSurface } from "@/features/fulfillment-operations/pages/components/fulfillment-work-surface"
import {
    makeOperation,
    makePostedOutcome,
} from "@/features/fulfillment-operations/pages/hooks/test-data"
import {
    CORRECTION_NOTICE,
    OPERATION_ACTION_LABEL,
    OPERATION_CONFIRM_TITLE,
    OPERATION_DONE_LABEL,
    type FulfillmentDraft,
    type FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { impactPreview } from "@/features/fulfillment-operations/lib/validation"
import { deliveryActionsExcludeApproval } from "@/features/fulfillment-operations/lib/delivery-no-approval"

vi.mock("next/link", () => ({
    default: ({
        children,
        href,
        ...rest
    }: AnchorHTMLAttributes<HTMLAnchorElement> & {
        children?: ReactNode
        href: string
    }) => (
        <a href={href} {...rest}>
            {children}
        </a>
    ),
}))

afterEach(() => {
    cleanup()
})

function expectNoApprovalUi() {
    expect(screen.queryByText("审批流程")).toBeNull()
    expect(screen.queryByText("尚未绑定审批流程")).toBeNull()
    expect(screen.queryByText("当前审批人")).toBeNull()
    expect(screen.queryByRole("button", { name: "选择流程" })).toBeNull()
    expect(screen.queryByRole("button", { name: "通过" })).toBeNull()
    expect(screen.queryByRole("button", { name: "驳回" })).toBeNull()
    expect(screen.queryByRole("button", { name: "撤回审批" })).toBeNull()
    expect(screen.queryByRole("button", { name: "改派当前审批人" })).toBeNull()
    expect(screen.queryByRole("button", { name: "恢复当前审批人" })).toBeNull()
    expect(screen.queryByRole("button", { name: "取消受阻审批" })).toBeNull()
    expect(
        screen.queryByRole("button", { name: "更新审批流程版本" }),
    ).toBeNull()
    for (const label of FULFILLMENT_DELIVERY_FORBIDDEN_ACTIONS) {
        expect(screen.queryByRole("button", { name: label })).toBeNull()
    }
}

function makeShipOperation(): FulfillmentOperation {
    const base = makeOperation({
        operationId: "dl-1",
        operationType: "WAREHOUSE_SHIP",
    })
    const draft: Extract<FulfillmentDraft, { type: "WAREHOUSE_SHIP" }> = {
        type: "WAREHOUSE_SHIP",
        warehouseId: "wh_1",
        warehouseLabel: "中心仓",
        carrier: "顺丰",
        trackingNo: "SF-1",
        shippedAt: "2026-08-14T09:00",
        lines: [
            {
                salesOrderLineId: "sol_1",
                stockReservationId: "rsv_1",
                quantity: "10",
            },
        ],
    }
    return {
        ...base,
        lines: [
            {
                lineId: "line_1",
                salesOrderLineId: "sol_1",
                itemName: "演示商品",
                skuCode: "SKU-1",
                unitCode: "件",
                orderedQuantity: "10",
                remainingQuantity: "10",
                stockReservationId: "rsv_1",
                reservedQuantity: "10",
                availableOnHand: "20",
            },
        ],
        draft,
        summary: "待仓发 10 件",
        impact: "确认后扣减库存并消耗留货",
    }
}

function makeDirectOperation(): FulfillmentOperation {
    const base = makeOperation({
        operationId: "dl-2",
        operationType: "SUPPLIER_DIRECT",
    })
    const draft: Extract<FulfillmentDraft, { type: "SUPPLIER_DIRECT" }> = {
        type: "SUPPLIER_DIRECT",
        carrier: "顺丰",
        trackingNo: "SF-2",
        shippedAt: "2026-08-14T09:00",
        lines: [
            {
                salesOrderLineId: "sol_1",
                purchaseLineSalesAllocationId: "alloc_1",
                quantity: "4",
            },
        ],
    }
    return {
        ...base,
        lines: [
            {
                lineId: "line_1",
                salesOrderLineId: "sol_1",
                itemName: "演示商品",
                skuCode: "SKU-1",
                unitCode: "件",
                orderedQuantity: "4",
                remainingQuantity: "4",
                purchaseLineSalesAllocationId: "alloc_1",
            },
        ],
        draft,
        summary: "待直发 4 件",
        impact: "供应商直发不影响自有库存",
    }
}

describe("FulfillmentShipForm", () => {
    it("prints warehouse-ship facts and does not render the approval zone", () => {
        const operation = makeShipOperation()
        if (operation.draft.type !== "WAREHOUSE_SHIP") {
            throw new Error("fixture must be a WAREHOUSE_SHIP draft")
        }
        render(
            <FulfillmentShipForm
                operation={operation}
                draft={operation.draft}
                onChange={vi.fn()}
            />,
        )
        expect(screen.getByText("公司仓发")).toBeTruthy()
        expect(screen.getByText("发货仓")).toBeTruthy()
        expect(screen.getByDisplayValue("中心仓")).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("FulfillmentDirectForm", () => {
    it("prints supplier-direct facts and does not render the approval zone", () => {
        const operation = makeDirectOperation()
        if (operation.draft.type !== "SUPPLIER_DIRECT") {
            throw new Error("fixture must be a SUPPLIER_DIRECT draft")
        }
        render(
            <FulfillmentDirectForm
                operation={operation}
                draft={operation.draft}
                onChange={vi.fn()}
            />,
        )
        expect(screen.getByText("供应商直发")).toBeTruthy()
        expect(screen.getByText("承运方")).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("FulfillmentResultPanel delivery path", () => {
    it("shows posted delivery facts without process selection or decisions", () => {
        const outcome = makePostedOutcome({
            factType: "DELIVERY",
            factNo: "FH-2026-001",
            formalStatus: "SHIPPED",
            operationType: "WAREHOUSE_SHIP",
            acceptanceRequired: true,
            inventoryImpactSummary: "发货单已确认；库存与留货影响以库存台账为准。",
            reference: "FH-2026-001",
        })
        render(
            <FulfillmentResultPanel
                lastResult={{
                    status: "succeeded",
                    title: "已发货",
                    description: "仓发记录已确认",
                    reference: outcome.reference,
                    outcome,
                    stayOnItem: true,
                }}
                currentUrl="/fulfillment?type=warehouse_ship"
                onResolveUnknown={vi.fn()}
                onNext={vi.fn()}
            />,
        )
        expect(screen.getAllByText("FH-2026-001").length).toBeGreaterThan(0)
        expect(screen.getByText("发货")).toBeTruthy()
        expect(screen.getByRole("button", { name: "下一条" })).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("delivery detail and submit confirmation", () => {
    it("offers save and confirm-ship without approval actions", () => {
        const operation = makeShipOperation()
        if (operation.draft.type !== "WAREHOUSE_SHIP") {
            throw new Error("fixture must be a WAREHOUSE_SHIP draft")
        }
        render(
            <FulfillmentWorkSurface
                operation={operation}
                draft={operation.draft}
                validationIssues={[]}
                saveMessage={null}
                canExecute
                canPost
                formalPending={false}
                supportsSave
                dirty={false}
                autoNext={false}
                readOnlyNote=""
                responsibilityStatus="assigned_to_me"
                responsibilityStatusLabel="当前岗位可处理"
                currentUrl="/fulfillment?type=warehouse_ship"
                snapshotUpdatedAt="2026-08-14T10:00:00.000Z"
                position={1}
                total={1}
                shortcutsOpen={false}
                headingRef={{ current: null }}
                resultUnknown={false}
                onDraftChange={vi.fn()}
                onSkip={vi.fn()}
                onDiscard={vi.fn()}
                onSave={vi.fn()}
                onConfirm={vi.fn()}
                onBack={vi.fn()}
                onToggleShortcuts={vi.fn()}
            />,
        )
        expect(
            screen.getAllByRole("button", { name: "确认发货" }).length,
        ).toBeGreaterThan(0)
        expect(screen.getByRole("button", { name: "保存草稿" })).toBeTruthy()
        expectNoApprovalUi()
        expect(
            deliveryActionsExcludeApproval(
                operation.actionBlockers.map((blocker) => blocker.action),
            ),
        ).toBe(true)
    })

    it("confirms a warehouse delivery with the business dialog instead of an approval route", () => {
        const operation = makeShipOperation()
        render(
            <FormalActionConfirmDialog
                open
                onOpenChange={vi.fn()}
                title={OPERATION_CONFIRM_TITLE.WAREHOUSE_SHIP}
                description="没确认成功之前，库存和留货都不会动。"
                actionLabel={OPERATION_ACTION_LABEL.WAREHOUSE_SHIP}
                confirmLabel={OPERATION_ACTION_LABEL.WAREHOUSE_SHIP}
                fromStatus={{ label: "待确认", tone: "warning" }}
                toStatus={{
                    label: OPERATION_DONE_LABEL.WAREHOUSE_SHIP,
                    tone: "success",
                }}
                lockedFields={["来源单据、版本和留货", "单据类型"]}
                effects={impactPreview(operation, operation.draft)}
                irreversibleEffects={[CORRECTION_NOTICE]}
                nextDepartment="做完之后由销售登记客户验收"
                pending={false}
                onConfirm={vi.fn()}
            />,
        )
        expect(screen.getByText("确认发货？")).toBeTruthy()
        expect(screen.getByRole("button", { name: "确认发货" })).toBeTruthy()
        expectNoApprovalUi()
    })
})
