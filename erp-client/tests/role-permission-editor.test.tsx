import * as React from "react"
import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest"
import { PermissionOptionsPanel } from "@/features/admin/components/roles/permission-panel"
import type { PermissionView } from "@/features/admin/lib/permission-editor"
import { CopyRolePermissions } from "@/features/admin/components/roles/role-permission-dialogs"
import { RoleFormPage } from "@/features/admin/pages/role-form-page"

const mocks = vi.hoisted(() => ({
    update: vi.fn(),
    create: vi.fn(),
    push: vi.fn(),
    wildcard: false,
}))
vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: { permissions: ["role:update", "data_scope:list"] },
    }),
}))
vi.mock("next/navigation", () => ({ useRouter: () => ({ push: mocks.push }) }))
vi.mock("@/features/admin/hooks/queries", () => ({
    useRolesQuery: () => ({
        data: [
            {
                id: "sales",
                name: "销售",
                permissions: mocks.wildcard
                    ? ["*:*"]
                    : ["customer:list", "contract:*"],
                created_at: 0,
            },
        ],
        isPending: false,
        isError: false,
    }),
    useAdminsQuery: () => ({ data: [{ role_ids: ["sales"] }] }),
    useRoleMutations: () => ({
        updateRole: mocks.update,
        createRole: mocks.create,
        isCreating: false,
        isUpdating: false,
    }),
}))
beforeAll(() => {
    globalThis.ResizeObserver = class {
        observe() {}
        unobserve() {}
        disconnect() {}
    }
    HTMLElement.prototype.scrollIntoView = function () {}
    HTMLElement.prototype.hasPointerCapture = function () {
        return false
    }
    HTMLElement.prototype.releasePointerCapture = function () {}
    HTMLElement.prototype.setPointerCapture = function () {}
})
afterEach(() => {
    cleanup()
    vi.clearAllMocks()
    mocks.wildcard = false
})

function Panel() {
    const initial = ["customer:list", "sales_selection_booklet:get"]
    const [selected, setSelected] = React.useState(initial)
    const [view, setView] = React.useState<PermissionView>("all")
    return (
        <PermissionOptionsPanel
            selected={selected}
            initial={initial}
            onChange={setSelected}
            view={view}
            onViewChange={setView}
        />
    )
}

