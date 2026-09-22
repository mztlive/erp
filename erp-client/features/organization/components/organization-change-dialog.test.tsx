import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, expect, it, vi } from "vitest"

import { OrganizationChangeDialog } from "./organization-change-dialog"
import { EMPTY_CHANGE_DRAFT } from "@/features/organization/lib/change-payload"
import type {
    OrganizationChangeRequest,
    OrganizationStateView,
} from "@/features/organization/types"

const view: OrganizationStateView = {
    version: 3,
    organizationVersion: 3,
    policyVersion: 1,
    scopeVersion: "v",
    asOf: "2026-09-15T00:00:00Z",
    emptyReason: null,
    scopeSummary: "组织配置",
    ownershipBasis: "org_unit_configuration",
    people: [],
    roles: [],
    units: [
        {
            id: "sales",
            name: "销售部",
            parent_id: null,
            kind: "department",
            enabled: true,
            version: 1,
            reason: "初始化",
        },
    ],
    memberships: [],
    management: [],
}

afterEach(cleanup)

it("提交使用已预览命令；版本冲突失败路径可见", async () => {
    const previewed = {
        expected_version: 3,
        idempotency_key: "fixed-key",
        reason: "新建组织",
        change: {
            operation: "create_unit" as const,
            name: "一组",
            parent_id: null,
            kind: "department" as const,
        },
    }
    const onPreview = vi.fn(async (request) => ({
        id: "r1",
        actor_id: "admin",
        request,
        before: {
            version: 3,
            units: view.units,
            memberships: [],
            management: [],
        },
        after: {
            version: 4,
            units: view.units,
            memberships: [],
            management: [],
        },
        as_of: 1,
    }))
    const onSubmit = vi.fn(async (_request: OrganizationChangeRequest) => {
        throw Object.assign(new Error("组织范围已变化，请刷新后重试"), {
            status: 409,
            kind: "Http",
        })
    })
    vi.spyOn(crypto, "randomUUID").mockReturnValue(
        "fixed-key" as `${string}-${string}-${string}-${string}-${string}`,
    )
    render(
        <OrganizationChangeDialog
            open
            onOpenChange={vi.fn()}
            view={view}
            draft={{
                ...EMPTY_CHANGE_DRAFT,
                operation: "create_unit",
                name: "一组",
                reason: "新建组织",
            }}
            expectedVersion={3}
            previewing={false}
            submitting={false}
            onPreview={onPreview}
            onSubmit={onSubmit}
        />,
    )
    fireEvent.click(screen.getByRole("button", { name: "预览影响" }))
    await waitFor(() => expect(onPreview).toHaveBeenCalledTimes(1))
    expect(onPreview.mock.calls[0]?.[0]).toMatchObject(previewed)
    fireEvent.click(screen.getByRole("button", { name: "确认提交" }))
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit.mock.calls[0]?.[0]).toEqual(onPreview.mock.calls[0]?.[0])
    expect(await screen.findByText("组织范围已变化，请刷新后重试")).toBeTruthy()
})

it("预览后改字段会作废回执并要求重新预览", async () => {
    const onPreview = vi.fn(async (request) => ({
        id: "r1",
        actor_id: "admin",
        request,
        before: {
            version: 3,
            units: view.units,
            memberships: [],
            management: [],
        },
        after: {
            version: 4,
            units: view.units,
            memberships: [],
            management: [],
        },
        as_of: 1,
    }))
    render(
        <OrganizationChangeDialog
            open
            onOpenChange={vi.fn()}
            view={view}
            draft={{
                ...EMPTY_CHANGE_DRAFT,
                operation: "create_unit",
                name: "一组",
                reason: "新建组织",
            }}
            expectedVersion={3}
            previewing={false}
            submitting={false}
            onPreview={onPreview}
            onSubmit={vi.fn()}
        />,
    )
    fireEvent.click(screen.getByRole("button", { name: "预览影响" }))
    await screen.findByRole("button", { name: "确认提交" })
    fireEvent.change(screen.getByLabelText("组织名称"), {
        target: { value: "二组" },
    })
    await waitFor(() =>
        expect(screen.getByRole("button", { name: "预览影响" })).toBeTruthy(),
    )
})

it("从账号调整部门时固定人员并展示原部门到目标部门，确认才提交", async () => {
    const onSubmit = vi.fn(async () => undefined)
    const onPreview = vi.fn(async (request: OrganizationChangeRequest) => ({
        id: "receipt",
        actor_id: "admin",
        request,
        before: view,
        after: view,
        as_of: 1,
    }))
    render(
        <OrganizationChangeDialog
            open
            onOpenChange={vi.fn()}
            view={{
                ...view,
                people: [
                    {
                        id: "sales-user",
                        label: "销售员",
                        account: "xiaoshou",
                        active: true,
                        own_org_unit_id: null,
                    },
                ],
            }}
            draft={{
                ...EMPTY_CHANGE_DRAFT,
                operation: "transfer_member",
                userId: "sales-user",
                orgUnitId: "sales",
                reason: "入职分配",
            }}
            expectedVersion={3}
            previewing={false}
            submitting={false}
            onPreview={onPreview}
            onSubmit={onSubmit}
        />,
    )
    expect(screen.getByRole("heading", { name: "调整所属部门" })).toBeTruthy()
    expect(screen.queryByLabelText("变更类型")).toBeNull()
    expect(screen.getByText(/销售员：未分配部门 → 销售部/)).toBeTruthy()
    expect(
        screen
            .getByRole("combobox", { name: "人员" })
            .getAttribute("aria-disabled") === "true" ||
            (screen.getByRole("combobox", { name: "人员" }) as HTMLInputElement)
                .disabled,
    ).toBe(true)
    fireEvent.click(screen.getByRole("button", { name: "预览影响" }))
    await screen.findByRole("button", { name: "确认提交" })
    expect(onSubmit).not.toHaveBeenCalled()
    expect(onPreview.mock.calls[0]?.[0].change).toEqual({
        operation: "transfer_member",
        user_id: "sales-user",
        org_unit_id: "sales",
    })
    fireEvent.click(screen.getByRole("button", { name: "确认提交" }))
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
})
