import * as React from "react"
import { act, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { CustomerComboboxItem } from "@/components/business/entity-comboboxes"
import type { CustomerSearch } from "@/features/entity-selectors/api"
import { CustomerSearchCombobox } from "./customer-search-combobox"

vi.mock("../hooks/use-search-input", async (importOriginal) => {
    const original = await importOriginal<typeof import("../hooks/use-search-input")>()
    return {
        useSearchInput: () => {
            const s = original.useSearchInput()
            const set = s.onSearchChange
            s.onSearchChange = (v) => {
                console.log("SEARCH-SET ->", JSON.stringify(v))
                set(v)
            }
            return s
        },
    }
})

const { ITEM } = vi.hoisted(() => {
    const ITEM: CustomerComboboxItem = {
        id: "c-1", customerNo: "KH-000001", legalName: "上海示例贸易有限公司",
        shortName: "示例贸易", statusLabel: "启用", statusTone: "success",
    }
    return { ITEM }
})

const calls: string[] = []

vi.mock("../hooks/queries", async (importOriginal) => {
    const original = await importOriginal<typeof import("../hooks/queries")>()
    return {
        ...original,
        useCustomerSelectorQuery: (input: CustomerSearch, _selectedId?: string) => {
            calls.push(`query=${JSON.stringify(input.query)}`)
            return {
                list: { data: input.query.trim() === "" ? [ITEM] : [], isFetching: false, isError: false, error: null },
                selected: { data: null, isFetching: false },
            }
        },
    }
})

function Harness() {
    const [value, setValue] = React.useState<string | undefined>(undefined)
    return (
        <CustomerSearchCombobox
            value={value}
            onValueChange={(id) => { console.log("onValueChange ->", id); setValue(id ?? "") }}
            onItemChange={(item) => console.log("onItemChange ->", item?.legalName)}
            scope="assigned"
            placeholder="搜索客户编号或名称"
        />
    )
}

describe("debug", () => {
    afterEach(() => vi.useRealTimers())
    it("trace", () => {
        vi.useFakeTimers()
        render(<Harness />)
        const input = screen.getByRole("combobox") as HTMLInputElement
        console.log("initial input.value:", JSON.stringify(input.value))
        fireEvent.mouseDown(input)
        console.log("after mousedown, options:", screen.queryAllByRole("option").length)
        const option = screen.getByRole("option")
        fireEvent.click(option)
        console.log("after click input.value:", JSON.stringify(input.value), "calls:", JSON.stringify(calls))
        for (let i = 0; i < 4; i++) {
            act(() => { vi.advanceTimersByTime(260) })
            console.log(`round ${i}: input.value=`, JSON.stringify(input.value), "calls:", JSON.stringify(calls))
        }
        expect(true).toBe(true)
    })
})
