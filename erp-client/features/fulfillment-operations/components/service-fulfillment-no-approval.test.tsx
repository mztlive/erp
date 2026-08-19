import type { AnchorHTMLAttributes, ReactNode } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { FormalActionConfirmDialog } from "@/components/business"
import { FULFILLMENT_SERVICE_FULFILLMENT_FORBIDDEN_ACTIONS } from "@/app/(workspace)/fulfillment/service-fulfillment-page-proof"
import { FulfillmentServiceForm } from "./forms/fulfillment-service-form"
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
import { serviceFulfillmentActionsExcludeApproval } from "@/features/fulfillment-operations/lib/service-fulfillment-no-approval"

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
    for (const label of FULFILLMENT_SERVICE_FULFILLMENT_FORBIDDEN_ACTIONS) {
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

describe("FulfillmentServiceForm", () => {
    it("prints service fulfillment facts and does not render the approval zone", () => {
        const operation = makeServiceOperation()
        if (operation.draft.type !== "SERVICE") {
            throw new Error("fixture must be a SERVICE draft")
        }
        render(
            <FulfillmentServiceForm
                operation={operation}
                draft={operation.draft}
                onChange={vi.fn()}
            />,
        )
        expect(screen.getByText("线下服务")).toBeTruthy()
        expect(screen.getByText("服务地点")).toBeTruthy()
        expect(screen.getByDisplayValue("客户现场")).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("FulfillmentResultPanel service fulfillment path", () => {
    it("shows posted service fulfillment facts without process selection or decisions", () => {
        const outcome = makePostedOutcome({
            factType: "SERVICE_FULFILLMENT",
            factNo: "SF-2026-001",
            formalStatus: "CONFIRMED",
            operationType: "SERVICE",
            acceptanceRequired: true,
            inventoryImpactSummary: "不影响自有库存。",
            reference: "SF-2026-001",
        })
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
        expect(screen.getByText("服务履约")).toBeTruthy()
        expect(screen.getByRole("button", { name: "下一条" })).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("service fulfillment detail and submit confirmation", () => {
    it("offers confirm-completion without approval actions", () => {
        const operation = makeServiceOperation()
        if (operation.draft.type !== "SERVICE") {
            throw new Error("fixture must be a SERVICE draft")
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
                currentUrl="/fulfillment?type=service"
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
            screen.getAllByRole("button", { name: "确认完成" }).length,
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
            serviceFulfillmentActionsExcludeApproval(
                operation.actionBlockers.map((blocker) => blocker.action),
            ),
        ).toBe(true)
    })

    it("confirms a service fulfillment with the business dialog instead of an approval route", () => {
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
        expectNoApprovalUi()
    })
})
