import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useSupplierEditor } from "./use-supplier-editor"
import { createSupplierEditorDefaults } from "@/features/master-data/lib/supplier-editor-model"
import type {
    MasterDataCenterView,
    MasterDataMutationResult,
} from "@/features/master-data/types"

const navMocks = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: navMocks.back,
    }),
}))

const toastMocks = vi.hoisted(() => ({ add: vi.fn() }))

vi.mock("@/components/ui/toast", () => ({
    toast: { add: toastMocks.add },
}))

const authMocks = vi.hoisted(() => ({
    permissions: [
        "supplier:create",
        "supplier:update",
        "supplier:delete",
        "supplier_sensitive:reveal",
    ] as readonly string[],
}))

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        isPending: false,
        isError: false,
        data: { permissions: authMocks.permissions },
    }),
}))

const masterDataMocks = vi.hoisted(() => ({
    detail: {
        data: undefined as MasterDataCenterView | undefined,
        isPending: false,
        isError: false,
        error: null,
        refetch: vi.fn(),
    },
    create: {
        mutateAsync: vi.fn(),
        isPending: false,
    },
    revise: {
        mutateAsync: vi.fn(),
        isPending: false,
    },
}))

vi.mock("@/features/master-data/hooks/queries", () => ({
    useMasterDataCenterQuery: () => masterDataMocks.detail,
    useCreateMasterDataMutation: () => masterDataMocks.create,
    useCreateRevisionMutation: () => masterDataMocks.revise,
}))

function makeCenterView(
    overrides: Partial<MasterDataCenterView> = {},
): MasterDataCenterView {
    return {
        resource: "suppliers",
        stableId: "sup-1",
        stableNo: "S-001",
        name: "示例供应商",
        lifecycleStatus: "ENABLED",
        lifecycleStatusLabel: "启用",
        lifecycleTone: "success",
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        lockVersion: 3,
        currentRevision: {
            revisionId: "rev-3",
            revisionNo: 3,
            name: "示例供应商",
            effectiveFrom: "2026-06-01T00:00:00.000Z",
            changeReason: "初始建档",
            actor: "张三",
            fields: [
                { label: "企业主体", value: "示例企业有限公司" },
                { label: "公司签约主体", value: "福尚云" },
                { label: "公司付款主体", value: "福尚云" },
            ],
        },
        revisionTimeline: [],
        selectorEligibility: [],
        usageSummary: { historicalReferenceCount: 0, note: "" },
        sensitiveFields: [],
        resourceFacts: [],
        allowedActions: ["CREATE_REVISION", "DISABLE"],
        actionBlockers: [],
        auditEvents: [],
        sections: ["overview", "versions", "relations", "audit"],
        ...overrides,
    }
}

function makeSucceededResult(
    stableId: string,
): MasterDataMutationResult {
    return {
        outcome: "succeeded",
        stableId,
        stableNo: "S-999",
        revisionId: "rev-9",
        revisionNo: 9,
        revisionState: "CURRENT",
        effectiveFrom: "2026-06-01T00:00:00.000Z",
        recordedAt: "2026-06-01T00:00:00.000Z",
        actor: "张三",
        changeReason: "新建供应商",
        reference: "",
        nextActions: [],
    }
}

beforeEach(() => {
    navMocks.push.mockClear()
    navMocks.replace.mockClear()
    toastMocks.add.mockClear()
    masterDataMocks.detail.data = undefined
    masterDataMocks.detail.refetch = vi
        .fn()
        .mockResolvedValue({ data: masterDataMocks.detail.data })
    masterDataMocks.create.mutateAsync = vi.fn()
    masterDataMocks.revise.mutateAsync = vi.fn()
})

