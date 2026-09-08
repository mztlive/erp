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
        facts: {
            sections: item.summarySections ?? [],
            lines: item.briefLines ?? [],
        },
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

vi.mock(
    "@/features/fulfillment-operations/pages/hooks/use-fulfillment-operations-controller",
    () => ({
        useFulfillmentOperationsController: () => ({
            queueQuery: { isPending: true, isError: false },
            context: undefined,
        }),
    }),
)

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

const registeredTasks = [
    ["PROCUREMENT_ORDER_CREATION", "sales_order"],
    ["FULFILLMENT_OPERATION", "purchase_receipt"],
    ["FULFILLMENT_OPERATION", "delivery"],
    ["FULFILLMENT_OPERATION", "electronic_delivery"],
    ["FULFILLMENT_OPERATION", "service_fulfillment"],
    ["CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order"],
    ["DOCUMENT_APPROVAL", "sales_order"],
    ["SUPPLIER_PAYMENT_EXECUTION", "payable_account"],
    ["SALES_INVOICE_EXECUTION", "receivable_account"],
    ["SUPPLIER_SETTLEMENT_REVIEW", "supplier_settlement_statement"],
    ["IMPORT_BUSINESS_CONFIRMATION", "LEGACY_IMPORT_BATCH"],
    ["INTEGRATION_RESULT_UNKNOWN", "integration_error_task"],
    ["BUSINESS_EXCEPTION", "integration_error_task"],
    ["BUSINESS_EXCEPTION", "reconciliation_difference"],
    ["INTEGRATION_RESULT_UNKNOWN", "reconciliation_difference"],
    ["INTEGRATION_RESULT_UNKNOWN", "SUPPLIER_FULFILLMENT_ORDER"],
    ["BUSINESS_EXCEPTION", "SUPPLIER_FULFILLMENT_ORDER"],
    ["BUSINESS_EXCEPTION", "SUPPLIER_OFFERING"],
    ["DOCUMENT_APPROVAL", "voucher_sales_order"],
    ["DOCUMENT_APPROVAL", "sales_change_order"],
    ["DOCUMENT_APPROVAL", "purchase_order"],
    ["DOCUMENT_APPROVAL", "purchase_change_order"],
    ["DOCUMENT_APPROVAL", "stock_adjustment"],
    ["DOCUMENT_APPROVAL", "customer_receipt"],
    ["DOCUMENT_APPROVAL", "customer_refund"],
    ["DOCUMENT_APPROVAL", "receipt_reversal"],
    ["DOCUMENT_APPROVAL", "supplier_refund"],
    ["DOCUMENT_APPROVAL", "payment_reversal"],
] as const

test.each(registeredTasks)(
    "%s / %s 的只读入口保留公共字段并隐藏执行控件",
    (workItemType, businessObjectType) => {
        render(
            <WorkspaceTaskDetail
                item={{
                    ...item,
                    workItemType,
                    businessObjectType,
                    approval: undefined,
                    approvalProcessInstanceId: undefined,
                    allowedActions: ["VIEW"],
                    counterpartyName: "测试往来方",
                    summarySections: [
                        ...item.summarySections!,
                        { label: "业务备注", value: "提交冻结内容" },
                    ],
                    documentSummaryResolved: true,
                }}
            />,
        )
        expect(screen.getByText("测试往来方")).toBeTruthy()
        expect(screen.getByText("含税金额")).toBeTruthy()
        expect(screen.getByText("不含税金额")).toBeTruthy()
        expect(screen.getByText("税额")).toBeTruthy()
        expect(screen.getByText("提交冻结内容")).toBeTruthy()
        expect(screen.queryByRole("textbox")).toBeNull()
        expect(screen.queryByRole("combobox")).toBeNull()
        expect(screen.queryByRole("spinbutton")).toBeNull()
        expect(
            screen.queryByRole("button", {
                name: /同意审批|预览供给分配|确认付款|确认开票|确认导入|重试|重放/,
            }),
        ).toBeNull()
    },
)

