import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

if (typeof globalThis.ResizeObserver === "undefined") {
    class ResizeObserverStub {
        observe() {}
        unobserve() {}
        disconnect() {}
    }
    globalThis.ResizeObserver =
        ResizeObserverStub as unknown as typeof ResizeObserver
}
if (!Element.prototype.scrollIntoView) {
    Element.prototype.scrollIntoView = () => {}
}

const state = vi.hoisted(() => ({
    permissions: [
        "procurement_responsibility:list",
        "procurement_responsibility:manage",
    ],
    profileError: false,
    dependencyError: false,
}))

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: state.profileError
            ? undefined
            : { permissions: state.permissions },
        isPending: false,
        isError: state.profileError,
        error: state.profileError ? new Error("profile failed") : null,
        refetch: vi.fn(),
    }),
}))
vi.mock("@/features/procurement-responsibilities/queries", () => ({
    useProcurementResponsibilityRulesQuery: () => ({
        data: [
            {
                ruleId: "rule-1",
                ruleType: "DEFAULT_DISPATCHER",
                ownerUserId: "buyer-1",
                ownerName: "采购李四",
                enabled: true,
                version: 1,
            },
        ],
        isPending: false,
        isError: false,
        refetch: vi.fn(),
    }),
    useSaveProcurementResponsibilityRuleMutation: () => ({
        mutateAsync: vi.fn(),
        isPending: false,
    }),
}))
vi.mock("@/features/admin/hooks/queries", () => ({
    useAdminsQuery: () => ({
        data: state.dependencyError ? undefined : [],
        isPending: false,
        isError: state.dependencyError,
        error: state.dependencyError ? new Error("admins failed") : null,
        refetch: vi.fn(),
    }),
}))
vi.mock("@/features/master-data/hooks/queries", () => ({
    useMasterDataListQuery: () => ({
        data: state.dependencyError ? undefined : { rows: [] },
        isPending: false,
        isError: state.dependencyError,
        error: state.dependencyError ? new Error("categories failed") : null,
        refetch: vi.fn(),
    }),
}))

import { ProcurementResponsibilityRulesPage } from "@/features/procurement-responsibilities/components/procurement-responsibility-rules-page"

afterEach(() => {
    cleanup()
    state.permissions = [
        "procurement_responsibility:list",
        "procurement_responsibility:manage",
    ]
    state.profileError = false
    state.dependencyError = false
})

describe("ProcurementResponsibilityRulesPage", () => {
    it("renders enabled rules and manage test selectors", () => {
        render(<ProcurementResponsibilityRulesPage />)

        expect(
            screen
                .getByTestId("procurement-responsibility-rules")
                .textContent?.includes("默认调度人"),
        ).toBe(true)
        expect(screen.getByText("采购李四")).toBeTruthy()
        expect(screen.getByText("已启用")).toBeTruthy()
        expect(
            screen.getByTestId("procurement-responsibility-create"),
        ).toBeTruthy()
        expect(screen.queryByRole("button", { name: "编辑" })).toBeNull()
        expect(screen.queryByRole("columnheader", { name: "操作" })).toBeNull()
    })

    it("keeps the list read-only without manage permission", () => {
        state.permissions = ["procurement_responsibility:list"]
        render(<ProcurementResponsibilityRulesPage />)

        expect(
            screen.getByTestId("procurement-responsibility-rules"),
        ).toBeTruthy()
        expect(
            screen.queryByTestId("procurement-responsibility-create"),
        ).toBeNull()
        expect(screen.queryByRole("button", { name: "编辑" })).toBeNull()
    })

    it("reports profile loading failures instead of showing permission denied", () => {
        state.profileError = true
        render(<ProcurementResponsibilityRulesPage />)

        expect(screen.getByText("权限信息加载失败")).toBeTruthy()
        expect(screen.queryByText("权限不足")).toBeNull()
    })

    it("keeps rule editing disabled when owner or category dependencies fail", () => {
        state.dependencyError = true
        render(<ProcurementResponsibilityRulesPage />)

        expect(screen.getByText("规则编辑依赖加载失败")).toBeTruthy()
        expect(
            (
                screen.getByTestId(
                    "procurement-responsibility-create",
                ) as HTMLButtonElement
            ).disabled,
        ).toBe(true)
    })
})
