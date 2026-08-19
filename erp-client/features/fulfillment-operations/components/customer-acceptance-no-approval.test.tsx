import type { AnchorHTMLAttributes, ReactNode } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { FormalActionConfirmDialog } from "@/components/business"
import { FULFILLMENT_CUSTOMER_ACCEPTANCE_FORBIDDEN_ACTIONS } from "@/app/(workspace)/fulfillment/customer-acceptance-page-proof"
import { FulfillmentResultPanel } from "@/features/fulfillment-operations/pages/components/fulfillment-result-panel"
import {
    makeOperation,
    makePostedOutcome,
} from "@/features/fulfillment-operations/pages/hooks/test-data"
import {
    CORRECTION_NOTICE,
    NOT_ACCEPTANCE_NOTICE,
    OPERATION_ACTION_LABEL,
    OPERATION_CONFIRM_TITLE,
    OPERATION_DONE_LABEL,
    type FulfillmentDraft,
    type FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { impactPreview } from "@/features/fulfillment-operations/lib/validation"
import {
    customerAcceptanceActionsExcludeApproval,
    isCustomerAcceptanceHandoff,
} from "@/features/fulfillment-operations/lib/customer-acceptance-no-approval"
import { acceptanceHref } from "@/features/fulfillment-operations/pages/lib/gate-copy"

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
    for (const label of FULFILLMENT_CUSTOMER_ACCEPTANCE_FORBIDDEN_ACTIONS) {
        expect(screen.queryByRole("button", { name: label })).toBeNull()
    }
}

function makeServiceOperation(): FulfillmentOperation {
    const base = makeOperation({
        operationId: "sf-1",
        operationType: "SERVICE",
    })
    const draft: Extract<FulfillmentDraft, { type: "SERVICE" }> = {
        type: "SERVICE",
        startedAt: "2026-08-14T09:00",
        endedAt: "2026-08-14T11:00",
        serviceLocation: "客户现场",
        result: "SUCCESS",
        completionNote: "已完成安装",
        lines: [
            {
                salesOrderLineId: "sol_1",
                purchaseLineSalesAllocationId: "alloc_1",
                quantity: "2",
            },
        ],
    }
    return {
        ...base,
        lines: [
            {
                lineId: "line_1",
                salesOrderLineId: "sol_1",
                itemName: "上门安装",
                skuCode: "SKU-S1",
                unitCode: "次",
                orderedQuantity: "2",
                remainingQuantity: "2",
                purchaseLineSalesAllocationId: "alloc_1",
            },
        ],
        draft,
        summary: "待服务履约 2 次",
        impact: "服务履约不影响自有库存",
    }
}

describe("FulfillmentResultPanel customer acceptance handoff", () => {
    it("shows the acceptance next-step without process selection or decisions", () => {
        const outcome = makePostedOutcome({
            factType: "SERVICE_FULFILLMENT",
            factNo: "SF-2026-001",
            formalStatus: "CONFIRMED",
            operationType: "SERVICE",
            acceptanceRequired: true,
            acceptanceNextStep: "服务履约已确认。请销售在客户验收登记。",
            inventoryImpactSummary: "不影响自有库存。",
            reference: "SF-2026-001",
            salesOrderId: "so_1",
        })
        expect(isCustomerAcceptanceHandoff(outcome)).toBe(true)
        render(
            <FulfillmentResultPanel
                lastResult={{
                    status: "succeeded",
                    title: "已完成",
                    description: "服务履约已确认",
                    reference: outcome.reference,
                    outcome,
                    stayOnItem: true,
                }}
                currentUrl="/fulfillment?type=service"
                onResolveUnknown={vi.fn()}
                onNext={vi.fn()}
            />,
        )
        expect(screen.getAllByText("SF-2026-001").length).toBeGreaterThan(0)
        expect(screen.getByText(NOT_ACCEPTANCE_NOTICE)).toBeTruthy()
        const acceptanceLink = screen.getByRole("button", {
            name: "去登记客户验收",
        })
        expect(acceptanceLink.getAttribute("href")).toBe(
            acceptanceHref("so_1", "/fulfillment?type=service"),
        )
        expect(acceptanceLink.getAttribute("href")).toContain(
            "section=acceptance",
        )
        expect(acceptanceLink.getAttribute("href")).not.toContain("approval")
        expect(screen.getByRole("button", { name: "下一条" })).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("customer acceptance detail and submit confirmation", () => {
    it("offers the business confirm dialog instead of an approval route", () => {
        const operation = makeServiceOperation()
        render(
            <FormalActionConfirmDialog
                open
                onOpenChange={vi.fn()}
                title={OPERATION_CONFIRM_TITLE.SERVICE}
                description="没确认成功之前，服务履约事实不会落账。"
                actionLabel={OPERATION_ACTION_LABEL.SERVICE}
                confirmLabel={OPERATION_ACTION_LABEL.SERVICE}
                fromStatus={{ label: "待确认", tone: "warning" }}
                toStatus={{
                    label: OPERATION_DONE_LABEL.SERVICE,
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
        expect(screen.getByText("确认服务完成？")).toBeTruthy()
        expect(screen.getByRole("button", { name: "确认完成" })).toBeTruthy()
        expect(screen.getByText("做完之后由销售登记客户验收")).toBeTruthy()
        expectNoApprovalUi()
        expect(
            customerAcceptanceActionsExcludeApproval([
                "CREATE_ACCEPTANCE",
                "POST_ACCEPTANCE",
            ]),
        ).toBe(true)
    })
})
