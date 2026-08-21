import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { MallSyncViewToolbar } from "./mall-sync-view-toolbar"
import type {
    MallSyncAppliedChip,
    MallSyncFilterKey,
} from "@/features/mall-sync/pages/hooks/use-mall-sync-url-state"
import type { MallSyncViewName } from "@/features/mall-sync/types"

afterEach(cleanup)

function renderToolbar(
    overrides: {
        view?: MallSyncViewName
        panelOpen?: boolean
        hasActiveFilters?: boolean
        hasStructuredFilters?: boolean
        appliedChips?: readonly MallSyncAppliedChip[]
        onRemoveFilter?: (key: MallSyncFilterKey) => void
        onApply?: () => void
        onResetMore?: () => void
        onClearAll?: () => void
        onSetPanel?: () => void
    } = {},
) {
    const onApply = overrides.onApply ?? vi.fn()
    const onResetMore = overrides.onResetMore ?? vi.fn()
    const onClearAll = overrides.onClearAll ?? vi.fn()
    const onRemoveFilter = overrides.onRemoveFilter ?? vi.fn()
    const onSetPanel = overrides.onSetPanel ?? vi.fn()
    render(
        <MallSyncViewToolbar
            view={overrides.view ?? "mapping"}
            onViewChange={vi.fn()}
            searchInputRef={{ current: null }}
            searchDraft=""
            setSearchDraft={vi.fn()}
            mappingTypeDraft="all"
            setMappingTypeDraft={vi.fn()}
            panelOpen={overrides.panelOpen ?? false}
            setPanelOpen={onSetPanel}
            hasStructuredFilters={overrides.hasStructuredFilters ?? false}
            hasActiveFilters={overrides.hasActiveFilters ?? false}
            appliedChips={overrides.appliedChips ?? []}
            removeFilter={onRemoveFilter}
            applyFilters={onApply}
            resetMoreFilters={onResetMore}
            clearAllFilters={onClearAll}
        />,
    )
    return { onApply, onResetMore, onClearAll, onRemoveFilter, onSetPanel }
}

describe("MallSyncViewToolbar", () => {
    it("收起态搜索框尾部无提交箭头，且不出现「应用全部筛选」", () => {
        const { onApply } = renderToolbar()
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
        fireEvent.submit(document.querySelector("form")!)
        expect(onApply).toHaveBeenCalledTimes(1)
    })

    it("展开态隐藏搜索框尾部箭头，只有面板底部唯一主提交", () => {
        const { onApply } = renderToolbar({ panelOpen: true })
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
        fireEvent.click(screen.getByRole("button", { name: "应用全部筛选" }))
        expect(onApply).toHaveBeenCalledTimes(1)
    })

    it("收起态 Enter 与展开态主按钮走同一个 form 提交", () => {
        const { onApply } = renderToolbar({ panelOpen: true })
        fireEvent.submit(document.querySelector("form")!)
        expect(onApply).toHaveBeenCalledTimes(1)
        const forms = document.querySelectorAll("form")
        expect(forms).toHaveLength(1)
    })

    it("「更多筛选」带 aria-expanded / aria-controls，点击只切展开态", () => {
        const { onSetPanel } = renderToolbar()
        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        expect(toggle.getAttribute("aria-expanded")).toBe("false")
        expect(toggle.hasAttribute("aria-controls")).toBe(true)
        fireEvent.click(toggle)
        expect(onSetPanel).toHaveBeenCalled()
    })

    it("非映射视图不渲染「更多筛选」", () => {
        renderToolbar({ view: "jobs" })
        expect(screen.queryByRole("button", { name: /更多筛选/ })).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
    })

    it("展开面板含映射类型字段与底部范围说明和两个动作", () => {
        renderToolbar({ panelOpen: true })
        expect(screen.getByText("映射类型")).toBeDefined()
        expect(
            screen.getByText(
                "将同时应用上方关键词和以下筛选条件；结果也用于导出。",
            ),
        ).toBeDefined()
        expect(
            screen.getByRole("button", { name: "重置更多条件" }),
        ).toBeDefined()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
    })

    it("重置更多条件只重置结构化条件", () => {
        const { onResetMore } = renderToolbar({ panelOpen: true })
        fireEvent.click(screen.getByRole("button", { name: "重置更多条件" }))
        expect(onResetMore).toHaveBeenCalledTimes(1)
    })

    it("已生效条件以 chip 行展示，末尾清空全部", () => {
        const { onClearAll } = renderToolbar({
            hasActiveFilters: true,
            appliedChips: [
                { key: "q", label: "搜索：SO-1" },
                { key: "mappingType", label: "映射类型：客户映射" },
            ],
        })
        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：SO-1")).toBeDefined()
        expect(screen.getByText("映射类型：客户映射")).toBeDefined()
        fireEvent.click(screen.getByRole("button", { name: "清空全部" }))
        expect(onClearAll).toHaveBeenCalledTimes(1)
    })

    it("chip 关闭按钮只移除自己的条件", () => {
        const { onRemoveFilter } = renderToolbar({
            hasActiveFilters: true,
            appliedChips: [{ key: "jobId", label: "任务：JOB-1" }],
        })
        fireEvent.click(
            screen.getByRole("button", { name: "移除任务：JOB-1" }),
        )
        expect(onRemoveFilter).toHaveBeenCalledWith("jobId")
    })

    it("无 chip 且面板收起时不渲染 secondary 行", () => {
        renderToolbar()
        expect(screen.queryByText("已筛选")).toBeNull()
        expect(screen.queryByText("清空全部")).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
    })
})