test("已解析但缺少历史摘要时明确提示，不显示提交金额冒充完整内容", () => {
    render(
        <WorkspaceTaskDetail
            item={{
                ...item,
                documentSummaryResolved: true,
                summarySections: [],
                briefLines: [],
            }}
        />,
    )
    expect(screen.getByRole("status").textContent).toContain(
        "此次提交的单据摘要暂不可用",
    )
    expect(screen.queryByText("提交金额")).toBeNull()
})

test("只读开票任务的文档图标只用于查看，不使用登记动作的文案和 URL", () => {
    render(
        <WorkspaceTaskDetail
            item={{
                ...item,
                workItemType: "SALES_INVOICE_EXECUTION",
                businessObjectType: "receivable_account",
                businessObjectId: "receivable-1",
                rootBusinessObjectId: "sales-1",
                queueContextId: "queue-1",
                handlerKey: "sales_invoice_execution",
                destinationWorkspaceId: "W11",
                approval: undefined,
                approvalProcessInstanceId: undefined,
            }}
        />,
    )
    const link = screen.getByRole("link", { name: "查看应收账户" })
    expect(link.querySelector("svg.lucide-file-text")).toBeTruthy()
    expect(link.getAttribute("href")).not.toContain("register=")
    expect(screen.queryByRole("link", { name: "去登记销项发票" })).toBeNull()
})

test.each([
    "purchase_receipt",
    "delivery",
    "electronic_delivery",
    "service_fulfillment",
])(
    "%s 管理员保留转交与公共字段，不进入执行者的空队列",
    (businessObjectType) => {
        render(
            <WorkspaceTaskDetail
                item={{
                    ...item,
                    workItemType: "FULFILLMENT_OPERATION",
                    businessObjectType,
                    handlerKey: "fulfillment_operation",
                    destinationWorkspaceId: "W01",
                    rootBusinessObjectId: "parent-1",
                    allowedActions: ["VIEW", "REASSIGN"],
                    approval: undefined,
                    approvalProcessInstanceId: undefined,
                }}
            />,
        )
        expect(screen.getByRole("button", { name: "转交责任" })).toBeTruthy()
        expect(screen.getByText("含税金额")).toBeTruthy()
        expect(screen.queryByText("本单当前没有待处理的履约单据")).toBeNull()
        expect(screen.queryByRole("spinbutton")).toBeNull()
    },
)

test.each([
    "purchase_receipt",
    "delivery",
    "electronic_delivery",
    "service_fulfillment",
])(
    "%s 的阅读人与处理人展示同一组履约事实，仅操作入口不同",
    (businessObjectType) => {
        const common: WorkspaceWorkItem = {
            ...item,
            workItemType: "FULFILLMENT_OPERATION",
            businessObjectType,
            handlerKey: "fulfillment_operation",
            rootBusinessObjectId: "source-1",
            approval: undefined,
            approvalProcessInstanceId: undefined,
            workItemTypeLabel: "履约处理",
            summarySections: [
                { label: "履约批次", value: "FUL-001" },
                { label: "履约状态", value: "待处理" },
                { label: "物流单号", value: "SF123" },
                { label: "完成说明", value: "已完成约定服务" },
            ],
            briefLines: [{ title: "龙井礼盒", quantity: "1 盒" }],
            allowedActions: ["VIEW"],
        }
        const view = render(<WorkspaceTaskDetail item={common} />)
        for (const text of [
            "FUL-001",
            "SF123",
            "已完成约定服务",
            "龙井礼盒",
            "1 盒",
        ])
            expect(screen.getByText(text)).toBeTruthy()
        expect(screen.queryByRole("button", { name: "处理履约" })).toBeNull()
        view.rerender(
            <WorkspaceTaskDetail
                item={{ ...common, allowedActions: ["VIEW", "PROCESS"] }}
            />,
        )
        for (const text of [
            "FUL-001",
            "SF123",
            "已完成约定服务",
            "龙井礼盒",
            "1 盒",
        ])
            expect(screen.getByText(text)).toBeTruthy()
        expect(screen.getByRole("button", { name: "处理履约" })).toBeTruthy()
        expect(screen.queryByRole("dialog")).toBeNull()
        expect(screen.queryByText("任务处理器未登记")).toBeNull()
        fireEvent.click(screen.getByRole("button", { name: "处理履约" }))
        expect(screen.getByRole("dialog", { name: "处理履约" })).toBeTruthy()
    },
)
