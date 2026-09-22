import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, expect, it, vi } from "vitest"
import { AccountFormDialog } from "./account-form-dialog"

const mutations = vi.hoisted(() => ({ create: vi.fn(), update: vi.fn() }))
vi.mock("@/features/admin/hooks/queries", () => ({
    useAdminMutations: () => ({
        createAdmin: mutations.create,
        updateAdmin: mutations.update,
        isCreating: false,
        isUpdating: false,
    }),
}))
afterEach(() => {
    cleanup()
    vi.resetAllMocks()
})

it("账号有未保存修改时不能跳去调整部门丢失输入", async () => {
    const adjust = vi.fn()
    render(
        <AccountFormDialog
            mode="edit"
            account={{
                id: "u1",
                account: "sales",
                name: "销售员",
                role_ids: ["role-sales"],
            }}
            roleOptions={[{ id: "role-sales", name: "销售" }]}
            onOpenChange={vi.fn()}
            departmentLabel="销售部"
            onAdjustDepartment={adjust}
        />,
    )
    expect(
        (screen.getByRole("button", { name: "调整部门" }) as HTMLButtonElement)
            .disabled,
    ).toBe(false)
    fireEvent.change(screen.getByRole("textbox", { name: /姓名/ }), {
        target: { value: "销售员甲" },
    })
    await waitFor(() =>
        expect(
            (
                screen.getByRole("button", {
                    name: "调整部门",
                }) as HTMLButtonElement
            ).disabled,
        ).toBe(true),
    )
    expect(screen.getByText("先保存账号修改，再调整部门")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "调整部门" }))
    expect(adjust).not.toHaveBeenCalled()
})
