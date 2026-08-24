import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { PurchaseOrdersCreateDialog } from "@/features/purchase-orders/pages/purchase-orders-list-create-dialog"
import type { PurchaseCreationBasis } from "@/features/purchase-orders/types"

vi.mock("@/features/purchase-orders/api/purchase-orders", () => ({
    fetchCreationBases: vi.fn(),
}))

const basis: PurchaseCreationBasis = {
    basisId: "basis-1",
    workItemId: "work-item-1",
    salesOrderId: "so-1",
    salesOrderNo: "XS202608240001",
    customerName: "客户甲",
    salesOrderRevisionId: "sales-revision-1",
    supplierId: "supplier-1",
    supplierName: "供应商甲",
    purchaseType: "PHYSICAL",
    fulfillmentResponsibility: "WAREHOUSE",
    paymentTermCode: "POSTPAY_NET30",
    paymentTermLabel: "货到 30 天",
    lines: [
        {
            salesOrderLineId: "sales-line-1",
            salesOrderRevisionLineId: "sales-revision-line-1",
            itemName: "测试商品",
            salesQuantity: "10",
            coveredQuantity: "4",
            remainingQuantity: "6",
            maxCreateQuantity: "5",
            unit: "件",
            unitCostGross: "20",
            inputTaxRate: "0.13",
            expectedDeliveryDate: "2026-09-01",
            salesAllocationLabel: "销售明细 1",
        },
    ],
    estimatedGross: "100",
    consumed: false,
}

function renderDialog(
    onCreate = vi.fn(),
    selectedBasis: PurchaseCreationBasis = basis,
    createResult: {
        status: "succeeded" | "failed" | "unknown"
        title: string
        description: string
    } | null = null,
) {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    })
    render(
        <QueryClientProvider client={client}>
            <PurchaseOrdersCreateDialog
                open
                onOpenChange={vi.fn()}
                openBases={[selectedBasis]}
                basesPending={false}
                basesFailed={false}
                onRetryBases={vi.fn()}
                basisFromUrl={null}
                salesOrderFromUrl="so-1"
                selectedBasisId={selectedBasis.basisId}
                onSelectedBasisIdChange={vi.fn()}
                createPending={false}
                createResult={createResult}
                onCreate={onCreate}
            />
        </QueryClientProvider>,
    )
    return onCreate
}

afterEach(cleanup)

describe("PurchaseOrdersCreateDialog", () => {
    it("defaults each purchase quantity to the maximum and submits line identity", async () => {
        const onCreate = renderDialog()
        const input = await screen.findByTestId(
            "purchase-basis-line-quantity-sales-line-1",
        )
        expect((input as HTMLInputElement).value).toBe("5")
        expect(
            screen
                .getByTestId("purchase-basis-basis-1")
                .textContent?.includes("剩余数量"),
        ).toBe(true)

        fireEvent.click(screen.getByTestId("purchase-create-from-basis"))
        await waitFor(() =>
            expect(onCreate).toHaveBeenCalledWith([
                {
                    salesOrderLineId: "sales-line-1",
                    quantity: "5",
                },
            ]),
        )
    })

    it("rejects a quantity above the maximum", async () => {
        const onCreate = renderDialog()
        const input = await screen.findByTestId(
            "purchase-basis-line-quantity-sales-line-1",
        )
        fireEvent.change(input, { target: { value: "6" } })

        await waitFor(() =>
            expect(screen.getByText("本次采购数量不能超过 5")).toBeTruthy(),
        )
        expect(
            (
                screen.getByTestId(
                    "purchase-create-from-basis",
                ) as HTMLButtonElement
            ).disabled,
        ).toBe(true)
        expect(onCreate).not.toHaveBeenCalled()
    })

    it("submits only selected lines from a multi-line basis", async () => {
        const multiLineBasis: PurchaseCreationBasis = {
            ...basis,
            lines: [
                ...basis.lines,
                {
                    ...basis.lines[0],
                    salesOrderLineId: "sales-line-2",
                    salesOrderRevisionLineId: "sales-revision-line-2",
                    itemName: "测试商品二",
                    maxCreateQuantity: "2.5",
                    remainingQuantity: "2.5",
                },
            ],
        }
        const onCreate = renderDialog(vi.fn(), multiLineBasis)
        fireEvent.click(
            await screen.findByTestId(
                "purchase-basis-line-selected-sales-line-1",
            ),
        )
        const secondQuantity = screen.getByTestId(
            "purchase-basis-line-quantity-sales-line-2",
        ) as HTMLInputElement
        expect(secondQuantity.step).toBe("any")
        fireEvent.change(secondQuantity, { target: { value: "0.5" } })
        fireEvent.click(screen.getByTestId("purchase-create-from-basis"))

        await waitFor(() =>
            expect(onCreate).toHaveBeenCalledWith([
                {
                    salesOrderLineId: "sales-line-2",
                    quantity: "0.5",
                },
            ]),
        )
    })

    it("shows succeeded, failed and unknown command outcomes inside the open dialog", () => {
        const client = new QueryClient({
            defaultOptions: { queries: { retry: false } },
        })
        const dialog = (createResult: {
            status: "succeeded" | "failed" | "unknown"
            title: string
            description: string
        }) => (
            <QueryClientProvider client={client}>
                <PurchaseOrdersCreateDialog
                    open
                    onOpenChange={vi.fn()}
                    openBases={[basis]}
                    basesPending={false}
                    basesFailed={false}
                    onRetryBases={vi.fn()}
                    basisFromUrl={null}
                    salesOrderFromUrl="so-1"
                    selectedBasisId="basis-1"
                    onSelectedBasisIdChange={vi.fn()}
                    createPending={false}
                    createResult={createResult}
                    onCreate={vi.fn()}
                />
            </QueryClientProvider>
        )
        render(
            dialog({
                status: "succeeded",
                title: "已创建采购草稿",
                description: "该销售单仍有待采购数量，可继续建单。",
            }),
        )
        expect(
            screen.getByTestId("purchase-create-result").textContent,
        ).toContain("仍有待采购数量，可继续建单")

        cleanup()
        render(
            dialog({
                status: "failed",
                title: "建单失败",
                description: "创建依据已刷新",
            }),
        )
        expect(
            screen.getByTestId("purchase-create-result").textContent,
        ).toContain("创建依据已刷新")

        cleanup()
        render(
            dialog({
                status: "unknown",
                title: "建单结果待确认",
                description: "请使用同一操作重试",
            }),
        )
        expect(
            screen.getByTestId("purchase-create-result").textContent,
        ).toContain("请使用同一操作重试")
    })

    it("does not render or submit stale bases after the basis query fails", () => {
        const client = new QueryClient({
            defaultOptions: { queries: { retry: false } },
        })
        render(
            <QueryClientProvider client={client}>
                <PurchaseOrdersCreateDialog
                    open
                    onOpenChange={vi.fn()}
                    openBases={[]}
                    basesPending={false}
                    basesFailed
                    onRetryBases={vi.fn()}
                    basisFromUrl={null}
                    salesOrderFromUrl="so-1"
                    selectedBasisId="basis-1"
                    onSelectedBasisIdChange={vi.fn()}
                    createPending={false}
                    createResult={null}
                    onCreate={vi.fn()}
                />
            </QueryClientProvider>,
        )

        expect(screen.getByText("创建依据加载失败，请重试。")).toBeTruthy()
        expect(screen.queryByTestId("purchase-basis-basis-1")).toBeNull()
        expect(
            (
                screen.getByTestId(
                    "purchase-create-from-basis",
                ) as HTMLButtonElement
            ).disabled,
        ).toBe(true)
    })
})
