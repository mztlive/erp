import type { AnchorHTMLAttributes, ReactNode } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { FormalActionConfirmDialog } from "@/components/business"
import { FULFILLMENT_PURCHASE_RECEIPT_FORBIDDEN_ACTIONS } from "@/app/(workspace)/fulfillment/purchase-receipt-page-proof"
import { FulfillmentReceiptForm } from "./forms/fulfillment-receipt-form"
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
} from "@/features/fulfillment-operations/types"
import { impactPreview } from "@/features/fulfillment-operations/lib/validation"
import { purchaseReceiptActionsExcludeApproval } from "@/features/fulfillment-operations/lib/purchase-receipt-no-approval"

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
    for (const label of FULFILLMENT_PURCHASE_RECEIPT_FORBIDDEN_ACTIONS) {
        expect(screen.queryByRole("button", { name: label })).toBeNull()
    }
}

describe("FulfillmentReceiptForm", () => {
    it("prints receipt facts and does not render the approval zone", () => {
        const operation = makeOperation()
        if (operation.draft.type !== "RECEIPT") {
            throw new Error("fixture must be a RECEIPT draft")
        }
        render(
            <FulfillmentReceiptForm
                operation={operation}
                draft={operation.draft}
                onChange={vi.fn()}
            />,
        )
        expect(screen.getByText("入库作业")).toBeTruthy()
        expect(screen.getByText("入库仓")).toBeTruthy()
        expect(screen.getByDisplayValue("中心仓")).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("FulfillmentResultPanel purchase receipt path", () => {
    it("shows posted receipt facts without process selection or decisions", () => {
        const outcome = makePostedOutcome()
        render(
            <FulfillmentResultPanel
                lastResult={{
                    status: "succeeded",
                    title: "已入库",
                    description: "采购入库已确认",
                    reference: outcome.reference,
                    outcome,
                    stayOnItem: true,
                }}
                currentUrl="/fulfillment?type=receipt"
                onResolveUnknown={vi.fn()}
                onNext={vi.fn()}
            />,
        )
        expect(screen.getAllByText("RK-2026-001").length).toBeGreaterThan(0)
        expect(screen.getByText("采购入库")).toBeTruthy()
        expect(screen.getByRole("button", { name: "下一条" })).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("purchase receipt detail and submit confirmation", () => {
    it("offers save and confirm-receipt without approval actions", () => {
        const operation = makeOperation()
        if (operation.draft.type !== "RECEIPT") {
            throw new Error("fixture must be a RECEIPT draft")
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
                currentUrl="/fulfillment?type=receipt"
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
            screen.getAllByRole("button", { name: "确认入库" }).length,
        ).toBeGreaterThan(0)
        expect(screen.getByRole("button", { name: "保存草稿" })).toBeTruthy()
        expectNoApprovalUi()
        expect(
            purchaseReceiptActionsExcludeApproval(
                operation.actionBlockers.map((blocker) => blocker.action),
            ),
        ).toBe(true)
    })

    it("confirms a receipt with the business dialog instead of an approval route", () => {
        const operation = makeOperation()
        render(
            <FormalActionConfirmDialog
                open
                onOpenChange={vi.fn()}
                title={OPERATION_CONFIRM_TITLE.RECEIPT}
                description="没确认成功之前，库存和留货都不会动。"
                actionLabel={OPERATION_ACTION_LABEL.RECEIPT}
                confirmLabel={OPERATION_ACTION_LABEL.RECEIPT}
                fromStatus={{ label: "待确认", tone: "warning" }}
                toStatus={{
                    label: OPERATION_DONE_LABEL.RECEIPT,
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
        expect(screen.getByText("确认入库？")).toBeTruthy()
        expect(screen.getByRole("button", { name: "确认入库" })).toBeTruthy()
        expectNoApprovalUi()
    })
})
