import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook, waitFor } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { SalesOrderDraftLineInput } from "@/features/sales-orders/types"
import { useSalesLineProcurementResponsibilities } from "./use-sales-line-procurement-responsibilities"

const resolveMock = vi.fn()

vi.mock("@/features/sales-orders/api/procurement-responsibility", () => ({
    resolveSalesLineProcurementResponsibilities: (
        lines: SalesOrderDraftLineInput[],
    ) => resolveMock(lines),
}))

function line(serviceRegion: string): SalesOrderDraftLineInput {
    return {
        rowKey: "row-1",
        name: "测试商品",
        sku: "sku-1",
        skuRevisionId: "sku-revision-1",
        serviceRegion,
        quantity: "1",
        unit: "件",
        unitPriceGross: "10",
        fulfillmentMode: "WAREHOUSE",
        dueDate: "2026-09-01",
        faceValue: "",
        giftRate: "",
        cardForm: "",
    }
}

function makeWrapper() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false, staleTime: 60_000 } },
    })
    return function Wrapper({ children }: { children: ReactNode }) {
        return (
            <QueryClientProvider client={client}>
                {children}
            </QueryClientProvider>
        )
    }
}

afterEach(() => resolveMock.mockReset())

describe("useSalesLineProcurementResponsibilities", () => {
    it("refetches and replaces the owner when only service region changes", async () => {
        resolveMock.mockImplementation(
            async (lines: SalesOrderDraftLineInput[]) => [
                {
                    rowKey: "row-1",
                    resolved: true,
                    ownerUserId:
                        lines[0]?.serviceRegion === "上海市"
                            ? "buyer-shanghai"
                            : "buyer-east",
                    ownerName:
                        lines[0]?.serviceRegion === "上海市"
                            ? "上海采购"
                            : "华东采购",
                },
            ],
        )
        const { result, rerender } = renderHook(
            ({ serviceRegion }: { serviceRegion: string }) =>
                useSalesLineProcurementResponsibilities({
                    nature: "physical_service",
                    lines: [line(serviceRegion)],
                }),
            {
                initialProps: { serviceRegion: "华东" },
                wrapper: makeWrapper(),
            },
        )

        await waitFor(() =>
            expect(result.current.byRowKey.get("row-1")?.ownerUserId).toBe(
                "buyer-east",
            ),
        )
        rerender({ serviceRegion: "上海市" })
        await waitFor(() =>
            expect(result.current.byRowKey.get("row-1")?.ownerUserId).toBe(
                "buyer-shanghai",
            ),
        )
        expect(resolveMock).toHaveBeenCalledTimes(2)
        expect(result.current.allResolved).toBe(true)
    })
})
