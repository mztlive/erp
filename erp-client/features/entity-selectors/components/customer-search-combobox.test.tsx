import * as React from "react"
import { act, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { CustomerComboboxItem } from "@/components/business/entity-comboboxes"
import type { CustomerSearch } from "@/features/entity-selectors/api"
import { CustomerSearchCombobox } from "./customer-search-combobox"

const { ITEM, LABEL } = vi.hoisted(() => {
    const ITEM: CustomerComboboxItem = {
        id: "c-1",
        customerNo: "KH-000001",
        legalName: "上海示例贸易有限公司",
        shortName: "示例贸易",
        statusLabel: "启用",
        statusTone: "success",
    }
    // 列表条目的展示标签：全称（简称）
    const LABEL = "上海示例贸易有限公司（示例贸易）"
    return { ITEM, LABEL }
})

vi.mock("../hooks/queries", async (importOriginal) => {
    const original = await importOriginal<typeof import("../hooks/queries")>()
    return {
        ...original,
        useCustomerSelectorQuery: (
            input: CustomerSearch,
            _selectedId?: string,
        ) => ({
            // 模拟后端关键字搜索：匹配不到“全称（简称）”的组合标签；
            // 详情接口失败时回退 null（fetchCustomerOption 吞错返回 null）。
            list: {
                data: input.query.trim() === "" ? [ITEM] : [],
                isFetching: false,
                isError: false,
                error: null,
            },
            selected: { data: null, isFetching: false },
        }),
    }
})

function Harness() {
    const [value, setValue] = React.useState<string | undefined>(undefined)
    return (
        <CustomerSearchCombobox
            value={value}
            onValueChange={(id) => setValue(id ?? "")}
            scope="assigned"
            placeholder="搜索客户编号或名称"
        />
    )
}

describe("CustomerSearchCombobox 选中回填与远程搜索", () => {
    afterEach(() => {
        vi.useRealTimers()
    })

    it("选中条目后输入框保持名称，不在“名称↔空”之间来回闪烁", () => {
        vi.useFakeTimers()
        render(<Harness />)
        const input = screen.getByRole("combobox") as HTMLInputElement

        // 打开下拉并选中唯一条目
        fireEvent.mouseDown(input)
        const option = screen.getByRole("option")
        fireEvent.click(option)

        expect(input.value).toBe(LABEL)

        // 选中后 Base UI 会用条目标签回填输入框；回填文本不应被当成远程搜索词。
        // 若被当成搜索词：后端搜不到该标签 → 列表缺已选项 → 受控值变 null →
        // 输入框被清空 → 空搜索又把条目带回 → 名称↔空反复闪烁（每 250ms 一轮）。
        for (let round = 0; round < 6; round++) {
            act(() => {
                vi.advanceTimersByTime(260)
            })
            expect(input.value).toBe(LABEL)
        }
    })
})
