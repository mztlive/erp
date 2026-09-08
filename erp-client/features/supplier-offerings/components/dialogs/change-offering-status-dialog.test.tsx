import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"

import { ChangeOfferingStatusDialog } from "./change-offering-status-dialog"
import { PAUSE_OFFERING_INTENT } from "@/features/supplier-offerings/lib/offering-status"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"

const revise = vi.hoisted(() => ({
    isPending: false,
    mutateAsync: vi.fn(),
}))

vi.mock("@/features/supplier-offerings/hooks/queries", () => ({
    useReviseSupplierOfferingMutation: () => revise,
}))

beforeEach(() => {
    revise.mutateAsync.mockReset()
    revise.mutateAsync.mockResolvedValue({ revision_no: 4 })
})

afterEach(cleanup)

const offering: SupplierOfferingView = {
    id: "offering-1",
    sku_id: "sku-1",
    sku_name: "测试 SKU",
    supplier_id: "supplier-1",
    supplier_name: "测试供应商",
    supplier_sku_code: "SUP-SKU-1",
    source_type: "MANUAL",
    status: "ACTIVE",
    current_revision_no: 3,
    dropship_supply_price_gross: "11.30",
    bulk_supply_price_gross: "9.04",
    input_tax_rate: "0.13",
    bulk_minimum_order_quantity: "10",
    supply_region: ["全国"],
    product_capabilities: ["REFUND"],
    dropship_express: "顺丰",
    freight_amount: "1.00",
    service_fee_amount: null,
    valid_from: "2026-08-08",
    valid_to: null,
    version: 4,
    created_at: 1,
}

test("暂停确认会把当前条款原样提交为新版本", async () => {
    const onOpenChange = vi.fn()
    render(
        <ChangeOfferingStatusDialog
            offering={offering}
            intent={PAUSE_OFFERING_INTENT}
            onOpenChange={onOpenChange}
        />,
    )

    expect(screen.getByText(/条款号从 v3 变为 v4/)).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "暂停并保存新版本" }))

    await waitFor(() => expect(revise.mutateAsync).toHaveBeenCalledTimes(1))
    expect(revise.mutateAsync.mock.calls[0]?.[0]).toEqual(
        expect.objectContaining({
            offeringId: "offering-1",
            expected_revision_no: 3,
            status: "PAUSED",
            change_reason: "暂停供给关系",
            terms: expect.objectContaining({
                dropship_supply_price_gross: "11.30",
                bulk_supply_price_gross: "9.04",
                input_tax_rate: "0.13",
                bulk_minimum_order_quantity: "10",
                supply_region: ["全国"],
                valid_from: "2026-08-08",
            }),
        }),
    )
    expect(onOpenChange).toHaveBeenCalledWith(false)
})
