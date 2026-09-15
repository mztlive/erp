import { afterEach, beforeEach, expect, it, vi } from "vitest"
import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { PublicSelectionPage } from "./public-selection-page"
import type { PublicPageView } from "../types"
const mocks = vi.hoisted(() => ({
    query: vi.fn(),
    save: vi.fn(),
    submit: vi.fn(),
}))
vi.mock("../queries", () => ({
    usePublicSelectionQuery: mocks.query,
    salesSelectionKeys: { public: (token: string) => ["selection", token] },
}))
vi.mock("../api", () => ({
    savePublicSession: mocks.save,
    submitPublicSession: mocks.submit,
    publicImageUrl: () => undefined,
}))
const page: PublicPageView = {
    kind: "SELECTING",
    customer_name: "测试客户",
    form: "SINGLE_SKU",
    submit_mode: "BY_QUANTITY",
    session_version: 1,
    items: ["A", "B"].map((id) => ({
        item_id: id,
        name: `商品${id}`,
        price: "10.00",
        specification: [],
        members: [],
    })),
    choices: [],
    notices: [],
}
/** 模拟不同设备保存后的最新清单。 */
const mount = () => {
    mocks.query.mockReturnValue({
        data: page,
        refetch: vi.fn().mockResolvedValue({
            data: {
                ...page,
                session_version: 3,
                choices: [{ item_id: "B", quantity: 2 }],
            },
        }),
    })
    return render(
        <QueryClientProvider
            client={
                new QueryClient({
                    defaultOptions: { mutations: { retry: false } },
                })
            }
        >
            <PublicSelectionPage token="test-token" />
        </QueryClientProvider>,
    )
}
/** 点击真实表单控件。 */
const click = (name: string) =>
    fireEvent.click(screen.getByRole("button", { name }))
beforeEach(() => {
    vi.resetAllMocks()
    sessionStorage.clear()
})
afterEach(cleanup)
it("核对后改选撤销确认，重新保存屏幕中的选择再提交", async () => {
    mocks.save.mockImplementation(async (_token, input) => ({
        ...page,
        session_version: input.expectedSessionVersion + 1,
        choices: input.choices,
        total_amount: "10.00",
    }))
    mocks.submit.mockResolvedValue({ ...page, kind: "ENDED" })
    mount()
    fireEvent.click(screen.getByRole("checkbox", { name: /商品A/ }))
    click("核对并提交")
    await screen.findByRole("button", { name: "确认并提交选品" })
    await waitFor(() =>
        expect(
            (
                screen.getByRole("checkbox", {
                    name: /商品A/,
                }) as HTMLInputElement
            ).checked,
        ).toBe(true),
    )
    fireEvent.click(screen.getByRole("checkbox", { name: /商品A/ }))
    await waitFor(() =>
        expect(
            (
                screen.getByRole("checkbox", {
                    name: /商品A/,
                }) as HTMLInputElement
            ).checked,
        ).toBe(false),
    )
    fireEvent.click(screen.getByRole("checkbox", { name: /商品B/ }))
    expect(screen.queryByRole("button", { name: "确认并提交选品" })).toBeNull()
    click("核对并提交")
    fireEvent.click(
        await screen.findByRole("button", { name: "确认并提交选品" }),
    )
    await waitFor(() => expect(mocks.submit).toHaveBeenCalledTimes(1))
    expect(mocks.save.mock.calls[1][1].choices).toEqual([
        { item_id: "B", quantity: 1 },
    ])
    expect(mocks.submit.mock.calls[0][1].expectedSessionVersion).toBe(3)
})
it("未知结果锁定编辑并用相同幂等键和载荷恢复", async () => {
    mocks.save.mockRejectedValueOnce({ kind: "Network" }).mockResolvedValue({
        ...page,
        session_version: 2,
        choices: [{ item_id: "A", quantity: 1 }],
    })
    mount()
    fireEvent.click(screen.getByRole("checkbox", { name: /商品A/ }))
    click("保存选择")
    const retry = await screen.findByRole("button", {
        name: "核对并恢复本次操作",
    })
    expect(
        screen.getByRole("checkbox", { name: /商品A/ }).closest("fieldset")
            ?.disabled,
    ).toBe(true)
    expect(sessionStorage.length).toBe(1)
    fireEvent.click(retry)
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(2))
    expect(mocks.save.mock.calls[1]).toEqual(mocks.save.mock.calls[0])
    await waitFor(() => expect(sessionStorage.length).toBe(0))
})
it("冲突后展示最新清单，保留本地选择并要求明确核对", async () => {
    mocks.save.mockRejectedValueOnce({ status: 409 }).mockResolvedValue({
        ...page,
        session_version: 4,
        choices: [{ item_id: "A", quantity: 1 }],
    })
    mount()
    fireEvent.click(screen.getByRole("checkbox", { name: /商品A/ }))
    click("核对并提交")
    const acknowledge = await screen.findByRole("button", {
        name: "已核对，保留本地选择继续编辑",
    })
    await waitFor(() =>
        expect((acknowledge as HTMLButtonElement).disabled).toBe(false),
    )
    expect(
        (screen.getByRole("checkbox", { name: /商品A/ }) as HTMLInputElement)
            .checked,
    ).toBe(true)
    expect(screen.getByText("商品B × 2 份")).toBeTruthy()
    fireEvent.click(acknowledge)
    click("核对并提交")
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(2))
    expect(mocks.save.mock.calls[1][1]).toMatchObject({
        expectedSessionVersion: 3,
        choices: [{ item_id: "A", quantity: 1 }],
    })
})
it("失效链接显示结束说明，网络故障提供重试", () => {
    mocks.query.mockReturnValue({
        isError: true,
        error: { status: 404 },
        refetch: vi.fn(),
    })
    const view = render(<PublicSelectionPage token="old" />)
    expect(screen.getByText("选品链接已失效")).toBeTruthy()
    mocks.query.mockReturnValue({
        isError: true,
        error: { kind: "Network" },
        refetch: vi.fn(),
    })
    view.rerender(<PublicSelectionPage token="offline" />)
    expect(screen.getByRole("button", { name: "重新打开" })).toBeTruthy()
})
it("在核对中心移除单项不返回上一页，自动保存并可直接提交", async () => {
    mocks.save.mockImplementation(async (_token, input) => ({
        ...page,
        session_version: input.expectedSessionVersion + 1,
        choices: input.choices,
        total_amount: "20.00",
    }))
    mocks.submit.mockResolvedValue({ ...page, kind: "ENDED" })
    mount()
    fireEvent.click(screen.getByRole("checkbox", { name: /商品A/ }))
    fireEvent.click(screen.getByRole("checkbox", { name: /商品B/ }))
    click("核对并提交")
    await screen.findByRole("button", { name: "确认并提交选品" })
    const removeButtons = screen.getAllByRole("button", { name: "移除" })
    expect(removeButtons.length).toBeGreaterThan(0)
    fireEvent.click(removeButtons[0])
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(2))
    expect(screen.getByRole("button", { name: "确认并提交选品" })).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "确认并提交选品" }))
    await waitFor(() => expect(mocks.submit).toHaveBeenCalledTimes(1))
    expect(mocks.save.mock.calls[1][1].choices).toEqual([
        { item_id: "B", quantity: 1 },
    ])
})
