import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import { SupplierOfferingRowActions } from "./supplier-offering-row-actions"
import {
    PAUSE_OFFERING_INTENT,
    STOP_OFFERING_INTENT,
} from "@/features/supplier-offerings/lib/offering-status"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"

afterEach(cleanup)

function offering(
    overrides: Partial<SupplierOfferingView> = {},
): SupplierOfferingView {
    return {
        id: "offering-1",
        sku_id: "sku-1",
        supplier_id: "supplier-1",
        supplier_sku_code: "SUP-SKU-1",
        source_type: "MANUAL",
        status: "ACTIVE",
        current_revision_no: 2,
        dropship_supply_price_gross: "11.30",
        bulk_supply_price_gross: "9.04",
        input_tax_rate: "0.13",
        bulk_minimum_order_quantity: "10",
        supply_region: ["全国"],
        product_capabilities: [],
        valid_from: "2026-08-08",
        version: 1,
        created_at: 1,
        ...overrides,
    }
}

test("行上只露出操作菜单，启用中可从菜单暂停或停止", () => {
    const onChangeStatus = vi.fn()
    const onUpdateAvailability = vi.fn()
    const onReviseOffering = vi.fn()
    render(
        <SupplierOfferingRowActions
            offering={offering()}
            onUpdateAvailability={onUpdateAvailability}
            onReviseOffering={onReviseOffering}
            onChangeStatus={onChangeStatus}
        />,
    )

    expect(screen.queryByRole("menuitem", { name: "暂停" })).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: /操作/ }))
    fireEvent.click(screen.getByRole("menuitem", { name: "更新可供" }))
    fireEvent.click(screen.getByRole("button", { name: /操作/ }))
    fireEvent.click(screen.getByRole("menuitem", { name: "修订条款" }))
    fireEvent.click(screen.getByRole("button", { name: /操作/ }))
    fireEvent.click(screen.getByRole("menuitem", { name: "暂停" }))
    fireEvent.click(screen.getByRole("button", { name: /操作/ }))
    fireEvent.click(screen.getByRole("menuitem", { name: "停止" }))

    expect(onUpdateAvailability).toHaveBeenCalledTimes(1)
    expect(onReviseOffering).toHaveBeenCalledTimes(1)
    expect(onChangeStatus).toHaveBeenNthCalledWith(
        1,
        expect.objectContaining({ id: "offering-1" }),
        PAUSE_OFFERING_INTENT,
    )
    expect(onChangeStatus).toHaveBeenNthCalledWith(
        2,
        expect.objectContaining({ id: "offering-1" }),
        STOP_OFFERING_INTENT,
    )
})

test("已停止的行菜单只给出启用，不再提供停止", () => {
    render(
        <SupplierOfferingRowActions
            offering={offering({ status: "STOPPED" })}
            onUpdateAvailability={() => undefined}
            onReviseOffering={() => undefined}
            onChangeStatus={() => undefined}
        />,
    )

    fireEvent.click(screen.getByRole("button", { name: /操作/ }))
    expect(screen.getByRole("menuitem", { name: "启用" })).toBeTruthy()
    expect(screen.queryByRole("menuitem", { name: "停止" })).toBeNull()
    expect(screen.queryByRole("menuitem", { name: "暂停" })).toBeNull()
})
