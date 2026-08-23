import { act, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { useAccessAuditPage } from "./use-access-audit-page"
import * as api from "@/features/access-audit/api"
import type { AccessListView } from "@/features/access-audit/types"

let currentSearchParams = ""
let mockReplace: ReturnType<typeof vi.fn>
let mockPush: ReturnType<typeof vi.fn>

vi.mock("next/navigation", () => ({
    useRouter: () => ({ push: mockPush, replace: mockReplace, back: vi.fn() }),
    useSearchParams: () => new URLSearchParams(currentSearchParams),
    usePathname: () => "/system/access-audit",
    useParams: () => ({}),
}))

vi.mock("@/features/access-audit/api", () => ({
    fetchAccessList: vi.fn(),
    fetchAuditEvent: vi.fn(),
    fetchEffectiveAccess: vi.fn(),
    previewAccessChange: vi.fn(),
    submitAccessChange: vi.fn(),
}))

vi.mock("@/features/admin/queries", () => ({
    useAssignableRolesQuery: () => ({ data: undefined }),
}))

const governancePolicies = {
    userRoleTimePolicy: {
        state: "MISSING",
        allowedActions: ["EMERGENCY_REVOKE_USER_ROLE"],
        blockerCode: "USER_ROLE_TIME_POLICY_MISSING",
    },
    fieldPolicyGranularity: {
        state: "MISSING",
        editable: false,
        blockerCode: "FIELD_POLICY_GRANULARITY_MISSING",
    },
    auditAccessPolicy: {
        state: "MISSING",
        fallbackFrom: "2026-01-01T00:00:00.000Z",
        fallbackTo: "2026-01-01T02:00:00.000Z",
        configurationExportAllowed: false,
        auditExportAllowed: false,
        blockerCode: "AUDIT_ACCESS_POLICY_MISSING",
    },
} as const

const makeListView = (overrides: Partial<AccessListView> = {}): AccessListView => ({
    view: "roles",
    permissionVersion: "pv-live",
    watermark: "w19-2026-01-01T00:00:00.000Z",
    calculatedAt: "2026-01-01T00:00:00.000Z",
    metrics: {
        roleCount: 0,
        userCount: 0,
        scopeCount: 0,
        fieldPolicyCount: 0,
        auditEventCount: 0,
    },
    governancePolicies,
    roles: [],
    users: [],
    scopes: [],
    fieldPolicies: [],
    auditEvents: [],
    allowedActions: ["VIEW_EFFECTIVE_ACCESS"],
    actionBlockers: [],
    workItemSupport: "DISABLED",
    ...overrides,
})

function renderPageHook() {
    const client = createFreshQueryClient()
    const view = renderHookWithProviders(() => useAccessAuditPage(), {
        queryClient: client,
    })
    return { client, view }
}

beforeEach(() => {
    currentSearchParams = ""
    mockPush = vi.fn()
    mockReplace = vi.fn()
    vi.clearAllMocks()
    vi.mocked(api.fetchAccessList).mockResolvedValue(makeListView())
    vi.mocked(api.fetchEffectiveAccess).mockResolvedValue(null)
    vi.mocked(api.fetchAuditEvent).mockResolvedValue(null)
})

describe("useAccessAuditPage", () => {
    it("parses view/q/page from the URL and initializes search input and pagination", async () => {
        currentSearchParams = "view=users&q=abc&page=2"
        const { view } = renderPageHook()

        expect(view.result.current.view).toBe("users")
        expect(view.result.current.isAudit).toBe(false)
        expect(view.result.current.searchDraft).toBe("abc")
        expect(view.result.current.pagination).toEqual({
            pageIndex: 1,
            pageSize: 20,
        })
        await waitFor(() => expect(view.result.current.pageQuery.isSuccess).toBe(true))
        expect(api.fetchAccessList).toHaveBeenCalledWith(
            expect.objectContaining({
                view: "users",
                q: "abc",
            }),
        )
        expect(view.result.current.pageQuery.data?.view).toBe("roles")
    })

    it("defaults to the roles view for a missing or unknown view param", async () => {
        currentSearchParams = "view=unknown"
        const { view } = renderPageHook()

        expect(view.result.current.view).toBe("roles")
        await waitFor(() => expect(view.result.current.pageQuery.isSuccess).toBe(true))
        expect(api.fetchAccessList).toHaveBeenCalledWith(
            expect.objectContaining({ view: "roles" }),
        )
    })

    it("keeps subject params out of the list query and only opens the detail sheet", async () => {
        currentSearchParams = "view=users&subjectType=USER&subjectId=u1"
        const { view } = renderPageHook()

        expect(view.result.current.explainSubject).toEqual({
            type: "USER",
            id: "u1",
        })
        await waitFor(() => expect(view.result.current.pageQuery.isSuccess).toBe(true))
        expect(api.fetchAccessList).toHaveBeenCalledWith(
            expect.objectContaining({
                view: "users",
                subjectType: undefined,
                subjectId: undefined,
            }),
        )
    })

    it("derives hasActiveFilters from the active view's params", () => {
        currentSearchParams = "view=users&q=abc"
        const { view } = renderPageHook()
        expect(view.result.current.hasActiveFilters).toBe(true)

        currentSearchParams = "view=audit&actorId=a1"
        const audit = renderPageHook()
        expect(audit.view.result.current.hasActiveFilters).toBe(true)
        expect(audit.view.result.current.hasStructuredFilters).toBe(true)
    })

    it("applies the search draft to the URL only through applyFilters", () => {
        currentSearchParams = "view=roles"
        const { view } = renderPageHook()

        act(() => {
            view.result.current.setSearchDraft("新关键词")
        })
        // Draft 变化不写 URL
        expect(mockReplace).not.toHaveBeenCalled()

        act(() => {
            view.result.current.applyFilters()
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/system/access-audit?view=roles&q=%E6%96%B0%E5%85%B3%E9%94%AE%E8%AF%8D",
            { scroll: false },
        )
        expect(view.result.current.panelOpen).toBe(false)
    })

    it("applies panel drafts to the URL through applyFilters and opens the panel for structured deep links", () => {
        currentSearchParams = "view=audit"
        const { view } = renderPageHook()
        expect(view.result.current.panelOpen).toBe(false)

        act(() => view.result.current.updateDraft("actorId", "张三"))
        act(() => view.result.current.updateDraft("result", "DENIED"))
        act(() => view.result.current.applyFilters())
        expect(mockReplace).toHaveBeenCalledWith(
            "/system/access-audit?view=audit&result=DENIED&actorId=%E5%BC%A0%E4%B8%89",
            { scroll: false },
        )
        expect(view.result.current.panelOpen).toBe(false)

        // 带结构化条件的初始深链展开面板
        currentSearchParams = "view=audit&result=SUCCESS"
        const deepLink = renderPageHook()
        expect(deepLink.view.result.current.panelOpen).toBe(true)
    })

    it("builds applied chips for every active filter", () => {
        currentSearchParams =
            "view=audit&q=abc&actorId=a1&result=DENIED&action=user_role.revoke"
        const { view } = renderPageHook()

        const keys = view.result.current.appliedChips.map((chip) => chip.key)
        expect(keys).toEqual(["q", "action", "result", "actorId"])
        expect(
            view.result.current.appliedChips.find(
                (chip) => chip.key === "action",
            )?.label,
        ).toBe("动作：用户角色 · 撤权")
        expect(view.result.current.hasActiveFilters).toBe(true)

        act(() => {
            view.result.current.removeFilter("result")
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/system/access-audit?view=audit&q=abc&actorId=a1&action=user_role.revoke",
            { scroll: false },
        )
    })
    it("updates pagination and URL on page change; page 1 removes the param", () => {
        currentSearchParams = "view=roles"
        const { view } = renderPageHook()

        act(() => {
            view.result.current.handlePaginationChange({
                pageIndex: 1,
                pageSize: 20,
            })
        })
        expect(view.result.current.pagination.pageIndex).toBe(1)
        expect(mockReplace).toHaveBeenCalledWith(
            "/system/access-audit?view=roles&page=2",
        )

        act(() => {
            view.result.current.handlePaginationChange({
                pageIndex: 0,
                pageSize: 20,
            })
        })
        expect(mockReplace).toHaveBeenLastCalledWith(
            "/system/access-audit?view=roles",
        )
    })

    it("switches views by patching the URL, resetting pagination and clearing audit filters", () => {
        currentSearchParams = "view=roles"
        const { view } = renderPageHook()

        act(() => {
            view.result.current.switchView("audit")
        })
        expect(mockPush).toHaveBeenCalledWith(
            "/system/access-audit?view=audit",
        )
        expect(view.result.current.pagination.pageIndex).toBe(0)

        currentSearchParams = "view=audit&actorId=a1&action=QUERY_AUDIT"
        const audit = renderPageHook()
        act(() => {
            audit.view.result.current.switchView("roles")
        })
        expect(mockPush).toHaveBeenCalledWith(
            "/system/access-audit?view=roles",
        )
    })

    it("clears all filters through clearFilters", () => {
        currentSearchParams = "view=audit&q=x&actorId=a1&result=DENIED"
        const { view } = renderPageHook()

        act(() => {
            view.result.current.clearFilters()
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/system/access-audit?view=audit",
            { scroll: false },
        )
        expect(view.result.current.searchDraft).toBe("")
        expect(view.result.current.pagination.pageIndex).toBe(0)
    })

    it("flags export as blocked when the data does not allow export actions", async () => {
        currentSearchParams = "view=roles"
        const { view } = renderPageHook()

        await waitFor(() => expect(view.result.current.pageQuery.isSuccess).toBe(true))
        expect(view.result.current.exportBlocked).toBe(true)

        act(() => {
            view.result.current.handleExport()
        })
        expect(view.result.current.actionError).toBe(
            "导出策略未配置，导出已禁用。",
        )
    })

    it("opens explain state and patches the URL from openExplain", () => {
        currentSearchParams = "view=roles"
        const { view } = renderPageHook()

        act(() => {
            view.result.current.openExplain("ROLE", "role_1")
        })
        expect(view.result.current.explainSubject).toEqual({
            type: "ROLE",
            id: "role_1",
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/system/access-audit?view=roles&subjectType=ROLE&subjectId=role_1",
        )
    })

    it("closes explain state and removes subject params", () => {
        currentSearchParams = "view=roles&subjectType=ROLE&subjectId=role_1"
        const { view } = renderPageHook()

        act(() => {
            view.result.current.closeExplain()
        })
        expect(view.result.current.explainSubject).toBeNull()
        expect(mockReplace).toHaveBeenCalledWith(
            "/system/access-audit?view=roles",
        )
    })

    it("runs the change flow: preview failure records the action error", async () => {
        vi.mocked(api.previewAccessChange).mockRejectedValue(
            new Error("预览失败"),
        )
        currentSearchParams = "view=roles"
        const { view } = renderPageHook()

        await act(async () => {
            await view.result.current.startChange({
                subjectType: "ROLE",
                subjectId: "role_1",
                action: "DISABLE_ROLE",
                expectedPermissionVersion: "pv-live",
                reasonCode: "SECURITY_OPS",
                idempotencyKey: "pending",
                changeSet: [
                    {
                        targetReference: "status",
                        operation: "REPLACE",
                        valueReference: "disabled",
                    },
                ],
            })
        })
        expect(view.result.current.actionError).toBe("预览失败")
        expect(view.result.current.changeOpen).toBe(false)
    })

    it("opens the change dialog with impact after a successful preview", async () => {
        vi.mocked(api.previewAccessChange).mockResolvedValue({
            subjectLabel: "role_1",
            actionLabel: "DISABLE_ROLE",
            changeSummary: "预览：DISABLE_ROLE → role_1",
            affectedSubjectCount: 1,
            affectedWorkSurfaceSummary: "权限与审计",
            riskLevel: "high",
            riskSummary: "风险说明",
            riskFlags: [],
            diffs: [],
            submissionBlocker: {
                action: "DISABLE_ROLE",
                code: "REVIEW_POLICY_UNCONFIGURED",
                message: "复核策略未确定",
            },
        })
        currentSearchParams = "view=roles"
        const { view } = renderPageHook()

        await act(async () => {
            await view.result.current.startChange({
                subjectType: "ROLE",
                subjectId: "role_1",
                action: "DISABLE_ROLE",
                expectedPermissionVersion: "pv-live",
                reasonCode: "SECURITY_OPS",
                idempotencyKey: "pending",
                changeSet: [
                    {
                        targetReference: "status",
                        operation: "REPLACE",
                        valueReference: "disabled",
                    },
                ],
            })
        })
        expect(view.result.current.changeOpen).toBe(true)
        expect(view.result.current.impact?.submissionBlocker?.code).toBe(
            "REVIEW_POLICY_UNCONFIGURED",
        )
    })

    it("confirmChange records the blocked outcome and closes the dialog when a submission blocker exists", async () => {
        currentSearchParams = "view=roles"
        const { view } = renderPageHook()
        await act(async () => {
            view.result.current.setImpact({
                subjectLabel: "role_1",
                actionLabel: "DISABLE_ROLE",
                changeSummary: "预览",
                affectedSubjectCount: 1,
                affectedWorkSurfaceSummary: "权限与审计",
                riskLevel: "high",
                riskSummary: "风险说明",
                riskFlags: [],
                diffs: [],
                submissionBlocker: {
                    action: "DISABLE_ROLE",
                    code: "REVIEW_POLICY_UNCONFIGURED",
                    message: "复核策略未确定",
                },
            })
            view.result.current.setPendingCommand({
                subjectType: "ROLE",
                subjectId: "role_1",
                action: "DISABLE_ROLE",
                expectedPermissionVersion: "pv-live",
                reasonCode: "SECURITY_OPS",
                idempotencyKey: "pending",
                changeSet: [],
            })
        })

        await act(async () => {
            await view.result.current.confirmChange()
        })
        expect(view.result.current.changeOpen).toBe(false)
        expect(view.result.current.lastResult).toMatchObject({
            status: "blocked",
            title: "复核策略未确定，动作已阻断",
        })
    })

    it("confirmChange submits with the form values and records a succeeded result", async () => {
        vi.mocked(api.submitAccessChange).mockResolvedValue({
            outcome: "CONFIRMED",
            permissionVersion: "pv-live-7",
            auditEventId: "ae_1",
            affectedSubjectCount: 1,
            effectiveAt: "2026-01-01T00:00:00.000Z",
            reference: "op_1",
            nextSteps: ["刷新用户授权列表"],
            message: "已提交紧急撤权。",
        })
        currentSearchParams = "view=roles"
        const { view } = renderPageHook()
        await act(async () => {
            view.result.current.setImpact({
                subjectLabel: "u1",
                actionLabel: "EMERGENCY_REVOKE_USER_ROLE",
                changeSummary: "预览",
                affectedSubjectCount: 1,
                affectedWorkSurfaceSummary: "权限与审计",
                riskLevel: "medium",
                riskSummary: "风险说明",
                riskFlags: [],
                diffs: [],
            })
            view.result.current.setPendingCommand({
                subjectType: "USER",
                subjectId: "u1",
                action: "EMERGENCY_REVOKE_USER_ROLE",
                roleAssignmentId: "ur_1",
                expectedPermissionVersion: "pv-live",
                reasonCode: "EMERGENCY_STOP_LOSS",
                idempotencyKey: "pending",
            })
        })

        await act(async () => {
            await view.result.current.confirmChange()
        })
        expect(api.submitAccessChange).toHaveBeenCalledWith(
            expect.objectContaining({
                subjectId: "u1",
                reasonCode: "SECURITY_OPS",
                comment: undefined,
                idempotencyKey: expect.stringMatching(/^w19-/),
            }),
        )
        expect(view.result.current.changeOpen).toBe(false)
        expect(view.result.current.lastResult).toMatchObject({
            status: "succeeded",
            title: "授权变更已生效",
        })
    })
})
