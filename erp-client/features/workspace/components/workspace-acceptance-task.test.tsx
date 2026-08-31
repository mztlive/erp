import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { QueryClientProvider } from "@tanstack/react-query"
import { afterEach, expect, test, vi } from "vitest"

import { createFreshQueryClient } from "@/features/test-utils"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceAcceptanceTask } from "./workspace-acceptance-task"

const navigation = vi.hoisted(() => ({
    pathname: "/workspace",
    searchParams: new URLSearchParams("currentWorkItemId=wi-acc-1"),
}))

vi.mock("next/navigation", () => ({
    usePathname: () => navigation.pathname,
    useSearchParams: () => navigation.searchParams,
}))

vi.mock("@/features/sales-orders/components/acceptance-workspace", () => ({
    AcceptanceWorkspace: () => <div>验收作业面</div>,
}))

vi.mock("./workspace-document-paper-dialog", () => ({
    WorkspaceDocumentPaperDialog: ({
        open,
        target,
    }: {
        open: boolean
        target: { objectId: string } | null
    }) =>
        open && target ? (
            <div data-testid="sales-order-paper">{target.objectId}</div>
        ) : null,
}))

afterEach(cleanup)

function sampleItem(
    overrides: Partial<WorkspaceWorkItem> = {},
): WorkspaceWorkItem {
    return {
        workItemId: "wi-acc-1",
        taskVersion: "1",
        workItemType: "CUSTOMER_ACCEPTANCE_REGISTRATION",
        workItemTypeLabel: "客户验收登记",
        businessObjectType: "sales_order",
        businessObjectId: "so-7",
        subjectVersion: "1",
        stableNumber: "销售单 XS20260831101354",
        objectTitle: "销售单 XS20260831101354",
        counterpartyName: "华东纸业",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "info",
        processingState: "READY",
        priority: 3,
        createdAt: "2026-08-31T10:13:00.000Z",
        ownerRole: "sales_order_owner",
        ownerRoleLabel: "销售单负责人",
        ownerOrganizationLabel: "责任组织",
        ownerUserLabel: "销售1",
        reasonLabel: "客户待验收",
        reasonCode: "CUSTOMER_ACCEPTANCE_REQUIRED",
        impactSummary: "不验收则销售单不能完结",
        nextActionHint: "核对本页事实后登记客户验收。",
        allowedActions: ["PROCESS"],
        actionBlockers: [],
        destinationWorkspaceId: "W06",
        handlerKey: "customer_acceptance_registration",
        enteredAtLabel: "8/31 10:13",
        dueAtLabel: "",
        dueBucket: "later",
        family: "fulfillment",
        ...overrides,
    }
}

function renderTask(item: WorkspaceWorkItem) {
    const client = createFreshQueryClient()
    return render(
        <QueryClientProvider client={client}>
            <WorkspaceAcceptanceTask item={item} />
        </QueryClientProvider>,
    )
}

test("验收任务标题栏露出销售单预览和跳转", () => {
    renderTask(sampleItem())

    const preview = screen.getByRole("button", { name: "查看销售单" })
    expect(preview.getAttribute("data-testid")).toBe(
        "work-item-read-sales-order-wi-acc-1",
    )

    const open = screen.getByRole("link", { name: "打开销售单" })
    expect(open.getAttribute("href")).toBe(
        "/sales/orders/so-7?from=workspace&returnTo=%2Fworkspace%3FcurrentWorkItemId%3Dwi-acc-1",
    )
    expect(open.getAttribute("data-testid")).toBe(
        "work-item-open-sales-order-wi-acc-1",
    )

    const help = screen.getByRole("button", { name: "任务说明" })
    expect(
        preview.compareDocumentPosition(open) &
            Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0)
    expect(
        open.compareDocumentPosition(help) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0)
})

test("点击预览后打开当前销售单纸质件", () => {
    renderTask(sampleItem())

    expect(screen.queryByTestId("sales-order-paper")).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "查看销售单" }))
    expect(screen.getByTestId("sales-order-paper").textContent).toBe("so-7")
})

test("身份不一致时不提供销售单预览和跳转", () => {
    renderTask(
        sampleItem({
            ownerRole: "role-finance",
        }),
    )

    expect(screen.queryByRole("button", { name: "查看销售单" })).toBeNull()
    expect(screen.queryByRole("link", { name: "打开销售单" })).toBeNull()
    expect(screen.getByText("任务责任与验收对象不一致")).toBeTruthy()
})

test("没有处理资格时仍可预览和打开销售单", () => {
    renderTask(
        sampleItem({
            allowedActions: [],
            actionBlockers: [
                {
                    action: "PROCESS",
                    code: "NOT_OWNER",
                    message: "当前账号没有处理此验收任务的资格。",
                },
            ],
        }),
    )

    expect(screen.getByText("当前无法登记客户验收")).toBeTruthy()
    expect(screen.getByRole("button", { name: "查看销售单" })).toBeTruthy()
    expect(screen.getByRole("link", { name: "打开销售单" })).toBeTruthy()
})
