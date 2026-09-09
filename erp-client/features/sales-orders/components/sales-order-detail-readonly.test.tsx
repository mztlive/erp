import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { cleanup, render, screen, within } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { AcceptancePanel } from "./sales-order-detail-acceptance-panel"
import { ReceivablePanel } from "./sales-order-detail-receivable-panel"
import { fetchCustomerAcceptanceWorkspace } from "../api/acceptance"
import { useSalesOrderReceivable } from "../hooks/use-sales-order-receivable"
import type { SalesOrderDetailView } from "../api/sales-orders"
import type { CustomerAcceptanceWorkspaceView } from "../lib/acceptance-types"

vi.mock("../api/acceptance", () => ({
    fetchCustomerAcceptanceWorkspace: vi.fn(),
}))
vi.mock("../hooks/use-sales-order-receivable", () => ({
    useSalesOrderReceivable: vi.fn(),
}))
const order = {
    id: "so-1",
    nature: "physical_service",
    currentRevisionNo: 1,
    receivedAmount: "80.00",
    invoicedAmount: "0.00",
    customerId: "c",
    settlementEntityId: "p",
} as unknown as SalesOrderDetailView
const success = (data: unknown) => ({
    data,
    isSuccess: true,
    isLoading: false,
    error: null,
    refetch: vi.fn(),
})
const finance = () => ({
    profile: { isPending: false, isError: false },
    canRead: () => true,
    accounts: success([
        {
            accountId: "a",
            accountSeq: 1,
            counterpartyPartyName: "主体甲",
            dueDate: "2026-09-01",
            statusLabel: "未结",
            statusTone: "info",
            openTotal: "120.00",
            settledTotal: "80.00",
            invoicedTotal: "0.00",
            openInvoiceableTotal: "200.00",
            entries: [{ entryId: "e" }],
        },
    ]),
    receipts: success({
        total: 1,
        items: [
            {
                receiptId: "r",
                receiptNo: "HK-1",
                receivedAt: "2026-09-09T00:00:00Z",
                status: "in_approval",
                statusLabel: "审批中",
                statusTone: "info",
                amount: "500.00",
                approval: { instance: { currentAssigneeName: "财务甲" } },
                allocations: [
                    {
                        targetId: "e",
                        amountGross: "80.00",
                        action: "APPLY",
                        isPosted: true,
                    },
                ],
                pendingAllocations: [
                    { targetId: "e", amountGross: "120.00" },
                    { targetId: "other", amountGross: "300.00" },
                ],
            },
        ],
    }),
    invoices: success({ total: 0, items: [] }),
    receiptPage: 1,
    invoicePage: 1,
    setReceiptPage: vi.fn(),
    setInvoicePage: vi.fn(),
    setPreview: vi.fn(),
    preview: null,
    detail: { isPending: false },
})
function renderWithQuery(element: React.ReactElement) {
    const client = new QueryClient({
        defaultOptions: {
            queries: { retry: false },
            mutations: { retry: false },
        },
    })
    render(<QueryClientProvider client={client}>{element}</QueryClientProvider>)
    return client
}
beforeEach(() => vi.clearAllMocks())
afterEach(cleanup)

it("详情即使收到可登记、可冲正权限与旧登记 URL，也只显示验收事实", async () => {
    window.history.replaceState(
        {},
        "",
        "/sales/orders/so-1?section=acceptance&mode=register",
    )
    vi.mocked(fetchCustomerAcceptanceWorkspace).mockResolvedValue({
        salesLines: [],
        history: [
            {
                acceptanceId: "acc",
                acceptanceNo: "YS-1",
                acceptedAt: "2026-09-01",
                postedAt: "2026-09-01",
                status: "POSTED",
                overallResult: "PASS",
                lines: [],
                recordedBy: "张三",
                version: 1,
                factOnlyNotice: "仅记录事实",
            },
        ],
        permissions: {
            allowedActions: ["REGISTER_ACCEPTANCE", "REVERSE_ACCEPTANCE"],
            actionBlockers: [],
            fieldVisibility: {},
        },
    } as unknown as CustomerAcceptanceWorkspaceView)
    const client = renderWithQuery(<AcceptancePanel order={order} />)
    expect(await screen.findByText("YS-1")).toBeTruthy()
    expect(screen.queryByRole("button", { name: /登记|冲正/ })).toBeNull()
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(fetchCustomerAcceptanceWorkspace).toHaveBeenCalledWith({
        salesOrderId: "so-1",
    })
    expect(client.getMutationCache().getAll()).toHaveLength(0)
})

it("有全部票款权限时仍只读，并区分单据金额、本单已核销和本单拟核销", () => {
    vi.mocked(useSalesOrderReceivable).mockReturnValue(
        finance() as unknown as ReturnType<typeof useSalesOrderReceivable>,
    )
    renderWithQuery(<ReceivablePanel order={order} />)
    const row = screen.getByText("HK-1").closest("tr")!
    expect(within(row).getByText("¥500.00")).toBeTruthy()
    expect(within(row).getByText("¥80.00")).toBeTruthy()
    expect(within(row).getByText("¥120.00")).toBeTruthy()
    expect(within(row).queryByText("¥420.00")).toBeNull()
    expect(within(row).getByText("财务甲")).toBeTruthy()
    expect(
        screen.queryByRole("button", { name: /^(登记|核销|冲正|通过|驳回)/ }),
    ).toBeNull()
})

it("回款失败保留失败提示，其他记录继续显示；不伪装成空记录", () => {
    const state = finance()
    state.receipts = {
        ...state.receipts,
        data: undefined,
        error: new Error("回款读取失败"),
    } as unknown as typeof state.receipts
    vi.mocked(useSalesOrderReceivable).mockReturnValue(
        state as unknown as ReturnType<typeof useSalesOrderReceivable>,
    )
    renderWithQuery(<ReceivablePanel order={order} />)
    expect(screen.getByText("记录加载失败")).toBeTruthy()
    expect(screen.queryByText("暂无回款记录")).toBeNull()
    expect(screen.getByText("暂无发票记录")).toBeTruthy()
})

it("没有明细权限时隐藏缓存记录，保留销售单摘要且明确权限限制", () => {
    const state = finance()
    state.canRead = () => false
    vi.mocked(useSalesOrderReceivable).mockReturnValue(
        state as unknown as ReturnType<typeof useSalesOrderReceivable>,
    )
    renderWithQuery(<ReceivablePanel order={order} />)
    expect(screen.queryByText("HK-1")).toBeNull()
    expect(screen.getAllByText(/当前账号无此类明细查看权限/)).toHaveLength(3)
    expect(screen.getByText("¥80.00")).toBeTruthy()
    expect(screen.getAllByText("待确认")).toHaveLength(2)
})
