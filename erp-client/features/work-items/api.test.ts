import { beforeEach, describe, expect, it, vi } from "vitest"

import * as api from "./api"
import type { WorkItemDto } from "./types"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

import { apiGet, apiPost } from "@/lib/api"

const makeWorkItemDto = (
    overrides: Partial<WorkItemDto> = {},
): WorkItemDto => ({
    id: "wi_1",
    work_item_type: "procurement_confirmation",
    handler_key: "procurement-confirmation",
    approval_step_instance_id: null,
    status: "OPEN",
    assignment_mode: "DIRECT",
    assignment_source: "assigned",
    owner_role: "procurement",
    owner_organization_id: "org_1",
    processing_state: "READY",
    business_object_type: "purchase_plan",
    business_object_id: "po_1",
    root_business_object_id: "po_1",
    subject_version: "v3",
    task_version: "5",
    priority: "HIGH",
    created_at: 1_700_000_000_000,
    ...overrides,
})

beforeEach(() => {
    vi.clearAllMocks()
})

describe("parseWorkItemConflict", () => {
    it("returns undefined for non-object errors", () => {
        expect(api.parseWorkItemConflict(null)).toBeUndefined()
        expect(api.parseWorkItemConflict("boom")).toBeUndefined()
        expect(api.parseWorkItemConflict(new Error("boom"))).toBeUndefined()
    })

    it("returns undefined for non-409 errors", () => {
        expect(api.parseWorkItemConflict({ status: 500 })).toBeUndefined()
        expect(api.parseWorkItemConflict({ status: 400 })).toBeUndefined()
    })

    it("returns undefined for a 409 without a known conflict code", () => {
        expect(
            api.parseWorkItemConflict({
                status: 409,
                code: "SOMETHING_ELSE",
            }),
        ).toBeUndefined()
        expect(api.parseWorkItemConflict({ status: 409 })).toBeUndefined()
    })

    it("extracts a conflict whose code lives on the error itself", () => {
        const current = makeWorkItemDto()
        const result = api.parseWorkItemConflict({
            status: 409,
            code: "WORK_ITEM_VERSION_CONFLICT",
            responseData: { data: { current_work_item: current } },
        })
        expect(result).toEqual({
            code: "WORK_ITEM_VERSION_CONFLICT",
            currentWorkItem: current,
        })
    })

    it("reads the conflict code from the response envelope as a fallback", () => {
        const current = makeWorkItemDto()
        const result = api.parseWorkItemConflict({
            status: 409,
            responseData: {
                code: "WORK_ITEM_RESPONSIBILITY_CONFLICT",
                data: { current_work_item: current },
            },
        })
        expect(result).toEqual({
            code: "WORK_ITEM_RESPONSIBILITY_CONFLICT",
            currentWorkItem: current,
        })
    })

    it("keeps a conflict with an absent current work item", () => {
        const result = api.parseWorkItemConflict({
            status: 409,
            code: "WORK_ITEM_VERSION_CONFLICT",
            responseData: { data: { current_work_item: null } },
        })
        expect(result).toEqual({
            code: "WORK_ITEM_VERSION_CONFLICT",
            currentWorkItem: null,
        })
    })

    it("drops a conflict payload that is not a valid work item", () => {
        const result = api.parseWorkItemConflict({
            status: 409,
            code: "WORK_ITEM_VERSION_CONFLICT",
            responseData: { data: { current_work_item: { id: 1 } } },
        })
        expect(result).toEqual({
            code: "WORK_ITEM_VERSION_CONFLICT",
            currentWorkItem: null,
        })
    })
})

describe("listWorkItems", () => {
    it("serializes filters, defaults and array fields", async () => {
        vi.mocked(apiGet).mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            page_size: 100,
        })
        await api.listWorkItems({
            scope: "mine",
            family: "procurement",
            workItemType: "procurement_confirmation",
            status: "COMPLETED",
            due: "overdue",
            priorities: [3, 1],
            query: "采购",
            sort: "due_asc",
            cursor: "c_1",
            queueContextId: "qc_1",
            currentWorkItemId: "wi_2",
            timezone: "Asia/Shanghai",
            page: 3,
            pageSize: 20,
        })
        expect(apiGet).toHaveBeenCalledWith("/admin/work-items", {
            scope: "mine",
            family: "procurement",
            work_item_type: "procurement_confirmation",
            status: "COMPLETED",
            due: "overdue",
            priorities: "3,1",
            q: "采购",
            sort: "due_asc",
            cursor: "c_1",
            queue_context_id: "qc_1",
            current_work_item_id: "wi_2",
            timezone: "Asia/Shanghai",
            page: 3,
            page_size: 20,
        })
    })

    it("applies default sort and paging when omitted", async () => {
        vi.mocked(apiGet).mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            page_size: 100,
        })
        await api.listWorkItems({ scope: "team", timezone: "UTC" })
        expect(apiGet).toHaveBeenCalledWith("/admin/work-items", {
            scope: "team",
            family: undefined,
            work_item_type: undefined,
            status: undefined,
            due: undefined,
            priorities: undefined,
            q: undefined,
            sort: "priority_due",
            cursor: undefined,
            queue_context_id: undefined,
            current_work_item_id: undefined,
            timezone: "UTC",
            page: 1,
            page_size: 100,
        })
    })
})

