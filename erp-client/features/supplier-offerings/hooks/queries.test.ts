import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, waitFor } from "@testing-library/react"

import {
    createSupplierOffering,
    fetchSupplierOfferings,
    fetchSupplierOfferingsForSkus,
    fetchSupplierSupplyExceptionWorkItem,
    reviseSupplierOffering,
    updateSupplierOfferingAvailability,
} from "@/features/supplier-offerings/api/offerings"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import type {
    CreateSupplierOfferingInput,
    ReviseSupplierOfferingInput,
    SupplierOfferingView,
    SupplierSupplyExceptionWorkItem,
    UpdateOfferingAvailabilityInput,
} from "@/features/supplier-offerings/types"
import {
    useCreateSupplierOfferingMutation,
    useReviseSupplierOfferingMutation,
    useSupplierOfferingsForSkusQuery,
    useSupplierOfferingsQuery,
    useSupplierSupplyExceptionWorkItemQuery,
    useUpdateOfferingAvailabilityMutation,
} from "./queries"

vi.mock("@/features/supplier-offerings/api/offerings", () => ({
    createSupplierOffering: vi.fn(),
    fetchSupplierOfferings: vi.fn(),
    fetchSupplierOfferingsForSkus: vi.fn(),
    fetchSupplierSupplyExceptionWorkItem: vi.fn(),
    reviseSupplierOffering: vi.fn(),
    updateSupplierOfferingAvailability: vi.fn(),
}))

const mockedCreate = vi.mocked(createSupplierOffering)
const mockedFetchList = vi.mocked(fetchSupplierOfferings)
const mockedFetchForSkus = vi.mocked(fetchSupplierOfferingsForSkus)
const mockedFetchWorkItem = vi.mocked(fetchSupplierSupplyExceptionWorkItem)
const mockedRevise = vi.mocked(reviseSupplierOffering)
const mockedUpdateAvailability = vi.mocked(updateSupplierOfferingAvailability)

const listQuery = { page: 1, pageSize: 50 } as const

const offeringFixture: SupplierOfferingView = {
    id: "ofr_1",
    sku_id: "sku_1",
    sku_no: "SKU-001",
    product_no: "P-1001",
    sku_name: "示例 SKU",
    specification: null,
    supplier_id: "sup_1",
    supplier_no: "SUP-001",
    supplier_name: "示例供应商",
    supplier_product_code: null,
    supplier_sku_code: "V-001",
    source_type: "MANUAL",
    source_connection_id: null,
    status: "ACTIVE",
    current_revision_id: "rev_1",
    current_revision_no: 1,
    dropship_supply_price_gross: "10.0000",
    dropship_supply_price_net: "9.0000",
    bulk_supply_price_gross: "8.0000",
    bulk_supply_price_net: "7.2000",
    input_tax_rate: "0.130000",
    bulk_minimum_order_quantity: "10",
    supply_region: ["华东"],
    product_capabilities: [],
    dropship_express: null,
    freight_amount: null,
    service_fee_amount: null,
    valid_from: "2026-01-01",
    valid_to: null,
    availability_status: "AVAILABLE",
    available_quantity: "100",
    availability_source_updated_at: null,
    availability_version: 1,
    version: 1,
    created_at: 1_700_000_000_000,
}

const pageFixture = {
    items: [offeringFixture],
    total: 1,
    page: 1,
    page_size: 50,
}

const workItemFixture: SupplierSupplyExceptionWorkItem = {
    workItemId: "wi_1",
    workItemType: "BUSINESS_EXCEPTION",
    handlerKey: "supplier_supply_exception",
    destinationWorkspaceId: "W21",
    status: "OPEN",
    assignmentSource: "admin",
    ownerRole: "buyer",
    ownerRoleLabel: "采购",
    ownerOrganization: { id: "org_1", displayName: "采购部" },
    ownerUser: { id: "u1", displayName: "张三" },
    processingState: "READY",
    businessObjectType: "SUPPLIER_OFFERING",
    businessObjectId: "ofr_1",
    rootBusinessObjectId: "ofr_1",
    businessObjectLabel: "示例供给",
    subjectVersion: "v1",
    taskVersion: "v2",
    allowedActions: ["VIEW", "PROCESS"],
    actionBlockers: [],
    priority: 1,
    reasonCode: "SUPPLIER_STOPPED",
    reasonLabel: "停止供应",
    impactSummary: "已暂停",
    nextActionHint: "处理供给异常",
    summarySections: [],
    briefLines: [],
    createdAt: 1_700_000_000_000,
}

