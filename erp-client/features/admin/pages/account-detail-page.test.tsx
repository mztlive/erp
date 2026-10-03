import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import type {
    OrganizationChangeRequest,
    OrganizationStateView,
} from "@/features/organization/types"

const state = vi.hoisted(() => ({
    permissions: [
        "admin:list",
        "admin:update",
        "org_unit:list",
        "org_unit:manage",
    ],
    organization: undefined as OrganizationStateView | undefined,
    pending: true,
    fetching: true,
    preview: vi.fn(),
    submit: vi.fn(),
    update: vi.fn(),
}))

vi.mock("next/navigation", () => ({ useRouter: () => ({ push: vi.fn() }) }))
vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: { permissions: state.permissions },
    }),
}))
vi.mock("../hooks/queries", () => ({
    useAdminsQuery: () => ({
        data: [
            {
                id: "new-person",
                account: "viewer",
                name: "原姓名",
                role_ids: ["viewer-role"],
                created_at: 1,
            },
        ],
        isPending: false,
        isError: false,
    }),
    useRolesQuery: () => ({
        data: [{ id: "viewer-role", name: "客户查看" }],
        isSuccess: true,
    }),
    useAssignableRolesQuery: () => ({
        data: [{ id: "viewer-role", name: "客户查看" }],
        isSuccess: true,
    }),
    useAdminMutations: () => ({ isUpdating: false, updateAdmin: state.update }),
}))
vi.mock("@/features/organization/hooks/queries", () => ({
    useOrganizationStateQuery: () => ({
        data: state.organization,
        isPending: state.pending,
        isFetching: state.fetching,
        isSuccess: !state.pending,
        isError: false,
    }),
    usePreviewOrganizationChangeMutation: () => ({
        isPending: false,
        mutateAsync: state.preview,
    }),
    useSubmitOrganizationChangeMutation: () => ({
        isPending: false,
        mutateAsync: state.submit,
    }),
}))
vi.mock("../components/accounts/account-operation-permissions", () => ({
    AccountOperationPermissions: () => null,
}))
vi.mock("../components/accounts/personal-business-permissions", () => ({
    PersonalBusinessPermissions: () => null,
}))
vi.mock("../components/accounts/access-check-panel", () => ({
    AccessCheckPanel: () => null,
}))

import { AccountDetailPage } from "./account-detail-page"

const organization: OrganizationStateView = {
    version: 6,
    organizationVersion: 6,
    policyVersion: 2,
    scopeVersion: "scope-2",
    asOf: "2026-10-03T12:00:00Z",
    emptyReason: null,
    scopeSummary: "公司范围",
    ownershipBasis: "当前人员",
    units: [],
    memberships: [],
    management: [],
    roles: [],
    people: [
        {
            id: "new-person",
            account: "viewer",
            label: "原姓名",
            active: true,
            own_org_unit_id: null,
        },
    ],
}

afterEach(() => {
    cleanup()
    vi.clearAllMocks()
    state.permissions = [
        "admin:list",
        "admin:update",
        "org_unit:list",
        "org_unit:manage",
    ]
    state.organization = undefined
    state.pending = true
    state.fetching = true
})

test.each(["首次读取", "刷新缓存人员"])(
    "%s未结束时不能冻结空组织资料；晚到事实按统一预览和确认保存",
    async (initial) => {
        if (initial === "刷新缓存人员") {
            state.organization = { ...organization, people: [] }
            state.pending = false
        }
        state.preview.mockImplementation(
            async (request: OrganizationChangeRequest) => ({
                id: "preview-receipt",
                actor_id: "admin",
                request,
                before: organization,
                after: organization,
                as_of: 1,
            }),
        )
        state.submit.mockResolvedValue(undefined)
        const { rerender } = render(
            <AccountDetailPage accountId="new-person" />,
        )
        const edit = screen.getByRole("button", {
            name: "编辑账号资料",
        }) as HTMLButtonElement
        expect(edit.disabled).toBe(true)
        fireEvent.click(edit)
        expect(screen.queryByRole("textbox", { name: /姓名/ })).toBeNull()

        state.organization = organization
        state.pending = false
        state.fetching = false
        rerender(<AccountDetailPage accountId="new-person" />)
        expect(edit.disabled).toBe(false)
        fireEvent.click(edit)
        fireEvent.change(screen.getByRole("textbox", { name: /姓名/ }), {
            target: { value: "核对后姓名" },
        })
        fireEvent.change(screen.getByRole("textbox", { name: /变更原因/ }), {
            target: { value: "资料统一核对" },
        })
        fireEvent.click(screen.getByRole("button", { name: "保存" }))
        await waitFor(() => expect(state.preview).toHaveBeenCalledTimes(1))
        expect(state.preview).toHaveBeenCalledWith(
            expect.objectContaining({
                expected_version: 6,
                reason: "资料统一核对",
                change: {
                    operation: "update_person_profile",
                    profile: {
                        user_id: "new-person",
                        expected_name: "原姓名",
                        expected_role_ids: ["viewer-role"],
                        role_ids: null,
                        name: "核对后姓名",
                        org_unit_id: null,
                    },
                },
            }),
        )
        await screen.findByText("请确认本次修改")
        fireEvent.click(screen.getByRole("button", { name: "确认保存" }))
        await waitFor(() =>
            expect(state.submit).toHaveBeenCalledWith(
                state.preview.mock.calls[0][0],
            ),
        )
        expect(state.update).not.toHaveBeenCalled()
    },
)

test("无组织管理权限的账号仍按姓名权限保存，不等待未授权的组织请求", async () => {
    state.permissions = ["admin:list", "admin:update"]
    state.update.mockResolvedValue(undefined)
    render(<AccountDetailPage accountId="new-person" />)
    const edit = screen.getByRole("button", {
        name: "编辑账号资料",
    }) as HTMLButtonElement
    expect(edit.disabled).toBe(false)
    fireEvent.click(edit)
    expect(screen.queryByRole("textbox", { name: /变更原因/ })).toBeNull()
    fireEvent.change(screen.getByRole("textbox", { name: /姓名/ }), {
        target: { value: "仅姓名更新" },
    })
    fireEvent.click(screen.getByRole("button", { name: "保存" }))
    await waitFor(() =>
        expect(state.update).toHaveBeenCalledWith({
            id: "new-person",
            payload: { name: "仅姓名更新" },
        }),
    )
    expect(state.preview).not.toHaveBeenCalled()
    expect(state.submit).not.toHaveBeenCalled()
})
