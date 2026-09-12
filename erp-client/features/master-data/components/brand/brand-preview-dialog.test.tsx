import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import { BrandPreviewDialog } from "./brand-preview-dialog"
import { mapBrandRow } from "@/features/master-data/api/list-mappers"

const brand = mapBrandRow({
    id: "brand-1",
    brand_code: "FSY",
    name: "福尚云",
    status: "active",
    created_at: 0,
    version: 1,
})
afterEach(cleanup)

test("品牌预览只读，维护操作传递当前品牌，关闭保留独立入口", () => {
    const onRevise = vi.fn()
    const onDisable = vi.fn()
    const onClose = vi.fn()
    render(
        <BrandPreviewDialog
            row={brand}
            lastFocusedRowId={{ current: null }}
            onClose={onClose}
            onRevise={onRevise}
            onDisable={onDisable}
        />,
    )
    expect(screen.getByRole("dialog", { name: "福尚云" })).toBeTruthy()
    expect(screen.getByText("FSY")).toBeTruthy()
    expect(screen.queryByRole("textbox")).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "更新资料" }))
    expect(onRevise).toHaveBeenCalledWith(brand)
    fireEvent.click(screen.getByRole("button", { name: "停用" }))
    expect(onDisable).toHaveBeenCalledWith(brand)
    fireEvent.click(
        document.getElementById("master-data-brands-preview-cancel")!,
    )
    expect(onClose).toHaveBeenCalledOnce()
})

test("品牌操作权限和阻断原因在预览内仍然生效", () => {
    const onRevise = vi.fn()
    const onDisable = vi.fn()
    const restricted = {
        ...brand,
        allowedActions: [],
        actionBlockers: [
            {
                action: "CREATE_REVISION",
                code: "FORBIDDEN",
                message: "没有品牌更新权限",
            },
            { action: "DISABLE", code: "DISABLED", message: "该品牌已停用" },
        ],
    }
    render(
        <BrandPreviewDialog
            row={restricted}
            lastFocusedRowId={{ current: null }}
            onClose={vi.fn()}
            onRevise={onRevise}
            onDisable={onDisable}
        />,
    )
    const revise = screen.getByRole<HTMLButtonElement>("button", {
        name: "更新资料",
    })
    const disable = screen.getByRole<HTMLButtonElement>("button", {
        name: "停用",
    })
    expect(revise.disabled).toBe(true)
    expect(revise.title).toBe("没有品牌更新权限")
    expect(disable.disabled).toBe(true)
    expect(disable.title).toBe("该品牌已停用")
    fireEvent.click(revise)
    fireEvent.click(disable)
    expect(onRevise).not.toHaveBeenCalled()
    expect(onDisable).not.toHaveBeenCalled()
})
