import { beforeEach, describe, expect, it, vi } from "vitest"

import { apiGet, apiPost } from "@/lib/api/client"
import type { WorkItemDto } from "@/features/work-items/types"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"
import {
    createSupplierOffering,
    fetchSupplierOfferings,
    fetchSupplierOfferingsForSkus,
    fetchSupplierSupplyExceptionWorkItem,
    reviseSupplierOffering,
    updateSupplierOfferingAvailability,
} from "./offerings"

vi.mock("@/lib/api/client", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

const mockedApiGet = vi.mocked(apiGet)
const mockedApiPost = vi.mocked(apiPost)

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

const validWorkItemDto: WorkItemDto = {
    id: "wi_1",
    work_item_type: "BUSINESS_EXCEPTION",
    handler_key: "supplier_supply_exception",
    destination_workspace_id: "W21",
    approval_step_instance_id: null,
    status: "OPEN",
    assignment_mode: "DIRECT",
    assignment_source: "admin",
    owner_role: "buyer",
    owner_role_label: "采购",
    owner_organization_id: "org_1",
    owner_organization: { id: "org_1", display_name: "采购部" },
    owner_user_id: "u1",
    owner_user: { id: "u1", display_name: "张三" },
    processing_state: "READY",
    processing_blocker: null,
    business_object_type: "SUPPLIER_OFFERING",
    business_object_id: "ofr_1",
    root_business_object_id: "ofr_1",
    business_object_label: "示例供给",
    counterparty_label: null,
    subject_version: "v1",
    task_version: "v2",
    allowed_actions: ["VIEW", "PROCESS"],
    action_blockers: [],
    priority: 1,
    due_at: null,
    reason_code: "SUPPLIER_STOPPED",
    reason_label: "停止供应",
    impact_summary: "已暂停",
    summary_sections: [],
    created_at: 1_700_000_000_000,
    queue_context_id: null,
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("fetchSupplierOfferings", () => {
    it("maps camelCase query fields to snake_case params with defaults", async () => {
        mockedApiGet.mockResolvedValue({
            items: [offeringFixture],
            total: 1,
            page: 1,
            page_size: 50,
        })

        await fetchSupplierOfferings({ page: 1, pageSize: 50 })

        expect(mockedApiGet).toHaveBeenCalledWith(
            "/admin/supplier-offerings",
            {
                q: undefined,
                sku_id: undefined,
                sku_no: undefined,
                product_no: undefined,
                supplier_id: undefined,
                status: undefined,
                source_type: undefined,
                availability_status: undefined,
                page: 1,
                page_size: 50,
                sort_by: "created_at",
                sort_dir: "desc",
            },
        )
    })

    it("trims text filters and passes through structured filters", async () => {
        mockedApiGet.mockResolvedValue({
            items: [],
            total: 0,
            page: 2,
            page_size: 20,
        })

        await fetchSupplierOfferings({
            q: "  abc  ",
            skuId: "sku_1",
            status: "ACTIVE",
            sourceType: "API",
            availabilityStatus: "STALE",
            page: 2,
            pageSize: 20,
        })

        expect(mockedApiGet).toHaveBeenCalledWith(
            "/admin/supplier-offerings",
            {
                q: "abc",
                sku_id: "sku_1",
                status: "ACTIVE",
                source_type: "API",
                availability_status: "STALE",
                page: 2,
                page_size: 20,
                sort_by: "created_at",
                sort_dir: "desc",
            },
        )
    })
})

describe("fetchSupplierOfferingsForSkus", () => {
    it("dedupes sku ids and walks all pages per sku", async () => {
        const pageOne = {
            items: [offeringFixture],
            total: 2,
            page: 1,
            page_size: 100,
        }
        const pageTwo = {
            items: [{ ...offeringFixture, id: "ofr_2" }],
            total: 2,
            page: 2,
            page_size: 100,
        }
        mockedApiGet
            .mockResolvedValueOnce(pageOne)
            .mockResolvedValueOnce(pageTwo)

        const result = await fetchSupplierOfferingsForSkus([
            "sku_1",
            "sku_1",
            "",
        ])

        expect(mockedApiGet).toHaveBeenCalledTimes(2)
        expect(mockedApiGet).toHaveBeenNthCalledWith(
            1,
            "/admin/supplier-offerings",
            expect.objectContaining({
                sku_id: "sku_1",
                page: 1,
                page_size: 100,
            }),
        )
        expect(mockedApiGet).toHaveBeenNthCalledWith(
            2,
            "/admin/supplier-offerings",
            expect.objectContaining({
                sku_id: "sku_1",
                page: 2,
                page_size: 100,
            }),
        )
        expect(result).toHaveLength(2)
        expect(result[0]?.id).toBe("ofr_1")
        expect(result[1]?.id).toBe("ofr_2")
    })
})

describe("fetchSupplierSupplyExceptionWorkItem", () => {
    it("rejects an empty workItemId before any request", async () => {
        await expect(
            fetchSupplierSupplyExceptionWorkItem("   "),
        ).rejects.toThrow("任务标识不能为空")
        expect(mockedApiGet).not.toHaveBeenCalled()
    })

    it("returns the mapped projection for a valid W21 task", async () => {
        mockedApiGet.mockResolvedValue(validWorkItemDto)

        const task = await fetchSupplierSupplyExceptionWorkItem("wi_1")

        expect(mockedApiGet).toHaveBeenCalledWith("/admin/work-items/wi_1")
        expect(task.workItemId).toBe("wi_1")
        expect(task.businessObjectId).toBe("ofr_1")
        expect(task.handlerKey).toBe("supplier_supply_exception")
        expect(task.allowedActions).toEqual(["VIEW", "PROCESS"])
    })

    it("blocks tasks that violate the W21 contract", async () => {
        mockedApiGet.mockResolvedValue({
            ...validWorkItemDto,
            handler_key: "other_handler",
        })

        await expect(
            fetchSupplierSupplyExceptionWorkItem("wi_1"),
        ).rejects.toThrow("当前任务不符合供应停止核对合同，已阻止处理。")
    })

    it("blocks tasks with unsupported allowed actions", async () => {
        mockedApiGet.mockResolvedValue({
            ...validWorkItemDto,
            allowed_actions: ["VIEW", "CLOSE"],
        })

        await expect(
            fetchSupplierSupplyExceptionWorkItem("wi_1"),
        ).rejects.toThrow("当前任务不符合供应停止核对合同，已阻止处理。")
    })
})

describe("write endpoints", () => {
    it("createSupplierOffering posts the full input to the collection route", async () => {
        mockedApiPost.mockResolvedValue({ offering_id: "ofr_1" })
        const input = {
            sku_id: "sku_1",
            supplier_id: "sup_1",
            supplier_product_code: null,
            supplier_sku_code: "V-001",
            source_type: "MANUAL" as const,
            source_connection_id: null,
            terms: {
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
            },
            availability_status: "AVAILABLE" as const,
            available_quantity: "100",
            change_reason: "新增供应商供给",
            idempotency_key: "k1",
        }

        await createSupplierOffering(input)

        expect(mockedApiPost).toHaveBeenCalledWith(
            "/admin/supplier-offerings",
            input,
        )
    })

    it("reviseSupplierOffering posts the body without offeringId to the revisions route", async () => {
        mockedApiPost.mockResolvedValue({ offering_id: "ofr_1" })

        await reviseSupplierOffering({
            offeringId: "ofr_1",
            expected_revision_no: 1,
            terms: {
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
            },
            status: "ACTIVE",
            change_reason: "调整供给条款",
            idempotency_key: "k2",
        })

        expect(mockedApiPost).toHaveBeenCalledWith(
            "/admin/supplier-offerings/ofr_1/revisions",
            expect.not.objectContaining({ offeringId: "ofr_1" }),
        )
    })

    it("updateSupplierOfferingAvailability posts to the availability route without offeringId", async () => {
        mockedApiPost.mockResolvedValue({ offering_id: "ofr_1" })

        await updateSupplierOfferingAvailability({
            offeringId: "ofr_1",
            expected_version: 1,
            availability_status: "STOPPED",
            available_quantity: null,
            change_reason: "更新当前可供情况",
            idempotency_key: "k3",
        })

        expect(mockedApiPost).toHaveBeenCalledWith(
            "/admin/supplier-offerings/ofr_1/availability",
            {
                expected_version: 1,
                availability_status: "STOPPED",
                available_quantity: null,
                change_reason: "更新当前可供情况",
                idempotency_key: "k3",
            },
        )
    })
})