describe("角色权限编辑", () => {
    it("切换模块不丢失勾选，搜索只修改匹配项，支持核对已取消权限", () => {
        render(<Panel />)
        expect(
            screen.getByRole("checkbox", { name: "客户 · 查看列表" }),
        ).toBeTruthy()
        fireEvent.click(screen.getByRole("button", { name: /^销售选品/ }))
        expect(
            screen.queryByRole("checkbox", { name: "客户 · 查看列表" }),
        ).toBeNull()
        expect(
            (
                screen.getByRole("checkbox", {
                    name: "选品册 · 查看详情",
                }) as HTMLInputElement
            ).checked,
        ).toBe(true)
        fireEvent.change(screen.getByRole("searchbox"), {
            target: { value: "复制链接" },
        })
        fireEvent.click(screen.getByRole("button", { name: "选择匹配项" }))
        fireEvent.change(screen.getByRole("searchbox"), {
            target: { value: "" },
        })
        expect(
            (
                screen.getByRole("checkbox", {
                    name: "选品册 · 查看详情",
                }) as HTMLInputElement
            ).checked,
        ).toBe(true)
        expect(
            (
                screen.getByRole("checkbox", {
                    name: "选品册 · 复制链接",
                }) as HTMLInputElement
            ).checked,
        ).toBe(true)
        fireEvent.click(
            screen.getByRole("button", { name: /^客户(?!往来|经营)/ }),
        )
        fireEvent.click(
            screen.getByRole("checkbox", { name: "客户 · 查看列表" }),
        )
        fireEvent.change(
            screen.getByRole("combobox", { name: "权限显示范围" }),
            { target: { value: "changed" } },
        )
        expect(
            (
                screen.getByRole("checkbox", {
                    name: "客户 · 查看列表",
                }) as HTMLInputElement
            ).checked,
        ).toBe(false)
        expect(screen.getByText("移除")).toBeTruthy()
        const controls = Array.from(
            document.querySelectorAll("button,input,select"),
        )
        expect(controls.every((node) => Boolean(node.id))).toBe(true)
        expect(new Set(controls.map((node) => node.id)).size).toBe(
            controls.length,
        )
    })
    it("提交保留目录外授权，保存失败保留编辑结果与错误", async () => {
        mocks.update.mockRejectedValueOnce(new Error("保存暂时失败"))
        render(<RoleFormPage roleId="sales" />)
        fireEvent.click(screen.getByRole("checkbox", { name: "客户 · 新建" }))
        fireEvent.click(screen.getByRole("button", { name: "保存角色" }))
        await waitFor(() => expect(mocks.update).toHaveBeenCalled())
        expect(mocks.update.mock.calls[0]![0]).toEqual({
            id: "sales",
            payload: {
                name: "销售",
                permissions: ["contract:*", "customer:list", "customer:create"],
            },
        })
        await waitFor(() => expect(screen.getByText("保存失败")).toBeTruthy())
        expect(
            (
                screen.getByRole("checkbox", {
                    name: "客户 · 新建",
                }) as HTMLInputElement
            ).checked,
        ).toBe(true)
        expect(mocks.push).not.toHaveBeenCalled()
    })
    it("取消编辑先确认，继续编辑保留修改", () => {
        render(<RoleFormPage roleId="sales" />)
        fireEvent.click(screen.getByRole("checkbox", { name: "客户 · 新建" }))
        fireEvent.click(screen.getByRole("button", { name: "取消" }))
        expect(screen.getByText("放弃未保存的修改？")).toBeTruthy()
        fireEvent.click(screen.getByRole("button", { name: "继续编辑" }))
        expect(
            (
                screen.getByRole("checkbox", {
                    name: "客户 · 新建",
                }) as HTMLInputElement
            ).checked,
        ).toBe(true)
        expect(mocks.push).not.toHaveBeenCalled()
    })
    it("全权角色不可逐项降权，改名保存原样保留全权编码", async () => {
        mocks.wildcard = true
        mocks.update.mockResolvedValueOnce(undefined)
        render(<RoleFormPage roleId="sales" />)
        const checkbox = screen.getByRole("checkbox", {
            name: "客户 · 查看列表",
        }) as HTMLInputElement
        expect(checkbox.checked).toBe(true)
        expect(checkbox.disabled).toBe(true)
        fireEvent.click(screen.getByRole("button", { name: "修改名称" }))
        fireEvent.change(screen.getByRole("textbox", { name: /角色名称/ }), {
            target: { value: "全权管理员" },
        })
        fireEvent.click(screen.getByRole("button", { name: "保存角色" }))
        await waitFor(() =>
            expect(mocks.update).toHaveBeenCalledWith({
                id: "sales",
                payload: { name: "全权管理员", permissions: ["*:*"] },
            }),
        )
        await waitFor(() =>
            expect(mocks.push).toHaveBeenCalledWith(
                "/system/access-audit?view=roles",
            ),
        )
    })
    it("新建角色名称通过校验后提交，所选权限保持原编码", async () => {
        mocks.create.mockResolvedValueOnce(undefined)
        render(<RoleFormPage roleId={null} />)
        fireEvent.change(screen.getByRole("textbox", { name: /角色名称/ }), {
            target: { value: "销售助理" },
        })
        fireEvent.click(
            screen.getByRole("checkbox", { name: "客户 · 查看列表" }),
        )
        fireEvent.click(screen.getByRole("button", { name: "创建角色" }))
        await waitFor(() =>
            expect(mocks.create).toHaveBeenCalledWith({
                name: "销售助理",
                permissions: ["customer:list"],
            }),
        )
    })
    it("选择复制来源不立即改动权限，明确替换后才回传可配置项", () => {
        const onCopy = vi.fn()
        render(
            <CopyRolePermissions
                disabled={false}
                currentCount={3}
                onCopy={onCopy}
                roles={[
                    {
                        id: "copy",
                        name: "销售助理",
                        permissions: ["customer:list", "contract:*"],
                        created_at: 0,
                    },
                    {
                        id: "root",
                        name: "全权管理员",
                        permissions: ["*:*"],
                        created_at: 0,
                    },
                ]}
            />,
        )
        fireEvent.click(screen.getByRole("button", { name: "从其他角色复制" }))
        const combo = screen.getByRole("combobox", {
            name: "复制权限的来源角色",
        })
        combo.focus()
        fireEvent.keyDown(combo, { key: "ArrowDown" })
        expect(screen.queryByRole("option", { name: "全权管理员" })).toBeNull()
        fireEvent.click(screen.getByRole("option", { name: "销售助理" }))
        expect(onCopy).not.toHaveBeenCalled()
        fireEvent.click(screen.getByRole("button", { name: "替换当前勾选" }))
        expect(onCopy).toHaveBeenCalledWith(["customer:list"])
    })
})
