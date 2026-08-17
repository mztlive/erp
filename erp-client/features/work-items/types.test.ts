import { describe, expect, it } from "vitest"

import {
    mapApprovalWorkItemSummaryDto,
    mapBlockedApprovalDto,
    mapWorkItemDto,
    type BlockedApprovalViewDto,
    type WorkItemDto,
} from "./types"

const makeWorkItemDto = (overrides: Partial<WorkItemDto> = {}): WorkItemDto => ({
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

describe("mapWorkItemDto", () => {
    it("maps all provided fields and normalizes versions and labels", () => {
        const projection = mapWorkItemDto(
            makeWorkItemDto({
                destination_workspace_id: "ws_1",
                route_context: { confirmation_scope: "scope_1" },
                approval_step_instance_id: "si_1",
                owner_role_label: "采购",
                owner_organization: { id: "org_1", display_name: "采购部" },
                owner_user: { id: "u_1", display_name: "周航" },
                processing_blocker: { code: "X", message: "被挡住" },
                business_object_label: "采购计划 PO-1",
                counterparty_label: "供应商 A",
                allowed_actions: ["START_PROCESSING", "VIEW"],
                action_blockers: [
                    "先处理别的",
                    { code: "B1", message: "再等等" },
                ],
                due_at: 1_700_000_000_100,
                reason_code: "LATE",
                reason_label: "来晚了",
                impact_summary: "影响入库",
                summary_sections: [{ label: "金额", value: "12.00", numeric: true }],
                queue_context_id: "qc_1",
            }),
        )

        expect(projection).toEqual({
            workItemId: "wi_1",
            workItemType: "procurement_confirmation",
            handlerKey: "procurement-confirmation",
            destinationWorkspaceId: "ws_1",
            routeContext: { confirmationScope: "scope_1" },
            approvalStepInstanceId: "si_1",
            status: "OPEN",
            assignmentMode: "DIRECT",
            assignmentSource: "assigned",
            ownerRole: "procurement",
            ownerRoleLabel: "采购",
            ownerOrganization: { id: "org_1", displayName: "采购部" },
            ownerUser: { id: "u_1", displayName: "周航" },
            processingState: "READY",
            processingBlocker: { code: "X", message: "被挡住" },
            businessObjectType: "purchase_plan",
            businessObjectId: "po_1",
            rootBusinessObjectId: "po_1",
            businessObjectLabel: "采购计划 PO-1",
            counterpartyLabel: "供应商 A",
            subjectVersion: "v3",
            taskVersion: "5",
            allowedActions: ["START_PROCESSING", "VIEW"],
            actionBlockers: ["先处理别的", "再等等"],
            priority: "HIGH",
            dueAt: 1_700_000_000_100,
            reasonCode: "LATE",
            reasonLabel: "来晚了",
            impactSummary: "影响入库",
            nextActionHint: "进入对应页面后提交处理结论。",
            summarySections: [{ label: "金额", value: "12.00", numeric: true }],
            briefLines: [],
            briefMoreCount: undefined,
            listSummary: undefined,
            createdAt: 1_700_000_000_000,
            queueContextId: "qc_1",
        })
    })

    it("applies stable fallbacks for missing optional fields", () => {
        const projection = mapWorkItemDto(makeWorkItemDto())

        expect(projection.destinationWorkspaceId).toBeUndefined()
        expect(projection.routeContext).toBeUndefined()
        expect(projection.approvalStepInstanceId).toBeUndefined()
        expect(projection.ownerRoleLabel).toBe("procurement")
        expect(projection.ownerOrganization).toEqual({
            id: "org_1",
            displayName: "责任组织",
        })
        expect(projection.ownerUser).toBeUndefined()
        expect(projection.processingBlocker).toBeUndefined()
        expect(projection.businessObjectLabel).toBe("procurement_confirmation")
        expect(projection.counterpartyLabel).toBeUndefined()
        expect(projection.allowedActions).toEqual([])
        expect(projection.actionBlockers).toEqual([])
        expect(projection.reasonLabel).toBe("需要你处理")
        expect(projection.impactSummary).toBe(
            "不处理将卡住后续业务，请进入对应页面核对。",
        )
        expect(projection.nextActionHint).toBe("进入对应页面后提交处理结论。")
        expect(projection.summarySections).toEqual([])
        expect(projection.briefLines).toEqual([])
        expect(projection.briefMoreCount).toBeUndefined()
        expect(projection.listSummary).toBeUndefined()
        expect(projection.queueContextId).toBeUndefined()
    })

    it("falls back to the owner user id when the owner user object is absent", () => {
        const projection = mapWorkItemDto(
            makeWorkItemDto({ owner_user_id: "u_9" }),
        )
        expect(projection.ownerUser).toEqual({
            id: "u_9",
            displayName: "处理人待确认",
        })
    })

    it("coerces numeric task versions to strings", () => {
        const projection = mapWorkItemDto(makeWorkItemDto({ task_version: 7 }))
        expect(projection.taskVersion).toBe("7")
    })
})

describe("mapApprovalWorkItemSummaryDto", () => {
    it("maps the approval-scoped summary", () => {
        const summary = mapApprovalWorkItemSummaryDto({
            id: "wi_1",
            work_item_type: "card_funds_review",
            approval_step_instance_id: "si_1",
            status: "COMPLETED",
            assignment_mode: "POOL",
            owner_role: "finance",
            owner_organization_id: "org_2",
            owner_user_id: "u_2",
            task_version: 12,
        })
        expect(summary).toEqual({
            workItemId: "wi_1",
            workItemType: "card_funds_review",
            approvalStepInstanceId: "si_1",
            status: "COMPLETED",
            assignmentMode: "POOL",
            ownerRole: "finance",
            ownerOrganizationId: "org_2",
            ownerUserId: "u_2",
            taskVersion: "12",
        })
    })

    it("omits absent optional fields", () => {
        const summary = mapApprovalWorkItemSummaryDto({
            id: "wi_1",
            work_item_type: "card_funds_review",
            approval_step_instance_id: null,
            status: "OPEN",
            assignment_mode: "DIRECT",
            owner_role: "finance",
            owner_organization_id: "org_2",
            owner_user_id: null,
            task_version: "1",
        })
        expect(summary.approvalStepInstanceId).toBeUndefined()
        expect(summary.ownerUserId).toBeUndefined()
        expect(summary.taskVersion).toBe("1")
    })
})

describe("mapBlockedApprovalDto", () => {
    it("maps a blocked approval without a work item", () => {
        const dto: BlockedApprovalViewDto = {
            approval_instance_id: "ai_1",
            instance_version: 2,
            current_step_instance_id: "si_1",
            step_version: 1,
            work_item: null,
            business_object_label: "采购计划 PO-1",
            blocker_code: "STEP_BLOCKED",
            blocker_message: "请处理后继续",
            blocked_at: 1_700_000_000_000,
            allowed_actions: ["RETRY_CURRENT_STEP"],
        }
        expect(mapBlockedApprovalDto(dto)).toEqual({
            approvalInstanceId: "ai_1",
            instanceVersion: "2",
            currentStepInstanceId: "si_1",
            stepVersion: "1",
            workItem: undefined,
            businessObjectLabel: "采购计划 PO-1",
            blockerCode: "STEP_BLOCKED",
            blockerMessage: "请处理后继续",
            blockedAt: 1_700_000_000_000,
            allowedActions: ["RETRY_CURRENT_STEP"],
        })
    })

    it("maps a blocked approval that still carries a work item summary", () => {
        const dto: BlockedApprovalViewDto = {
            approval_instance_id: "ai_2",
            instance_version: "9",
            current_step_instance_id: "si_2",
            step_version: 4,
            work_item: {
                id: "wi_2",
                work_item_type: "procurement_confirmation",
                approval_step_instance_id: "si_2",
                status: "OPEN",
                assignment_mode: "DIRECT",
                owner_role: "procurement",
                owner_organization_id: "org_1",
                owner_user_id: null,
                task_version: "3",
            },
            business_object_label: "采购计划 PO-2",
            blocker_code: "APPROVAL_BLOCKED",
            blocker_message: "等待复核",
            blocked_at: 1_700_000_000_500,
            allowed_actions: ["RETRY_CURRENT_STEP"],
        }
        const view = mapBlockedApprovalDto(dto)
        expect(view.instanceVersion).toBe("9")
        expect(view.stepVersion).toBe("4")
        expect(view.workItem).toEqual({
            workItemId: "wi_2",
            workItemType: "procurement_confirmation",
            approvalStepInstanceId: "si_2",
            status: "OPEN",
            assignmentMode: "DIRECT",
            ownerRole: "procurement",
            ownerOrganizationId: "org_1",
            ownerUserId: undefined,
            taskVersion: "3",
        })
    })
})