describe("useSupplierEditor", () => {
    it("starts a create session with empty defaults", () => {
        const { result } = renderHook(() => useSupplierEditor("new"))
        expect(result.current.isCreate).toBe(true)
        expect(result.current.form.state.values).toEqual(
            createSupplierEditorDefaults(true),
        )
        expect(result.current.canCreate).toBe(true)
        expect(result.current.canRevise).toBe(false)
        expect(result.current.listHref).toBe("/master-data/suppliers")
        expect(result.current.pending).toBe(false)
    })

    it("hydrates an existing record and resolves permissions", () => {
        masterDataMocks.detail.data = makeCenterView()
        const { result } = renderHook(() => useSupplierEditor("sup-1"))
        expect(result.current.isCreate).toBe(false)
        expect(result.current.data?.stableId).toBe("sup-1")
        expect(result.current.form.state.values).toEqual(
            expect.objectContaining({
                name: "示例供应商",
                company: "示例企业有限公司",
                signingEntity: "福尚云",
                paymentEntity: "福尚云",
            }),
        )
        expect(result.current.canRevise).toBe(true)
        expect(result.current.canDisable).toBe(true)
    })

    it("blocks a revision when the update permission is missing", () => {
        authMocks.permissions = ["supplier:create"]
        masterDataMocks.detail.data = makeCenterView()
        const { result } = renderHook(() => useSupplierEditor("sup-1"))
        expect(result.current.canRevise).toBe(false)
    })

    it("reports validation errors without calling the API", async () => {
        const { result } = renderHook(() => useSupplierEditor("new"))
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(result.current.formError).toBe("请填写供应商名称")
        expect(masterDataMocks.create.mutateAsync).not.toHaveBeenCalled()
    })

    it("creates a supplier and navigates to the new record on success", async () => {
        masterDataMocks.create.mutateAsync = vi
            .fn()
            .mockResolvedValue(makeSucceededResult("sup-new"))
        const { result } = renderHook(() => useSupplierEditor("new"))
        act(() => {
            result.current.form.setFieldValue("name", "示例供应商")
            result.current.form.setFieldValue("company", "示例企业有限公司")
            result.current.form.setFieldValue("signingEntity", "福尚云")
            result.current.form.setFieldValue("paymentEntity", "福尚云")
        })
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(masterDataMocks.create.mutateAsync).toHaveBeenCalledTimes(1)
        const input = masterDataMocks.create.mutateAsync.mock.calls[0]![0]!
        expect(input).toMatchObject({
            resource: "suppliers",
            name: "示例供应商",
            changeReason: "新建供应商",
        })
        expect(input.fields).toMatchObject({
            company: "示例企业有限公司",
            signingEntity: "福尚云",
            paymentEntity: "福尚云",
        })
        expect(input.idempotencyKey).toMatch(/^create-supplier-/)
        expect(toastMocks.add).toHaveBeenCalledTimes(1)
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/suppliers/sup-new",
        )
    })

    it("records a blocked result instead of navigating", async () => {
        masterDataMocks.create.mutateAsync = vi.fn().mockResolvedValue({
            outcome: "blocked",
            code: "POLICY_BLOCK",
            message: "资料不完整",
            detail: "缺少资质附件",
        })
        const { result } = renderHook(() => useSupplierEditor("new"))
        act(() => {
            result.current.form.setFieldValue("name", "示例供应商")
            result.current.form.setFieldValue("company", "示例企业有限公司")
            result.current.form.setFieldValue("signingEntity", "福尚云")
            result.current.form.setFieldValue("paymentEntity", "福尚云")
        })
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(masterDataMocks.create.mutateAsync).toHaveBeenCalledTimes(1)
        const input = masterDataMocks.create.mutateAsync.mock.calls[0]![0]!
        expect(input).toMatchObject({ resource: "suppliers" })
        expect(navMocks.replace).not.toHaveBeenCalled()
        expect(toastMocks.add).not.toHaveBeenCalled()
    })

    it("revises an existing record with the confirmed change reason", async () => {
        masterDataMocks.detail.data = makeCenterView()
        masterDataMocks.revise.mutateAsync = vi
            .fn()
            .mockResolvedValue(makeSucceededResult("sup-1"))
        const { result } = renderHook(() => useSupplierEditor("sup-1"))
        act(() => {
            result.current.pendingChangeReasonRef.current = "更新企业主体"
        })
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(masterDataMocks.revise.mutateAsync).toHaveBeenCalledTimes(1)
        const input = masterDataMocks.revise.mutateAsync.mock.calls[0]![0]!
        expect(input).toMatchObject({
            resource: "suppliers",
            stableId: "sup-1",
            baseRevisionId: "rev-3",
            expectedLockVersion: 3,
            changeReason: "更新企业主体",
        })
        expect(masterDataMocks.detail.refetch).toHaveBeenCalled()
    })

    it("guards navigation when the form is dirty", () => {
        const { result } = renderHook(() => useSupplierEditor("new"))
        act(() => {
            result.current.navigateAway("/master-data/suppliers")
        })
        expect(navMocks.push).toHaveBeenCalledWith("/master-data/suppliers")

        act(() => {
            result.current.form.setFieldValue("name", "修改中的名称")
        })
        act(() => {
            result.current.navigateAway("/master-data/suppliers")
        })
        expect(result.current.discardOpen).toBe(true)
        expect(navMocks.push).toHaveBeenCalledTimes(1)
    })

    it("confirms discarding and resumes the pending navigation", () => {
        const { result } = renderHook(() => useSupplierEditor("new"))
        act(() => {
            result.current.form.setFieldValue("name", "修改中的名称")
        })
        act(() => {
            result.current.navigateAway("/master-data/suppliers")
        })
        act(() => {
            result.current.setDiscardOpen(false)
            result.current.setPendingNav(null)
            navMocks.push("/master-data/suppliers")
        })
        expect(result.current.discardOpen).toBe(false)
        expect(result.current.pendingNav).toBe(null)
        expect(navMocks.push).toHaveBeenCalledWith("/master-data/suppliers")
    })
})
