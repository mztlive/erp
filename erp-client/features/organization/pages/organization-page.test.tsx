import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vitest"

const navigation = vi.hoisted(() => ({
    params: new URLSearchParams(),
    router: { replace: vi.fn(), push: vi.fn() },
}))
const queries = vi.hoisted(() => ({
    profile: {
        isPending: false,
        data: { permissions: ["org_unit:list", "org_unit:manage"] },
    },
    state: {
        isPending: false,
        isError: false,
        isFetching: false,
        data: undefined as
            | {
                  version: number
                  organizationVersion: number
                  policyVersion: number
                  scopeVersion: string
                  asOf: string
                  emptyReason: "no_scope" | null
                  scopeSummary: string
                  ownershipBasis: string
                  people: never[]
                  roles: never[]
                  units: never[]
                  memberships: never[]
                  management: never[]
              }
            | undefined,
        error: undefined as { status?: number; message?: string } | undefined,
        refetch: vi.fn(),
    },
}))

vi.mock("next/navigation", () => ({
    useSearchParams: () => navigation.params,
    usePathname: () => "/system/organization",
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
        useOrganizationStateQuery: () => queries.state,
        usePreviewOrganizationChangeMutation: () => ({
            isPending: false,
            mutateAsync: vi.fn(),
        }),
        useSubmitOrganizationChangeMutation: () => ({
            isPending: false,
            mutateAsync: vi.fn(),
        }),
    }
})

import { OrganizationPage } from "./organization-page"

function renderPage() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    })
    return render(
        <QueryClientProvider client={client}>
            <OrganizationPage />
        </QueryClientProvider>,
    )
}

afterEach(() => {
    cleanup()
    navigation.params = new URLSearchParams()
    queries.profile = {
        isPending: false,
        data: { permissions: ["org_unit:list", "org_unit:manage"] },
    }
    queries.state = {
        isPending: false,
        isError: false,
        isFetching: false,
        data: undefined,
        error: undefined,
        refetch: vi.fn(),
    }
})

it("权限未决时显示加载，不闪范围内无记录", () => {
    queries.profile = { isPending: true, data: { permissions: [] } }
    queries.state = { ...queries.state, isPending: true }
    renderPage()
    expect(screen.queryByText("范围内无记录")).toBeNull()
    expect(screen.queryByText("无模块权限")).toBeNull()
})

it("无 org_unit:list 的 403 呈现无模块权限，而不是范围内无记录", () => {
    queries.profile = { isPending: false, data: { permissions: [] } }
    queries.state = {
        ...queries.state,
        isError: true,
        error: { status: 403, message: "没有该资源动作权限" },
    }
    renderPage()
    expect(screen.getByText("无模块权限")).toBeTruthy()
    expect(screen.queryByText("范围内无记录")).toBeNull()
    expect(screen.queryByText("组织列表加载失败")).toBeNull()
})

it("后端 empty_reason=no_scope 呈现无数据范围", () => {
    queries.state = {
        ...queries.state,
        data: {
            version: 1,
            organizationVersion: 1,
            policyVersion: 1,
            scopeVersion: "v",
            asOf: "2026-09-15T00:00:00Z",
            emptyReason: "no_scope",
            scopeSummary: "组织配置边界内的内部组织、成员与管理关系",
            ownershipBasis: "org_unit_configuration",
            people: [],
            roles: [],
            units: [],
            memberships: [],
            management: [],
        },
    }
    renderPage()
    expect(screen.getByText("无数据范围")).toBeTruthy()
    expect(screen.queryByText("范围内无记录")).toBeNull()
})
