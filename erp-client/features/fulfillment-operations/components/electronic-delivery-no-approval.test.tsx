import type { AnchorHTMLAttributes, ReactNode } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { FormalActionConfirmDialog } from "@/components/business"
import { FULFILLMENT_ELECTRONIC_DELIVERY_FORBIDDEN_ACTIONS } from "@/app/(workspace)/fulfillment/electronic-delivery-page-proof"
import { FulfillmentElectronicForm } from "./forms/fulfillment-electronic-form"
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
import { electronicDeliveryActionsExcludeApproval } from "@/features/fulfillment-operations/lib/electronic-delivery-no-approval"

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
    for (const label of FULFILLMENT_ELECTRONIC_DELIVERY_FORBIDDEN_ACTIONS) {
        expect(screen.queryByRole("button", { name: label })).toBeNull()
    }
}

function makeElectronicOperation(): FulfillmentOperation {
    const base = makeOperation({
        operationId: "ed-1",
        operationType: "ELECTRONIC",
    })
    const draft: Extract<FulfillmentDraft, { type: "ELECTRONIC" }> = {
        type: "ELECTRONIC",
        occurredAt: "2026-08-14T09:00",
        recipientMasked: "138****0001",
        result: "SUCCESS",
        lines: [
            {
                salesOrderLineId: "sol_1",
                purchaseLineSalesAllocationId: "alloc_1",
                quantity: "5",
            },
        ],
    }
    return {
        ...base,
        lines: [
            {
                lineId: "line_1",
                salesOrderLineId: "sol_1",
                itemName: "演示卡密",
                skuCode: "SKU-E1",
                unitCode: "张",
                orderedQuantity: "5",
                remainingQuantity: "5",
                purchaseLineSalesAllocationId: "alloc_1",
            },
        ],
        draft,
        summary: "待电子交付 5 张",
        impact: "电子交付不影响自有库存",
    }
}

describe("FulfillmentElectronicForm", () => {
    it("prints electronic delivery facts and does not render the approval zone", () => {
        const operation = makeElectronicOperation()
        if (operation.draft.type !== "ELECTRONIC") {
            throw new Error("fixture must be an ELECTRONIC draft")
        }
        render(
            <FulfillmentElectronicForm
                operation={operation}
                draft={operation.draft}
                onChange={vi.fn()}
            />,
        )
        expect(screen.getByText("电子交付")).toBeTruthy()
        expect(screen.getByText("交付对象")).toBeTruthy()
        expect(screen.getByDisplayValue("138****0001")).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("FulfillmentResultPanel electronic delivery path", () => {
    it("shows posted electronic delivery facts without process selection or decisions", () => {
        const outcome = makePostedOutcome({
            factType: "ELECTRONIC_DELIVERY",
            factNo: "DZ-2026-001",
            formalStatus: "CONFIRMED",
            operationType: "ELECTRONIC",
            acceptanceRequired: true,
            inventoryImpactSummary: "不影响自有库存。",
            reference: "DZ-2026-001",
        })
        render(
            <FulfillmentResultPanel
                lastResult={{
                    status: "succeeded",
                    title: "已交付",
                    description: "电子交付已确认",
                    reference: outcome.reference,
                    outcome,
                    stayOnItem: true,
                }}
                currentUrl="/fulfillment?type=electronic"
                onResolveUnknown={vi.fn()}
                onNext={vi.fn()}
            />,
        )
        expect(screen.getAllByText("DZ-2026-001").length).toBeGreaterThan(0)
        expect(screen.getByText("电子交付")).toBeTruthy()
        expect(screen.getByRole("button", { name: "下一条" })).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("electronic delivery detail and submit confirmation", () => {
    it("offers confirm-delivery without approval actions", () => {
        const operation = makeElectronicOperation()
        if (operation.draft.type !== "ELECTRONIC") {
            throw new Error("fixture must be an ELECTRONIC draft")
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
                supportsSave={false}
                dirty={false}
                autoNext={false}
                readOnlyNote=""
                responsibilityStatus="assigned_to_me"
                responsibilityStatusLabel="当前岗位可处理"
                currentUrl="/fulfillment?type=electronic"
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
            screen.getAllByRole("button", { name: "确认交付" }).length,
        ).toBeGreaterThan(0)
        expect(
            (
                screen.getByRole("button", {
                    name: "保存草稿",
                }) as HTMLButtonElement
            ).disabled,
        ).toBe(true)
        expectNoApprovalUi()
        expect(
            electronicDeliveryActionsExcludeApproval(
                operation.actionBlockers.map((blocker) => blocker.action),
            ),
        ).toBe(true)
    })

    it("confirms an electronic delivery with the business dialog instead of an approval route", () => {
        const operation = makeElectronicOperation()
        render(
            <FormalActionConfirmDialog
                open
                onOpenChange={vi.fn()}
                title={OPERATION_CONFIRM_TITLE.ELECTRONIC}
                description="没确认成功之前，交付事实不会落账。"
                actionLabel={OPERATION_ACTION_LABEL.ELECTRONIC}
                confirmLabel={OPERATION_ACTION_LABEL.ELECTRONIC}
                fromStatus={{ label: "待确认", tone: "warning" }}
                toStatus={{
                    label: OPERATION_DONE_LABEL.ELECTRONIC,
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
        expect(screen.getByText("确认交付？")).toBeTruthy()
        expect(screen.getByRole("button", { name: "确认交付" })).toBeTruthy()
        expectNoApprovalUi()
    })
})