const terms = {
    dropship_supply_price_gross: "10.0000",
    bulk_supply_price_gross: "8.0000",
    input_tax_rate: "0.130000",
    bulk_minimum_order_quantity: "10",
    supply_region: ["华东"],
    product_capabilities: [],
    valid_from: "2026-01-01",
    valid_to: null,
    dropship_express: null,
    freight_amount: null,
    service_fee_amount: null,
} as const

const createInput: CreateSupplierOfferingInput = {
    sku_id: "sku_1",
    supplier_id: "sup_1",
    supplier_product_code: null,
    supplier_sku_code: "V-001",
    source_type: "MANUAL",
    source_connection_id: null,
    terms: { ...terms },
    availability_status: "AVAILABLE",
    available_quantity: "100",
    change_reason: "新增供应商供给",
    idempotency_key: "k1",
}

const reviseInput: ReviseSupplierOfferingInput = {
    offeringId: "ofr_1",
    expected_revision_no: 1,
    terms: { ...terms },
    status: "ACTIVE",
    change_reason: "调整供给条款",
    idempotency_key: "k2",
}

const availabilityInput: UpdateOfferingAvailabilityInput = {
    offeringId: "ofr_1",
    expected_version: 1,
    availability_status: "STOPPED",
    available_quantity: null,
    change_reason: "更新当前可供情况",
    idempotency_key: "k3",
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useSupplierOfferingsQuery", () => {
    it("fetches under the supplier-offerings/list key and reuses it across renders", async () => {
        mockedFetchList.mockResolvedValue(pageFixture)
        const queryClient = createFreshQueryClient()

        const { result, rerender } = renderHookWithProviders(
            () => useSupplierOfferingsQuery(listQuery),
            { queryClient },
        )

        await waitFor(() => expect(result.current.data).toEqual(pageFixture))
        expect(mockedFetchList).toHaveBeenCalledWith(listQuery)
        expect(
            queryClient.getQueryData(["supplier-offerings", "list", listQuery]),
        ).toEqual(pageFixture)

        rerender()
        await waitFor(() => expect(result.current.data).toEqual(pageFixture))
        expect(mockedFetchList).toHaveBeenCalledTimes(1)
    })

    it("propagates query errors", async () => {
        mockedFetchList.mockRejectedValue(new Error("network down"))
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useSupplierOfferingsQuery(listQuery),
            { queryClient },
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useSupplierSupplyExceptionWorkItemQuery", () => {
    it("stays disabled and never fetches without a workItemId", () => {
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useSupplierSupplyExceptionWorkItemQuery(undefined),
            { queryClient },
        )

        expect(result.current.fetchStatus).toBe("idle")
        expect(result.current.data).toBeUndefined()
        expect(mockedFetchWorkItem).not.toHaveBeenCalled()
    })

    it("fetches with the workItemId and caches under the supply-exception key", async () => {
        mockedFetchWorkItem.mockResolvedValue(workItemFixture)
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useSupplierSupplyExceptionWorkItemQuery("wi_1"),
            { queryClient },
        )

        await waitFor(() =>
            expect(result.current.data).toEqual(workItemFixture),
        )
        expect(mockedFetchWorkItem).toHaveBeenCalledWith("wi_1")
        expect(
            queryClient.getQueryData([
                "supplier-offerings",
                "supply-exception",
                "wi_1",
            ]),
        ).toEqual(workItemFixture)
    })

    it("surfaces validation failures as query errors", async () => {
        mockedFetchWorkItem.mockRejectedValue(new Error("任务标识不能为空"))
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useSupplierSupplyExceptionWorkItemQuery("wi_1"),
            { queryClient },
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useSupplierOfferingsForSkusQuery", () => {
    it("stays disabled for an empty or blank-only sku list", () => {
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useSupplierOfferingsForSkusQuery([]),
            { queryClient },
        )

        expect(result.current.fetchStatus).toBe("idle")
        expect(mockedFetchForSkus).not.toHaveBeenCalled()
    })

    it("dedupes and sorts sku ids in the query key and queryFn", async () => {
        mockedFetchForSkus.mockResolvedValue([offeringFixture])
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useSupplierOfferingsForSkusQuery(["b", "a", "b", ""]),
            { queryClient },
        )

        await waitFor(() =>
            expect(result.current.data).toEqual([offeringFixture]),
        )
        expect(mockedFetchForSkus).toHaveBeenCalledWith(["a", "b"])
        expect(
            queryClient.getQueryData([
                "supplier-offerings",
                "sku-details",
                ["a", "b"],
            ]),
        ).toEqual([offeringFixture])
    })
})

