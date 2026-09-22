import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeAll, expect, it, vi } from "vitest"
import type { OrganizationStateView } from "@/features/organization/types"

const state = vi.hoisted(() => ({
    permissions: ["admin:list", "org_unit:list", "org_unit:manage"],
    org: undefined as OrganizationStateView | undefined,
    error: false,
    enabled: vi.fn(),
    params: new URLSearchParams(),
}))
vi.mock("next/navigation", () => ({
    useRouter: () => ({ push: vi.fn(), replace: vi.fn() }),
    useSearchParams: () => state.params,
}))
vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: { permissions: state.permissions },
    }),
}))
vi.mock("@/features/admin/hooks/queries", () => ({
    useAdminsQuery: () => ({
        data: [
            {
                id: "u1",
                account: "xiaoshou",
                name: "销售员",
                role_ids: ["sales"],
                created_at: 1,
            },
        ],
    }),
    useRolesQuery: () => ({ data: [{ id: "sales", name: "销售" }] }),
    useAssignableRolesQuery: () => ({ data: [] }),
}))
vi.mock("@/features/organization/hooks/queries", () => ({
    useOrganizationStateQuery: (enabled: boolean) => {
        state.enabled(enabled)
        return {
            data: state.org,
            isError: state.error,
            error: new Error("加载失败"),
            refetch: vi.fn(),
        }
    },
    usePreviewOrganizationChangeMutation: () => ({
        isPending: false,
        mutateAsync: vi.fn(),
    }),
    useSubmitOrganizationChangeMutation: () => ({
        isPending: false,
        mutateAsync: vi.fn(),
    }),
}))
vi.mock(
    "@/features/admin/components/accounts/account-permissions-sheet",
    () => ({ AccountPermissionsSheet: () => null }),
)
import { AccountsPage } from "./accounts-page"

beforeAll(() => {
    globalThis.ResizeObserver = class {
        observe() {}
        unobserve() {}
        disconnect() {}
    }
    HTMLElement.prototype.scrollIntoView = function () {}
})
afterEach(() => {
    cleanup()
    state.permissions = ["admin:list", "org_unit:list", "org_unit:manage"]
    state.org = undefined
    state.error = false
    vi.clearAllMocks()
})
function show() {
    return render(
        <QueryClientProvider client={new QueryClient()}>
            <AccountsPage />
        </QueryClientProvider>,
    )
}
function organization(): OrganizationStateView {
    return {
        version: 1,
        organizationVersion: 1,
        policyVersion: 1,
        scopeVersion: "v1",
        asOf: "2026-09-22T00:00:00Z",
        emptyReason: null,
        scopeSummary: "",
        ownershipBasis: "",
        units: [],
        memberships: [],
        management: [],
        roles: [],
        people: [
            {
                id: "u1",
                label: "销售员",
                account: "xiaoshou",
                active: true,
                own_org_unit_id: null,
            },
        ],
    }
}

it("未分配人员可从账号行直接打开固定人员的部门表单", () => {
    state.org = organization()
    show()
    expect(screen.getByText("未分配部门")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "销售员 更多操作" }))
    fireEvent.click(screen.getByRole("menuitem", { name: "调整部门" }))
    expect(screen.getByRole("heading", { name: "调整所属部门" })).toBeTruthy()
    expect(screen.getByText(/销售员：未分配部门 → 请选择部门/)).toBeTruthy()
})
it("无组织读取权不请求部门，也不误报未分配", () => {
    state.permissions = ["admin:list"]
    show()
    expect(state.enabled).toHaveBeenCalledWith(false)
    expect(screen.getByText("无部门查看权限")).toBeTruthy()
    expect(screen.queryByText("未分配部门")).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "销售员 更多操作" }))
    expect(screen.queryByRole("menuitem", { name: "调整部门" })).toBeNull()
})
it("组织范围不含该人员时不展示写入口", () => {
    state.org = { ...organization(), people: [] }
    show()
    expect(screen.getByText("不在可查看范围")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "销售员 更多操作" }))
    expect(screen.queryByRole("menuitem", { name: "调整部门" })).toBeNull()
})
it("部门加载失败保留账号列表并提供重试", () => {
    state.error = true
    show()
    expect(screen.getByText("xiaoshou")).toBeTruthy()
    expect(screen.getByText("部门信息加载失败")).toBeTruthy()
    expect(screen.queryByText("未分配部门")).toBeNull()
})
