import { expect, test } from "vitest"

import { mapWorkItemDto, type WorkItemDto } from "./types"

function approvalWorkItemDto(): WorkItemDto {
    return {
        id: "work-item-1",
        work_item_type: "DOCUMENT_APPROVAL",
        handler_key: "document_approval",
        approval_step_instance_id: null,
        approval_node_execution_id: "execution-1",
        approval_context: {
            instance_id: "instance-1",
            status: "RUNNING",
            current_round_no: 2,
            current_node_label: " 财务负责人审批 ",
            current_assignee_label: " 李四 ",
            latest_rejection_reason: " 税率依据不完整 ",
            process_version: 3,
        },
        status: "OPEN",
        assignment_source: "SYSTEM_RULE",
        owner_role: "finance_manager",
        owner_organization_id: "company",
        processing_state: "READY",
        business_object_type: "purchase_order",
        business_object_id: "purchase-order-1",
        root_business_object_id: "purchase-order-1",
        subject_version: "submission-2",
        task_version: "4",
        priority: 1,
        created_at: 1,
    }
}

test("maps authoritative approval context for workbench confirmation", () => {
    const item = mapWorkItemDto(approvalWorkItemDto())

    expect(item.approvalProcessInstanceId).toBe("instance-1")
    expect(item.approvalContext).toEqual({
        instanceId: "instance-1",
        status: "RUNNING",
        currentRoundNo: 2,
        currentNodeLabel: "财务负责人审批",
        currentAssigneeLabel: "李四",
        latestRejectionReason: "税率依据不完整",
        processVersion: "3",
    })
})
