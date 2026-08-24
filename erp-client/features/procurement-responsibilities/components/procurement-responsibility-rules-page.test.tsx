import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

const state = vi.hoisted(() => ({
    permissions: [
        "procurement_responsibility:list",
        "procurement_responsibility:manage",
    ],
}))

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: { permissions: state.permissions },
        isPending: false,
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
    useAdminsQuery: () => ({ data: [] }),
}))
vi.mock("@/features/master-data/hooks/queries", () => ({
    useMasterDataListQuery: () => ({ data: { rows: [] } }),
}))

import { ProcurementResponsibilityRulesPage } from "@/features/procurement-responsibilities/components/procurement-responsibility-rules-page"

afterEach(() => {
    cleanup()
    state.permissions = [
        "procurement_responsibility:list",
        "procurement_responsibility:manage",
    ]
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
})
