import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { WorkspaceWorkItem } from "@/features/workspace/types"

import { WorkspaceTaskCard } from "./workspace-task-card"
import { WorkspaceTaskDetail } from "./workspace-task-detail"

vi.mock("@/features/approval-workflow/components/approval-action-bar", () => ({
    ApprovalActionBar: () => <button type="button">通过</button>,
}))
vi.mock("@/features/approval-workflow/queries", () => ({
    useRecoveryOptionsQuery: () => ({ data: { actions: [] } }),
}))
vi.mock("@/features/workspace/hooks/use-workspace-document-facts", () => ({
    useWorkspaceDocumentFacts: () => ({ facts: null, isPending: false }),
}))

/** 与现网销售单审批任务同形：简报带 10 个键值段。 */
function salesApprovalItem(
    overrides: Partial<WorkspaceWorkItem> = {},
): WorkspaceWorkItem {
    return {
        workItemId: "wi-1",
        taskVersion: "1",
        workItemType: "SALES_APPROVAL",
        workItemTypeLabel: "销售单审批",
        businessObjectType: "SalesOrder",
        businessObjectId: "SO-1",
        subjectVersion: "1",
        stableNumber: "销售单 XS20260823114925",
        objectTitle: "销售单 XS20260823114925",
        counterpartyName: "E2E客户56920203",
        listSummary: "E2E客户56920203 · ¥100 · 货到 15 天 · 奇乐融融A 1 盒",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "info",
        processingState: "READY",
        priority: 1,
        createdAt: "2026-08-23T03:49:00.000Z",
        ownerRoleLabel: "sales_order_approver",
        ownerOrganizationLabel: "华东",
        ownerUserLabel: "采购1",
        reasonLabel: "",
        impactSummary: "不审批则销售单不能生效、不能履约",
        nextActionHint: "进入对应页面后提交处理结论。",
        allowedActions: ["APPROVE", "REJECT"],
        actionBlockers: [],
        destinationWorkspaceId: "W05",
        handlerKey: "sales_order_approver",
        enteredAtLabel: "11:49",
        dueAtLabel: "",
        dueBucket: "later",
        family: "approval",
        approvalProcessInstanceId: "inst-1",
        summarySections: [
            { label: "客户", value: "E2E客户56920203" },
            { label: "业务性质", value: "实物及服务" },
            { label: "结算主体", value: "E2E客户56920203" },
            { label: "合同", value: "HT-7456920203" },
            { label: "含税金额", value: "¥100", numeric: true },
            { label: "不含税金额", value: "¥87", numeric: true },
            { label: "税额", value: "¥13", numeric: true },
            { label: "付款条件", value: "货到 15 天" },
            { label: "项目", value: "年节礼包" },
            { label: "提交人", value: "7e9e521afce041b79218edb9a246e974" },
        ],
        briefLines: [{ title: "奇乐融融A", quantity: "1 盒 · ¥100" }],
        briefMoreCount: 0,
        ...overrides,
    }
}

describe("WorkspaceTaskDetail", () => {
    afterEach(() => {
        cleanup()
    })

    it("shows the counterparty once instead of repeating the list summary", () => {
        render(<WorkspaceTaskDetail item={salesApprovalItem()} />)

        expect(screen.getAllByText(/E2E客户56920203/)).toHaveLength(1)
        expect(screen.queryByText(/货到 15 天 · 奇乐融融A/)).toBeNull()
    })

    it("lifts amounts out of the field grid", () => {
        render(<WorkspaceTaskDetail item={salesApprovalItem()} />)

        expect(screen.getByText("¥100")).toBeTruthy()
        expect(screen.getByText("¥87")).toBeTruthy()
        expect(screen.getByText("¥13")).toBeTruthy()
        expect(screen.getAllByText("含税金额")).toHaveLength(1)
    })

    it("keeps decision fields open and collapses the rest", () => {
        render(<WorkspaceTaskDetail item={salesApprovalItem()} />)

        expect(screen.getByText("业务性质")).toBeTruthy()
        expect(screen.getByText("付款条件")).toBeTruthy()
        expect(
            screen.getByRole("button", { name: /单据字段（3）/ }),
        ).toBeTruthy()
        expect(screen.queryByText("HT-7456920203")).toBeNull()
    })

    it("never puts a raw submitter id on screen", () => {
        render(<WorkspaceTaskDetail item={salesApprovalItem()} />)

        expect(
            screen.queryByText(/7e9e521afce041b79218edb9a246e974/),
        ).toBeNull()
        expect(screen.queryByText("提交人")).toBeNull()
    })

    it("does not render an empty approval-progress placeholder", () => {
        render(<WorkspaceTaskDetail item={salesApprovalItem()} />)

        expect(screen.queryByText("审批摘要")).toBeNull()
        expect(screen.queryByText("当前没有可展示的审批进度")).toBeNull()
    })

    it("labels the line section with the real row count", () => {
        render(
            <WorkspaceTaskDetail
                item={salesApprovalItem({ briefMoreCount: 2 })}
            />,
        )

        expect(screen.getByText("明细 · 3 行")).toBeTruthy()
        expect(screen.getByText(/另有 2 行/)).toBeTruthy()
    })

    it("opens procurement creation with stable task selectors and next-step copy", () => {
        const item = salesApprovalItem({
            workItemId: "wi-procurement-1",
            workItemType: "PROCUREMENT_ORDER_CREATION",
            workItemTypeLabel: "待采购建单",
            businessObjectId: "so-1",
            stableNumber: "销售单 XS202608240001",
            objectTitle: "销售单 XS202608240001",
            destinationWorkspaceId: "W08",
            handlerKey: "procurement_order_creation",
            allowedActions: ["PROCESS"],
            approvalProcessInstanceId: undefined,
            approvalNodeExecutionId: undefined,
            nextActionHint:
                "打开采购单页，按销售明细剩余数量选择本次采购数量并创建草稿。",
        })
        const { rerender } = render(
            <WorkspaceTaskCard item={item} selected onSelect={vi.fn()} />,
        )
        expect(
            screen.getByTestId(
                "work-item-procurement-order-creation-wi-procurement-1",
            ),
        ).toBeTruthy()

        rerender(<WorkspaceTaskDetail item={item} />)
        const link = screen.getByTestId(
            "work-item-open-document-wi-procurement-1",
        )
        expect(link.textContent?.includes("打开单据")).toBe(true)
        expect(link.getAttribute("href")).toBe(
            "/procurement/orders?action=create&salesOrderId=so-1&workItemId=wi-procurement-1",
        )
        expect(screen.getByText(/按销售明细剩余数量/)).toBeTruthy()
    })
})
