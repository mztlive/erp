import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"

const api = vi.hoisted(() => ({
    policy: vi.fn(),
    view: vi.fn(),
    current: vi.fn(),
    history: vi.fn(),
}))
const navigation = vi.hoisted(() => ({
    params: new URLSearchParams(),
    router: { push: vi.fn(), replace: vi.fn() },
}))

vi.mock("next/navigation", () => ({
    useSearchParams: () => navigation.params,
    usePathname: () => "/analytics/customer-quality",
    useRouter: () => navigation.router,
}))
vi.mock("../api/customer-quality", () => ({
    fetchCustomerQualityPeriodPolicy: api.policy,
    fetchCustomerQuality: api.view,
    startCustomerQualityExport: vi.fn(),
}))
vi.mock("../api/dual-caliber", () => ({
    fetchCurrentQuality: api.current,
    fetchHistoryQuality: api.history,
    exportCurrentQuality: vi.fn(),
    exportHistoryQuality: vi.fn(),
    downloadQualityCsv: vi.fn(),
}))

import { CustomerQualityPage } from "./customer-quality-page"

beforeEach(() => {
    vi.stubGlobal("localStorage", {
        getItem: vi.fn(() => null),
        setItem: vi.fn(),
        removeItem: vi.fn(),
    })
})

afterEach(() => {
    cleanup()
    vi.clearAllMocks()
    navigation.params = new URLSearchParams()
    vi.unstubAllGlobals()
})

test("期间配置请求失败时退出骨架屏，保留页面标题和可重试入口", async () => {
    api.policy.mockRejectedValue(new Error("期间配置暂不可用"))
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    })
    render(
        <QueryClientProvider client={client}>
            <CustomerQualityPage />
        </QueryClientProvider>,
    )
    const retry = await screen.findByRole("button", {
        name: "重试",
    })
    expect(screen.getByRole("heading", { name: "客户经营质量" })).toBeTruthy()
    expect(screen.getByText("期间配置加载失败")).toBeTruthy()
    expect(retry.id).toBe("customers-quality-period-policy-retry")
    expect(api.view).not.toHaveBeenCalled()
    expect(api.current).not.toHaveBeenCalled()
    expect(api.history).not.toHaveBeenCalled()
    fireEvent.click(retry)
    await waitFor(() => expect(api.policy).toHaveBeenCalledTimes(2))
    client.clear()
})

test.each([
    ["current", "当前负责口径加载失败"],
    ["history", "历史贡献口径加载失败"],
] as const)(
    "期间配置失败后仍按显式期间执行 %s 真实查询分支并保留故障提示",
    async (caliber, queryFailureTitle) => {
        navigation.params = new URLSearchParams({
            from: "2026-10-01",
            to: "2026-10-03",
            caliber,
        })
        api.policy.mockRejectedValue(new Error("期间配置暂不可用"))
        api.view.mockRejectedValue(new Error("经营视图暂不可用"))
        api.current.mockRejectedValue(new Error("当前口径暂不可用"))
        api.history.mockRejectedValue(new Error("历史口径暂不可用"))
        const client = new QueryClient({
            defaultOptions: { queries: { retry: false } },
        })
        render(
            <QueryClientProvider client={client}>
                <CustomerQualityPage />
            </QueryClientProvider>,
        )
        await screen.findByText(queryFailureTitle)
        expect(screen.getByText("期间配置加载失败")).toBeTruthy()
        expect(
            screen.getByRole("region", { name: "客户经营质量双口径" }),
        ).toBeTruthy()
        expect(api[caliber]).toHaveBeenCalledWith(
            expect.objectContaining({
                from: "2026-10-01",
                to: "2026-10-03",
            }),
        )
        expect(
            api[caliber === "current" ? "history" : "current"],
        ).not.toHaveBeenCalled()
        client.clear()
    },
)