describe("useCreateSupplierOfferingMutation", () => {
    it("wires mutationFn to the api and invalidates offering and master data on success", async () => {
        mockedCreate.mockResolvedValue({
            offering_id: "ofr_1",
            revision_id: "rev_1",
            availability_id: "avl_1",
            revision_no: 1,
            status: "ACTIVE",
        })
        const queryClient = createFreshQueryClient()
        queryClient.setQueryData(
            ["supplier-offerings", "list", listQuery],
            pageFixture,
        )
        queryClient.setQueryData(["master-data"], { seeded: true })

        const { result } = renderHookWithProviders(
            () => useCreateSupplierOfferingMutation(),
            { queryClient },
        )

        await act(async () => {
            await result.current.mutateAsync(createInput)
        })

        expect(mockedCreate).toHaveBeenCalledWith(createInput)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(
            queryClient.getQueryState(["supplier-offerings", "list", listQuery])
                ?.isInvalidated,
        ).toBe(true)
        expect(queryClient.getQueryState(["master-data"])?.isInvalidated).toBe(
            true,
        )
    })

    it("keeps the error on the mutation state when creation fails", async () => {
        mockedCreate.mockRejectedValue(new Error("duplicate"))
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useCreateSupplierOfferingMutation(),
            { queryClient },
        )

        await act(async () => {
            await result.current.mutateAsync(createInput).catch(() => undefined)
        })

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useReviseSupplierOfferingMutation", () => {
    it("wires mutationFn to the revise api", async () => {
        mockedRevise.mockResolvedValue({
            offering_id: "ofr_1",
            revision_id: "rev_2",
            revision_no: 2,
            status: "ACTIVE",
            version: 2,
        })
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useReviseSupplierOfferingMutation(),
            { queryClient },
        )

        await act(async () => {
            await result.current.mutateAsync(reviseInput)
        })

        expect(mockedRevise).toHaveBeenCalledWith(reviseInput)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
    })
})

describe("useUpdateOfferingAvailabilityMutation", () => {
    it("wires mutationFn to the availability api and refetches the active list query on success", async () => {
        mockedFetchList.mockResolvedValue(pageFixture)
        mockedUpdateAvailability.mockResolvedValue({
            offering_id: "ofr_1",
            availability_status: "STOPPED",
            availability_version: 2,
            source_updated_at: 1_700_000_000_000,
        })
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => ({
                query: useSupplierOfferingsQuery(listQuery),
                mutation: useUpdateOfferingAvailabilityMutation(),
            }),
            { queryClient },
        )

        await waitFor(() =>
            expect(result.current.query.data).toEqual(pageFixture),
        )
        expect(mockedFetchList).toHaveBeenCalledTimes(1)

        await act(async () => {
            await result.current.mutation.mutateAsync(availabilityInput)
        })

        expect(mockedUpdateAvailability).toHaveBeenCalledWith(availabilityInput)
        await waitFor(() =>
            expect(result.current.mutation.isSuccess).toBe(true),
        )
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalledTimes(2))
    })
})
