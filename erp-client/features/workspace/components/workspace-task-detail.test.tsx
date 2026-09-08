import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskDetail } from "./workspace-task-detail"

vi.mock("next/navigation", () => ({
    usePathname: () => "/workspace",
    useSearchParams: () => new URLSearchParams(),
}))
vi.mock("../hooks/use-workspace-document-facts", () => ({
    useWorkspaceDocumentFacts: (item: WorkspaceWorkItem) => ({
        facts: { sections: item.summarySections ?? [], lines: [] },
        isPending: false,
        isError: false,
    }),
}))
vi.mock("../hooks/use-workspace-source-sales-order", () => ({
    useWorkspaceSourceSalesOrder: () => ({ source: undefined }),
}))
vi.mock("@/features/approval-workflow/queries", () => ({
    useRecoveryOptionsQuery: () => ({ data: { actions: [] } }),
}))
vi.mock("@/features/approval-workflow/components/approval-action-bar", () => ({
    ApprovalActionBar: () => null,
}))
vi.mock("./workspace-document-paper-dialog", () => ({
    WorkspaceDocumentPaperDialog: ({
        target,
        open,
    }: {
        target: { kind: string; objectId: string } | null
        open: boolean
    }) =>
        open ? (
            <div role="dialog" aria-label="单据预览">
                {target?.kind}:{target?.objectId}
            </div>
        ) : null,
}))

afterEach(cleanup)

const item: WorkspaceWorkItem = {
    taskVersion: "1",
    subjectVersion: "1",
    status: "OPEN",
    processingState: "READY",
    priority: 3,
    createdAt: "2026-09-08T01:00:00Z",
    ownerRole: "sales",
    impactSummary: "",
    nextActionHint: "",
    destinationWorkspaceId: "W01",
    handlerKey: "approval",
    dueBucket: "later",
    workItemId: "instance-1",
    workItemType: "APPROVAL_INSTANCE",
    businessObjectType: "sales_order",
    businessObjectId: "so-1",
    stableNumber: "XS-1",
    objectTitle: "销售单审批 XS-1",
    ownerRoleLabel: "销售",
    ownerUserLabel: "销售员",
    ownerOrganizationLabel: "业务部",
    reasonLabel: "审批中",
    enteredAtLabel: "今天",
    dueAtLabel: "未设截止",
    workItemTypeLabel: "销售单审批",
    allowedActions: ["VIEW"],
    actionBlockers: [],
    statusLabel: "审批中",
    statusTone: "info",
    family: "approval",
    approvalProcessInstanceId: "instance-1",
    approval: {
        instanceId: "instance-1",
        status: "RUNNING",
        currentRoundNo: 1,
        currentNodeLabel: "采购确认",
        currentAssigneeLabel: "采购",
    },
    amountSummary: { label: "提交金额", value: "¥1,288.00", numeric: true },
    summarySections: [
        { label: "含税金额", value: "¥1,288.00", numeric: true },
        { label: "不含税金额", value: "¥1,200.00", numeric: true },
        { label: "税额", value: "¥88.00", numeric: true },
    ],
}

test("详情只展示单据金额，不展示提交金额", () => {
    render(<WorkspaceTaskDetail item={item} />)
    for (const label of ["含税金额", "不含税金额", "税额"]) {
        expect(screen.getByText(label)).toBeTruthy()
    }
    expect(screen.queryByText("提交金额")).toBeNull()
    expect(screen.getByText("¥1,200.00")).toBeTruthy()
    expect(screen.getByText("¥88.00")).toBeTruthy()
    expect(screen.getByRole("list", { name: "审批阶段" })).toBeTruthy()
    expect(
        document.querySelector('[aria-current="step"]')?.textContent,
    ).toContain("采购确认")
})

test("详情金额未知时不回退提交金额，单据金额读取后正常展示", () => {
    const zero = { label: "提交金额", value: "¥0.00", numeric: true as const }
    const view = render(
        <WorkspaceTaskDetail
            item={{ ...item, summarySections: [], amountSummary: zero }}
        />,
    )
    expect(screen.queryByText("¥0.00")).toBeNull()
    expect(screen.queryByText("提交金额")).toBeNull()
    view.rerender(
        <WorkspaceTaskDetail item={{ ...item, amountSummary: zero }} />,
    )
    expect(screen.queryByText("¥0.00")).toBeNull()
    expect(screen.queryByText("提交金额")).toBeNull()
    expect(screen.getByText("税额")).toBeTruthy()
    expect(screen.getByText("含税金额")).toBeTruthy()
})

test.each([
    ["APPROVAL_INSTANCE", "sales_order", "查看销售单"],
    ["DOCUMENT_APPROVAL", "purchase_order", "查看采购单"],
])(
    "%s 的查看单据入口位于标题栏并打开当前单据预览",
    (workItemType, businessObjectType, label) => {
        render(
            <WorkspaceTaskDetail
                item={{
                    ...item,
                    workItemType,
                    businessObjectType,
                    nextActionHint: "打开单据查看完整审批进度。",
                }}
            />,
        )
        const preview = screen.getByRole("button", { name: label })
        expect(
            preview.closest('[data-slot="workspace-task-header"]'),
        ).toBeTruthy()
        expect(preview.querySelector("svg.lucide-file-text")).toBeTruthy()
        expect(
            document.querySelector('[data-slot="workspace-task-footer"]')
                ?.textContent ?? "",
        ).not.toContain(label)
        expect(screen.queryByText("打开单据查看完整审批进度。")).toBeNull()
        if (workItemType === "APPROVAL_INSTANCE") {
            expect(
                document.querySelector('[data-slot="workspace-task-footer"]')
                    ?.textContent,
            ).toBe("")
        }
        fireEvent.click(preview)
        expect(
            screen.getByRole("dialog", { name: "单据预览" }).textContent,
        ).toBe(`${businessObjectType}:so-1`)
    },
)
