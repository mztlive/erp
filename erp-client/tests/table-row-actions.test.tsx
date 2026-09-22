import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { TableRowActions } from "@/components/business/table-row-actions"

afterEach(() => {
    cleanup()
})

describe("TableRowActions", () => {
    it("露出最多两个文字按钮，破坏性动作进菜单底部", () => {
        render(
            <TableRowActions
                moreId="row-more"
                moreLabel="销售员 更多操作"
                actions={[
                    { id: "edit", label: "编辑", onClick: vi.fn() },
                    { id: "perms", label: "查看权限", onClick: vi.fn() },
                    {
                        id: "dept",
                        label: "调整部门",
                        placement: "menu",
                        onClick: vi.fn(),
                    },
                    {
                        id: "delete",
                        label: "删除",
                        destructive: true,
                        onClick: vi.fn(),
                    },
                ]}
            />,
        )

        expect(screen.getByRole("button", { name: "编辑" }).id).toBe("edit")
        expect(screen.getByRole("button", { name: "查看权限" })).toBeTruthy()
        expect(screen.queryByRole("button", { name: "删除" })).toBeNull()
        expect(screen.queryByRole("button", { name: "调整部门" })).toBeNull()
        fireEvent.click(screen.getByRole("button", { name: "销售员 更多操作" }))
        expect(screen.getByRole("menuitem", { name: "调整部门" }).id).toBe(
            "dept",
        )
        const remove = screen.getByRole("menuitem", { name: "删除" })
        expect(remove.id).toBe("delete")
        expect(remove.getAttribute("data-variant")).toBe("destructive")
    })

    it("只有一个破坏性动作时直接露出，并保留禁用原因", () => {
        render(
            <TableRowActions
                moreId="row-more"
                moreLabel="品牌 更多操作"
                actions={[
                    {
                        id: "disable",
                        label: "停用",
                        destructive: true,
                        disabled: true,
                        disabledReason: "没有停用权限",
                    },
                ]}
            />,
        )

        const disable = screen.getByRole<HTMLButtonElement>("button", {
            name: "停用",
        })
        expect(disable.id).toBe("disable")
        expect(disable.disabled).toBe(true)
        expect(disable.title).toBe("没有停用权限")
        expect(
            screen.queryByRole("button", { name: "品牌 更多操作" }),
        ).toBeNull()
    })

    it("点击露出按钮时阻止冒泡", () => {
        const onEdit = vi.fn()
        render(
            <TableRowActions
                moreId="row-more"
                moreLabel="行 更多操作"
                actions={[{ id: "edit", label: "编辑", onClick: onEdit }]}
            />,
        )
        fireEvent.click(screen.getByRole("button", { name: "编辑" }))
        expect(onEdit).toHaveBeenCalledOnce()
        expect(onEdit.mock.calls[0]?.[0].isPropagationStopped()).toBe(true)
    })

    it("多个描边请求只给第一个露出的动作描边", () => {
        render(
            <TableRowActions
                moreId="row-more"
                moreLabel="行 更多操作"
                actions={[
                    {
                        id: "view",
                        label: "查看",
                        emphasis: "outline",
                    },
                    {
                        id: "adjust",
                        label: "库存调整",
                        emphasis: "outline",
                    },
                ]}
            />,
        )
        expect(
            screen.getByRole("button", { name: "查看" }).className,
        ).toContain("border-border")
        expect(
            screen.getByRole("button", { name: "库存调整" }).className,
        ).not.toContain("border-border")
    })
})
