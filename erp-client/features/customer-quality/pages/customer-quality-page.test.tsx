import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

const api = vi.hoisted(() => ({
    policy: vi.fn(),
    view: vi.fn(),
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

import { CustomerQualityPage } from "./customer-quality-page"

afterEach(() => {
    cleanup()
    vi.clearAllMocks()
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
    fireEvent.click(retry)
    await waitFor(() => expect(api.policy).toHaveBeenCalledTimes(2))
    client.clear()
})
