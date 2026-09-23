import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vitest"

const navigation = vi.hoisted(() => ({
    params: new URLSearchParams(
        "subjectType=role&subjectId=role-sales&scopeType=company",
    ),
    router: { replace: vi.fn(), push: vi.fn() },
}))
const queries = vi.hoisted(() => ({
    profile: {
        isPending: false,
        data: { permissions: ["data_scope:list"] },
    },
    scopes: {
        isPending: false,
        isError: false,
        isFetching: false,
        data: {
            items: [],
            total: 0,
            emptyReason: null as "no_scope" | null,
        },
        error: undefined as { status?: number; message?: string } | undefined,
        refetch: vi.fn(),
    },
    org: {
        data: {
            people: [],
            roles: [{ id: "role-sales", name: "销售", enabled: true }],
            units: [],
        },
    },
}))

vi.mock("next/navigation", () => ({
    useSearchParams: () => navigation.params,
    usePathname: () => "/system/organization/scopes",
    useRouter: () => navigation.router,
}))
vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => queries.profile,
}))
vi.mock("@/features/organization/hooks/queries", async () => {
    const actual = await vi.importActual<
        typeof import("@/features/organization/hooks/queries")
    >("@/features/organization/hooks/queries")
    return {
        ...actual,
        useDataScopesQuery: () => queries.scopes,
        useOrganizationStateQuery: () => queries.org,
        useCreateDataScopeMutation: () => ({
            isPending: false,
            mutateAsync: vi.fn(),
        }),
        useDeleteDataScopeMutation: () => ({
            isPending: false,
            mutateAsync: vi.fn(),
        }),
    }
})

import { DataScopesPage } from "./data-scopes-page"

function renderPage() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    })
    return render(
        <QueryClientProvider client={client}>
            <DataScopesPage />
        </QueryClientProvider>,
    )
}

afterEach(() => {
    cleanup()
    navigation.params = new URLSearchParams(
        "subjectType=role&subjectId=role-sales&scopeType=company",
    )
    queries.profile = {
        isPending: false,
        data: { permissions: ["data_scope:list"] },
    }
    queries.scopes = {
        isPending: false,
        isError: false,
        isFetching: false,
        data: { items: [], total: 0, emptyReason: null },
        error: undefined,
        refetch: vi.fn(),
    }
})

it("主体与范围类型出现在可见筛选和条件芯片中，可单独清除", () => {
    renderPage()
    fireEvent.click(screen.getByRole("button", { name: /^更多筛选/ }))
    expect(screen.getByLabelText("主体")).toBeTruthy()
    expect(screen.getByLabelText("范围类型")).toBeTruthy()
    expect(screen.getByText("主体：销售")).toBeTruthy()
    expect(screen.getByText("公司级")).toBeTruthy()
    expect(screen.getByRole("button", { name: "重置" })).toBeTruthy()
})

it("无 data_scope:list 的 403 呈现无模块权限", () => {
    navigation.params = new URLSearchParams()
    queries.profile = { isPending: false, data: { permissions: [] } }
    queries.scopes = {
        ...queries.scopes,
        isError: true,
        data: { items: [], total: 0, emptyReason: null },
        error: { status: 403, message: "没有该资源动作权限" },
    }
    renderPage()
    expect(screen.getByText("无模块权限")).toBeTruthy()
    expect(screen.queryByText("范围内无记录")).toBeNull()
})
