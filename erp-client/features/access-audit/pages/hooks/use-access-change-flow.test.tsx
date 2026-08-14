import { act, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import * as api from "@/features/access-audit/api"
import { useAccessChangeFlow } from "./use-access-change-flow"
import type { AccessChangeCommand } from "@/features/access-audit/types"

vi.mock("@/features/access-audit/api", () => ({
    fetchAccessList: vi.fn(),
    fetchAuditEvent: vi.fn(),
    fetchEffectiveAccess: vi.fn(),
    previewAccessChange: vi.fn(),
    submitAccessChange: vi.fn(),
}))

const disableCommand: AccessChangeCommand = {
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
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useAccessChangeFlow", () => {
    it("opens the dialog with the preview impact after a successful preview", async () => {
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
        const { result } = renderHookWithProviders(() =>
            useAccessChangeFlow({
                setActionError: vi.fn(),
                setLastResult: vi.fn(),
            }),
        )

        await act(async () => {
            await result.current.startChange(disableCommand)
        })
        expect(result.current.changeOpen).toBe(true)
        expect(result.current.impact?.submissionBlocker?.code).toBe(
            "REVIEW_POLICY_UNCONFIGURED",
        )
        expect(api.previewAccessChange).toHaveBeenCalledWith(disableCommand)
    })

    it("records an action error when the preview fails", async () => {
        vi.mocked(api.previewAccessChange).mockRejectedValue(
            new Error("预览失败"),
        )
        const setActionError = vi.fn()
        const { result } = renderHookWithProviders(() =>
            useAccessChangeFlow({ setActionError, setLastResult: vi.fn() }),
        )

        await act(async () => {
            await result.current.startChange(disableCommand)
        })
        expect(setActionError).toHaveBeenCalledWith("预览失败")
        expect(result.current.changeOpen).toBe(false)
    })

    it("records the blocked result and closes the dialog when a submission blocker exists", async () => {
        const setLastResult = vi.fn()
        const { result } = renderHookWithProviders(() =>
            useAccessChangeFlow({ setActionError: vi.fn(), setLastResult }),
        )

        await act(async () => {
            result.current.setPendingCommand(disableCommand)
            result.current.setImpact({
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
        })

        await act(async () => {
            await result.current.confirmChange()
        })
        expect(result.current.changeOpen).toBe(false)
        expect(setLastResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "blocked",
                title: "复核策略未确定，动作已阻断",
            }),
        )
    })

    it("submits with the form values and records a succeeded result", async () => {
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
        const client = createFreshQueryClient()
        const setLastResult = vi.fn()
        const { result } = renderHookWithProviders(
            () =>
                useAccessChangeFlow({
                    setActionError: vi.fn(),
                    setLastResult,
                }),
            { queryClient: client },
        )

        const revokeCommand: AccessChangeCommand = {
            subjectType: "USER",
            subjectId: "u1",
            action: "EMERGENCY_REVOKE_USER_ROLE",
            roleAssignmentId: "ur_1",
            expectedPermissionVersion: "pv-live",
            reasonCode: "EMERGENCY_STOP_LOSS",
            idempotencyKey: "pending",
        }
        await act(async () => {
            result.current.setPendingCommand(revokeCommand)
            result.current.setImpact({
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
        })

        await act(async () => {
            await result.current.confirmChange()
        })
        expect(api.submitAccessChange).toHaveBeenCalledWith(
            expect.objectContaining({
                subjectId: "u1",
                reasonCode: "SECURITY_OPS",
                comment: undefined,
                idempotencyKey: expect.stringMatching(/^w19-/),
            }),
        )
        expect(result.current.changeOpen).toBe(false)
        expect(setLastResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "授权变更已生效",
            }),
        )
        await waitFor(() => expect(client.isMutating()).toBe(0))
    })
})
