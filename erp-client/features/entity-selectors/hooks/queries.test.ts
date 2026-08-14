import { act, renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { renderHookWithProviders } from "@/features/test-utils"

const apiMocks = vi.hoisted(() => ({
    searchSuppliers: vi.fn(),
    fetchSupplierOption: vi.fn(),
    searchCustomers: vi.fn(),
    fetchCustomerOption: vi.fn(),
    searchParties: vi.fn(),
    fetchPartyOption: vi.fn(),
    searchWarehouses: vi.fn(),
    fetchWarehouseOption: vi.fn(),
    searchContracts: vi.fn(),
    fetchContractOption: vi.fn(),
    searchSellableSkus: vi.fn(),
    searchCompanySkus: vi.fn(),
    fetchMallOptions: vi.fn(),
}))

vi.mock("@/features/entity-selectors/api/index", () => apiMocks)

const salesOrderMocks = vi.hoisted(() => ({
    fetchSalesOrders: vi.fn(),
    fetchSalesOrderDetail: vi.fn(),
}))

vi.mock("@/features/sales-orders/api", () => salesOrderMocks)

const masterDataMocks = vi.hoisted(() => ({
    fetchMasterDataList: vi.fn(),
}))

vi.mock("@/features/master-data/api", () => masterDataMocks)

import {
    entitySelectorKeys,
    useCompanySkuSelectorQuery,
    useContractSelectorQuery,
    useCustomerSelectorQuery,
    useDebouncedSearch,
    useMallSelectorQuery,
    usePartySelectorQuery,
    useSalesOrderSelectorQuery,
    useSellableSkuSelectorQuery,
    useSupplierSelectorQuery,
    useVoucherCategorySelectorQuery,
    useWarehouseSelectorQuery,
} from "@/features/entity-selectors/hooks/queries"

const cannedSupplier = {
    supplierId: "sup-1",
    supplierCode: "S001",
    supplierName: "示例供应商",
    statusLabel: "启用",
    statusTone: "success",
}

const cannedCustomer = {
    id: "cus-1",
    customerNo: "C001",
    legalName: "示例客户",
    statusLabel: "启用",
    statusTone: "success",
}

const cannedParty = {
    partyId: "p-1",
    partyCode: "P001",
    displayName: "示例主体",
    statusLabel: "启用",
    statusTone: "success",
}

const cannedWarehouse = {
    warehouseId: "wh-1",
    warehouseCode: "WH001",
    warehouseName: "示例仓库",
    statusLabel: "启用",
    statusTone: "success",
}

const cannedContract = {
    contractId: "c-1",
    contractNo: "CT-001",
    customerName: "示例客户",
    statusLabel: "生效中",
    statusTone: "success",
}

const cannedMall = { id: "mall-1", code: "MALL001", name: "示例商城" }

beforeEach(() => {
    for (const mock of Object.values(apiMocks)) mock.mockReset()
    salesOrderMocks.fetchSalesOrders.mockReset()
    salesOrderMocks.fetchSalesOrderDetail.mockReset()
    masterDataMocks.fetchMasterDataList.mockReset()
})

describe("entitySelectorKeys", () => {
    it("builds stable resource-scoped keys from the input", () => {
        const input = { query: "a", purpose: "form" } as const
        expect(entitySelectorKeys.supplier(input)).toEqual([
            "entity-selectors",
            "supplier",
            input,
        ])
        expect(entitySelectorKeys.supplier(input)).toEqual(
            entitySelectorKeys.supplier({ ...input }),
        )
        expect(entitySelectorKeys.customerDetail("cus-1")).toEqual([
            "entity-selectors",
            "customer",
            "detail",
            "cus-1",
        ])
        expect(entitySelectorKeys.contractDetail("c-1")).toEqual([
            "entity-selectors",
            "contract",
            "detail",
            "c-1",
            undefined,
        ])
        expect(entitySelectorKeys.contractDetail("c-1", "assigned")).toEqual([
            "entity-selectors",
            "contract",
            "detail",
            "c-1",
            "assigned",
        ])
        expect(entitySelectorKeys.malls("filter")).toEqual([
            "entity-selectors",
            "mall",
            { purpose: "filter" },
        ])
        expect(entitySelectorKeys.voucherCategories("form")).toEqual([
            "entity-selectors",
            "voucher-category",
            { purpose: "form" },
        ])
    })
})

describe("useDebouncedSearch", () => {
    it("only publishes the trimmed value after the delay", () => {
        vi.useFakeTimers()
        try {
            const { result, rerender } = renderHook(
                ({ value }: { value: string }) => useDebouncedSearch(value),
                { initialProps: { value: "" } },
            )
            rerender({ value: "  胶水 " })
            expect(result.current).toBe("")

            act(() => {
                vi.advanceTimersByTime(250)
            })
            expect(result.current).toBe("胶水")
        } finally {
            vi.useRealTimers()
        }
    })

    it("ignores intermediate values when typing faster than the delay", () => {
        vi.useFakeTimers()
        try {
            const { result, rerender } = renderHook(
                ({ value }: { value: string }) => useDebouncedSearch(value),
                { initialProps: { value: "" } },
            )
            rerender({ value: "a" })
            act(() => {
                vi.advanceTimersByTime(100)
            })
            rerender({ value: "ab" })
            act(() => {
                vi.advanceTimersByTime(100)
            })
            expect(result.current).toBe("")

            act(() => {
                vi.advanceTimersByTime(150)
            })
            expect(result.current).toBe("ab")
        } finally {
            vi.useRealTimers()
        }
    })
})

describe("useSupplierSelectorQuery", () => {
    const input = { query: "胶", purpose: "form" } as const

    it("lists suppliers through searchSuppliers with the given input", async () => {
        apiMocks.searchSuppliers.mockResolvedValue([cannedSupplier])
        const { result } = renderHookWithProviders(() =>
            useSupplierSelectorQuery(input),
        )
        expect(result.current.list.isPending).toBe(true)
        await waitFor(() =>
            expect(result.current.list.data).toEqual([cannedSupplier]),
        )
        expect(apiMocks.searchSuppliers).toHaveBeenCalledTimes(1)
        expect(apiMocks.searchSuppliers).toHaveBeenCalledWith(input)
    })

    it("fetches the selected supplier detail only when an id is given", async () => {
        apiMocks.searchSuppliers.mockResolvedValue([])
        apiMocks.fetchSupplierOption.mockResolvedValue(cannedSupplier)
        const { result } = renderHookWithProviders(() =>
            useSupplierSelectorQuery(input, "sup-1"),
        )
        await waitFor(() =>
            expect(result.current.selected.data).toEqual(cannedSupplier),
        )
        expect(apiMocks.fetchSupplierOption).toHaveBeenCalledWith("sup-1")
    })

    it("skips the detail query without an id", async () => {
        apiMocks.searchSuppliers.mockResolvedValue([])
        const { result } = renderHookWithProviders(() =>
            useSupplierSelectorQuery(input),
        )
        await waitFor(() => expect(result.current.list.isSuccess).toBe(true))
        expect(apiMocks.fetchSupplierOption).not.toHaveBeenCalled()
    })

    it("propagates list errors to the consumer", async () => {
        apiMocks.searchSuppliers.mockRejectedValue(new Error("boom"))
        const { result } = renderHookWithProviders(() =>
            useSupplierSelectorQuery(input),
        )
        await waitFor(() => expect(result.current.list.isError).toBe(true))
        expect(result.current.list.error).toEqual(new Error("boom"))
    })
})

describe("useCustomerSelectorQuery", () => {
    it("passes scope along to searchCustomers", async () => {
        apiMocks.searchCustomers.mockResolvedValue([cannedCustomer])
        const input = {
            query: "客",
            purpose: "form",
            scope: "all_authorized",
        } as const
        const { result } = renderHookWithProviders(() =>
            useCustomerSelectorQuery(input),
        )
        await waitFor(() =>
            expect(result.current.list.data).toEqual([cannedCustomer]),
        )
        expect(apiMocks.searchCustomers).toHaveBeenCalledWith(input)
    })

    it("fetches the selected customer detail by id", async () => {
        apiMocks.searchCustomers.mockResolvedValue([])
        apiMocks.fetchCustomerOption.mockResolvedValue(cannedCustomer)
        const { result } = renderHookWithProviders(() =>
            useCustomerSelectorQuery(
                { query: "", purpose: "form", scope: "mine" },
                "cus-1",
            ),
        )
        await waitFor(() =>
            expect(result.current.selected.data).toEqual(cannedCustomer),
        )
        expect(apiMocks.fetchCustomerOption).toHaveBeenCalledWith("cus-1")
    })
})

describe("usePartySelectorQuery", () => {
    it("lists parties and resolves the selected detail", async () => {
        apiMocks.searchParties.mockResolvedValue([cannedParty])
        apiMocks.fetchPartyOption.mockResolvedValue(cannedParty)
        const input = { query: "主", purpose: "form" } as const
        const { result } = renderHookWithProviders(() =>
            usePartySelectorQuery(input, "p-1"),
        )
        await waitFor(() =>
            expect(result.current.list.data).toEqual([cannedParty]),
        )
        await waitFor(() =>
            expect(result.current.selected.data).toEqual(cannedParty),
        )
        expect(apiMocks.searchParties).toHaveBeenCalledWith(input)
        expect(apiMocks.fetchPartyOption).toHaveBeenCalledWith("p-1")
    })
})

describe("useWarehouseSelectorQuery", () => {
    it("lists warehouses and resolves the selected detail", async () => {
        apiMocks.searchWarehouses.mockResolvedValue([cannedWarehouse])
        apiMocks.fetchWarehouseOption.mockResolvedValue(cannedWarehouse)
        const input = { query: "仓", purpose: "filter" } as const
        const { result } = renderHookWithProviders(() =>
            useWarehouseSelectorQuery(input, "wh-1"),
        )
        await waitFor(() =>
            expect(result.current.list.data).toEqual([cannedWarehouse]),
        )
        await waitFor(() =>
            expect(result.current.selected.data).toEqual(cannedWarehouse),
        )
        expect(apiMocks.searchWarehouses).toHaveBeenCalledWith(input)
        expect(apiMocks.fetchWarehouseOption).toHaveBeenCalledWith("wh-1")
    })
})

describe("useContractSelectorQuery", () => {
    const input = {
        query: "",
        purpose: "sales-order",
        scope: "assigned",
    } as const

    it("disables both queries when enabled is false", async () => {
        apiMocks.searchContracts.mockResolvedValue([])
        apiMocks.fetchContractOption.mockResolvedValue(null)
        renderHookWithProviders(() =>
            useContractSelectorQuery(input, "c-1", { enabled: false }),
        )
        await act(async () => {
            await Promise.resolve()
        })
        expect(apiMocks.searchContracts).not.toHaveBeenCalled()
        expect(apiMocks.fetchContractOption).not.toHaveBeenCalled()
    })

    it("scopes the selected contract fetch and keeps the scope in the key", async () => {
        apiMocks.searchContracts.mockResolvedValue([cannedContract])
        apiMocks.fetchContractOption.mockResolvedValue(cannedContract)
        const { result } = renderHookWithProviders(() =>
            useContractSelectorQuery(input, "c-1"),
        )
        await waitFor(() =>
            expect(result.current.selected.data).toEqual(cannedContract),
        )
        expect(apiMocks.fetchContractOption).toHaveBeenCalledWith("c-1", {
            scope: "assigned",
        })
        expect(entitySelectorKeys.contractDetail("c-1", "assigned")).toEqual([
            "entity-selectors",
            "contract",
            "detail",
            "c-1",
            "assigned",
        ])
    })
})

describe("useSalesOrderSelectorQuery", () => {
    const input = { query: "SO", purpose: "filter" } as const

    it("maps list rows into combobox items with nature labels", async () => {
        salesOrderMocks.fetchSalesOrders.mockResolvedValue({
            items: [
                {
                    id: "so-1",
                    documentNumber: "SO-001",
                    customerName: "示例客户",
                    primaryStatus: { label: "已确认", tone: "success" },
                    amountGross: "1,000.00",
                    nature: "card_voucher",
                },
                {
                    id: "so-2",
                    documentNumber: "SO-002",
                    customerName: "示例客户二",
                    primaryStatus: { label: "草稿", tone: "neutral" },
                    amountGross: "500.00",
                    nature: "physical_service",
                },
            ],
        })
        const { result } = renderHookWithProviders(() =>
            useSalesOrderSelectorQuery(input),
        )
        await waitFor(() =>
            expect(result.current.list.data).toEqual([
                {
                    id: "so-1",
                    documentNumber: "SO-001",
                    customerName: "示例客户",
                    statusLabel: "已确认",
                    statusTone: "success",
                    amountGross: "1,000.00",
                    natureLabel: "卡券",
                },
                {
                    id: "so-2",
                    documentNumber: "SO-002",
                    customerName: "示例客户二",
                    statusLabel: "草稿",
                    statusTone: "neutral",
                    amountGross: "500.00",
                    natureLabel: "实物与服务",
                },
            ]),
        )
        expect(salesOrderMocks.fetchSalesOrders).toHaveBeenCalledWith({
            page: 1,
            pageSize: 30,
            search: "SO",
        })
    })

    it("maps the selected order detail", async () => {
        salesOrderMocks.fetchSalesOrders.mockResolvedValue({ items: [] })
        salesOrderMocks.fetchSalesOrderDetail.mockResolvedValue({
            id: "so-1",
            documentNumber: "SO-001",
            customerName: "示例客户",
            primaryStatus: { label: "已确认", tone: "success" },
            amountGross: "1,000.00",
            nature: "physical_service",
        })
        const { result } = renderHookWithProviders(() =>
            useSalesOrderSelectorQuery(input, "so-1"),
        )
        await waitFor(() =>
            expect(result.current.selected.data).toEqual({
                id: "so-1",
                documentNumber: "SO-001",
                customerName: "示例客户",
                statusLabel: "已确认",
                statusTone: "success",
                amountGross: "1,000.00",
                natureLabel: "实物与服务",
            }),
        )
        expect(salesOrderMocks.fetchSalesOrderDetail).toHaveBeenCalledWith(
            "so-1",
        )
    })

    it("yields null when the detail fetch finds nothing", async () => {
        salesOrderMocks.fetchSalesOrders.mockResolvedValue({ items: [] })
        salesOrderMocks.fetchSalesOrderDetail.mockResolvedValue(null)
        const { result } = renderHookWithProviders(() =>
            useSalesOrderSelectorQuery(input, "so-missing"),
        )
        await waitFor(() =>
            expect(result.current.selected.data).toBeNull(),
        )
    })
})

describe("useSellableSkuSelectorQuery", () => {
    it("passes the full input to searchSellableSkus", async () => {
        const canned = [
            {
                productId: "sku-1",
                revisionId: "sku-r1",
                sku: "SKU-001",
                name: "可销售商品",
                statusLabel: "可销售",
                statusTone: "success",
                baseUnit: "件",
                description: "规格 · 单位 件 · 有效供应商 2",
            },
        ]
        apiMocks.searchSellableSkus.mockResolvedValue(canned)
        const input = {
            query: "商",
            purpose: "sales-order",
            productKind: "GOODS",
            excludeProductKind: "VOUCHER",
        } as const
        const { result } = renderHookWithProviders(() =>
            useSellableSkuSelectorQuery(input),
        )
        await waitFor(() => expect(result.current.data).toEqual(canned))
        expect(apiMocks.searchSellableSkus).toHaveBeenCalledWith(input)
    })
})

describe("useCompanySkuSelectorQuery", () => {
    it("passes the input to searchCompanySkus", async () => {
        const canned = [
            {
                productId: "cs-1",
                sku: "CS-001",
                name: "公司商品",
                statusLabel: "启用",
                statusTone: "success",
                description: "公司规格",
            },
        ]
        apiMocks.searchCompanySkus.mockResolvedValue(canned)
        const input = { query: "公", purpose: "supplier-offering" } as const
        const { result } = renderHookWithProviders(() =>
            useCompanySkuSelectorQuery(input),
        )
        await waitFor(() => expect(result.current.data).toEqual(canned))
        expect(apiMocks.searchCompanySkus).toHaveBeenCalledWith(input)
    })
})

describe("useMallSelectorQuery", () => {
    it("loads mall options keyed by purpose", async () => {
        apiMocks.fetchMallOptions.mockResolvedValue([cannedMall])
        const { result } = renderHookWithProviders(() =>
            useMallSelectorQuery("filter"),
        )
        await waitFor(() => expect(result.current.data).toEqual([cannedMall]))
        expect(apiMocks.fetchMallOptions).toHaveBeenCalledTimes(1)
    })
})

describe("useVoucherCategorySelectorQuery", () => {
    it("prefers enabled voucher-category profiles", async () => {
        masterDataMocks.fetchMasterDataList.mockResolvedValue({
            rows: [
                {
                    stableId: "vc-1",
                    currentRevisionId: "vc-r1",
                    stableNo: "VC-001",
                    name: "饮品券",
                    lifecycleStatusLabel: "启用",
                    lifecycleTone: "success",
                    keyFacts: [{ label: "说明", value: "全场通用" }],
                },
            ],
        })
        const { result } = renderHookWithProviders(() =>
            useVoucherCategorySelectorQuery("sales-order"),
        )
        await waitFor(() =>
            expect(result.current.data).toEqual([
                {
                    productId: "vc-1",
                    revisionId: "vc-r1",
                    sku: "VC-001",
                    name: "饮品券",
                    statusLabel: "启用",
                    statusTone: "success",
                    baseUnit: "张",
                    description: "全场通用",
                },
            ]),
        )
        expect(masterDataMocks.fetchMasterDataList).toHaveBeenCalledWith({
            resource: "voucher-categories",
            lifecycleStatus: "enabled",
        })
    })

    it("falls back to sellable voucher SKUs when the profile is empty", async () => {
        masterDataMocks.fetchMasterDataList
            .mockResolvedValueOnce({ rows: [] })
            .mockResolvedValueOnce({
                rows: [
                    {
                        stableId: "sku-1",
                        currentRevisionId: "sku-r1",
                        stableNo: "SKU-001",
                        name: "卡券商品",
                        lifecycleStatusLabel: "可销售",
                        lifecycleTone: "success",
                        keyFacts: [{ label: "商品类型", value: "卡券" }],
                    },
                ],
            })
        const { result } = renderHookWithProviders(() =>
            useVoucherCategorySelectorQuery("form"),
        )
        await waitFor(() => expect(result.current.data).toHaveLength(1))
        expect(result.current.data?.[0]).toEqual({
            productId: "sku-1",
            revisionId: "sku-r1",
            sku: "SKU-001",
            name: "卡券商品",
            statusLabel: "可销售",
            statusTone: "success",
            baseUnit: "张",
            description: "卡券",
        })
        expect(masterDataMocks.fetchMasterDataList).toHaveBeenNthCalledWith(2, {
            resource: "sellable-items",
            lifecycleStatus: "enabled",
            productKind: "VOUCHER",
        })
    })
})