describe("getWorkItemStats", () => {
    it("serializes stats filters", async () => {
        vi.mocked(apiGet).mockResolvedValue({
            assigned: 0,
            team: 0,
            due_today: 0,
            overdue: 0,
            exception: 0,
            as_of: 0,
        })
        await api.getWorkItemStats({
            scope: "managed",
            family: "settlement",
            workItemType: "card_funds_review",
            due: "today",
            timezone: "Asia/Shanghai",
        })
        expect(apiGet).toHaveBeenCalledWith("/admin/work-items/stats", {
            scope: "managed",
            family: "settlement",
            work_item_type: "card_funds_review",
            due: "today",
            timezone: "Asia/Shanghai",
        })
    })
})

describe("getWorkItem", () => {
    it("encodes the work item id in the detail path", async () => {
        vi.mocked(apiGet).mockResolvedValue(makeWorkItemDto())
        await api.getWorkItem("wi 1/2")
        expect(apiGet).toHaveBeenCalledWith("/admin/work-items/wi%201%2F2")
    })
})

describe("submitWorkItemResponsibility", () => {
    const common = {
        expected_task_version: "5",
        idempotency_key: "op_1",
    }

    it("posts START_PROCESSING to the dedicated endpoint", async () => {
        vi.mocked(apiPost).mockResolvedValue(makeWorkItemDto())
        await api.submitWorkItemResponsibility({
            kind: "START_PROCESSING",
            workItemId: "wi_1",
            expectedTaskVersion: "5",
            idempotencyKey: "op_1",
        })
        expect(apiPost).toHaveBeenCalledWith(
            "/admin/work-items/wi_1/start-processing",
            common,
        )
    })

    it("posts RELEASE_TO_TEAM with the reason", async () => {
        vi.mocked(apiPost).mockResolvedValue(makeWorkItemDto())
        await api.submitWorkItemResponsibility({
            kind: "RELEASE_TO_TEAM",
            workItemId: "wi_1",
            expectedTaskVersion: "5",
            reason: "开会去了",
            idempotencyKey: "op_2",
        })
        expect(apiPost).toHaveBeenCalledWith(
            "/admin/work-items/wi_1/release-to-team",
            { ...common, idempotency_key: "op_2", reason: "开会去了" },
        )
    })

    it("posts REASSIGN with target user and reason", async () => {
        vi.mocked(apiPost).mockResolvedValue(makeWorkItemDto())
        await api.submitWorkItemResponsibility({
            kind: "REASSIGN",
            workItemId: "wi_1",
            expectedTaskVersion: "5",
            targetUserId: "u_9",
            reason: "转交",
            idempotencyKey: "op_3",
        })
        expect(apiPost).toHaveBeenCalledWith(
            "/admin/work-items/wi_1/reassign",
            {
                ...common,
                idempotency_key: "op_3",
                target_user_id: "u_9",
                reason: "转交",
            },
        )
    })

    it("posts CLOSE with reason code, replacement and comment", async () => {
        vi.mocked(apiPost).mockResolvedValue(makeWorkItemDto())
        await api.submitWorkItemResponsibility({
            kind: "CLOSE",
            workItemId: "wi_1",
            expectedTaskVersion: "5",
            reasonCode: "DUPLICATE",
            replacementWorkItemId: "wi_2",
            comment: "重复任务",
            idempotencyKey: "op_4",
        })
        expect(apiPost).toHaveBeenCalledWith("/admin/work-items/wi_1/close", {
            ...common,
            idempotency_key: "op_4",
            reason_code: "DUPLICATE",
            replacement_work_item_id: "wi_2",
            comment: "重复任务",
        })
    })
})
