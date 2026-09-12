import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeAll, expect, test, vi } from "vitest"

import { DataTable } from "@/components/business/data-table"
import { SellableGallerySelectionBar } from "@/features/master-data/components/list/sellable-gallery-selection-bar"
import { useSellableGallerySelection } from "@/features/master-data/hooks/use-sellable-gallery-selection"
import type { MasterDataListItem } from "@/features/master-data/types"

beforeAll(() => {
    globalThis.ResizeObserver = class {
        observe() {}
        unobserve() {}
        disconnect() {}
    }
})
afterEach(cleanup)

// This surface reads only identity and name; business details are outside this test.
const rows = ["sku-1", "sku-2", "sku-3"].map((stableId) => ({
    stableId,
    name: stableId,
})) as MasterDataListItem[]
const preview = vi.fn()

function SelectionTable() {
    const selection = useSellableGallerySelection(rows)
    return (
        <>
            <SellableGallerySelectionBar
                idPrefix="sellable-table-selection"
                resultCount={rows.length}
                selectedCount={selection.selectedCount}
                allSelected={selection.allSelected}
                someSelected={selection.someSelected}
                onSelectAll={selection.selectAllResults}
                onClear={selection.clear}
            />
            <DataTable
                id="sellable-table"
                data={rows}
                rowCount={rows.length}
                columns={[{ accessorKey: "name", header: "商品" }]}
                getRowId={(row) => row.stableId}
                rowLabel={(row) => row.name}
                enableRowSelection
                rowSelection={selection.rowSelection}
                onRowSelectionChange={selection.onRowSelectionChange}
                defaultPagination={{ pageIndex: 0, pageSize: 2 }}
                manualPagination={false}
                onRowPreview={preview}
            />
            <output aria-label="已选商品">
                {[...selection.selectedIds].sort().join(",")}
            </output>
        </>
    )
}

test("table checks preserve selection across pages without opening the preview", () => {
    preview.mockClear()
    render(<SelectionTable />)
    fireEvent.click(screen.getByRole("checkbox", { name: "选择 sku-1" }))
    expect(screen.getByLabelText("已选商品").textContent).toBe("sku-1")
    expect(preview).not.toHaveBeenCalled()
    fireEvent.click(
        screen.getByRole("checkbox", { name: "选择当前页全部记录" }),
    )
    expect(screen.getByLabelText("已选商品").textContent).toBe("sku-1,sku-2")
    fireEvent.click(screen.getByRole("button", { name: "下一页" }))
    fireEvent.click(screen.getByRole("checkbox", { name: "选择 sku-3" }))
    expect(screen.getByLabelText("已选商品").textContent).toBe(
        "sku-1,sku-2,sku-3",
    )
    fireEvent.click(
        screen.getByRole("checkbox", { name: "选择当前页全部记录" }),
    )
    expect(screen.getByLabelText("已选商品").textContent).toBe("sku-1,sku-2")
})

test("select all results includes other pages and clear resets visible checkboxes", () => {
    render(<SelectionTable />)
    fireEvent.click(
        document.getElementById("sellable-table-selection-select-all-action")!,
    )
    expect(screen.getByLabelText("已选商品").textContent).toBe(
        "sku-1,sku-2,sku-3",
    )
    fireEvent.click(
        document.getElementById("sellable-table-selection-clear-selection")!,
    )
    expect(screen.getByLabelText("已选商品").textContent).toBe("")
    expect(
        screen
            .getByRole("checkbox", { name: "选择 sku-1" })
            .getAttribute("aria-checked"),
    ).toBe("false")
})
