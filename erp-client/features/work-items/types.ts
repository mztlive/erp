/**
 * 人工任务的前端稳定合同。
 *
 * `WorkItemDto` 与 HTTP JSON 一一对应；页面只能消费经 `mapWorkItemDto`
 * 得到的 `WorkItemProjection`，不得自行推断责任、状态或允许动作。
 */

import {
    displayImpactSummary,
    displayNextActionHint,
    displayOwnerName,
    displayReasonLabel,
} from "./display"
import { displayBusinessText, displayName } from "@/lib/display-name"

export type WorkItemStatus = "OPEN" | "COMPLETED" | "CLOSED"

export type WorkItemScope = "mine" | "managed" | "history"

export type WorkItemProcessingState =
    | "READY"
    | "APPROVAL_BLOCKED"
    | "EXECUTION_BLOCKED"

export type WorkItemAllowedAction =
    | "REASSIGN"
    | "CLOSE"
    | "VIEW"
    | "PROCESS"
    | "APPROVE"
    | "REJECT"
    | "OPEN_DOCUMENT"
    | "RESUME_CURRENT_APPROVER"
    | "CANCEL_BLOCKED_APPROVAL"

export type WorkItemConflictCode =
    | "WORK_ITEM_VERSION_CONFLICT"
    | "WORK_ITEM_RESPONSIBILITY_CONFLICT"

export type WorkItemActionBlockerDto =
    | string
    | Readonly<{ code: string; message: string }>

export type WorkItemPartyDto = Readonly<{
    id: string
    display_name: string
}>

export type WorkItemApprovalContextDto = Readonly<{
    instance_id: string
    status: string
    current_round_no: number
    current_node_label: string
    current_assignee_label?: string | null
    latest_rejection_reason?: string | null
    process_version?: number | null
}>

/** `/admin/work-items` 返回的单条任务。 */
export type WorkItemDto = Readonly<{
    id: string
    work_item_type: string
    handler_key: string
    destination_workspace_id?: string | null
    route_context?: Readonly<{
        confirmation_scope?: string | null
    }> | null
    approval_step_instance_id: string | null
    approval_process_instance_id?: string | null
    approval_node_execution_id?: string | null
    approval_context?: WorkItemApprovalContextDto | null
    status: WorkItemStatus
    assignment_source: string
    owner_role: string
    owner_role_label?: string | null
    owner_organization_id: string
    owner_organization?: WorkItemPartyDto | null
    owner_user_id?: string | null
    owner_user?: WorkItemPartyDto | null
    processing_state: WorkItemProcessingState
    processing_blocker?: Readonly<{ code: string; message: string }> | null
    business_object_type: string
    business_object_id: string
    root_business_object_id: string
    business_object_label?: string | null
    counterparty_label?: string | null
    subject_version: string
    task_version: string
    allowed_actions?: readonly WorkItemAllowedAction[]
    action_blockers?: readonly WorkItemActionBlockerDto[]
    priority: string | number
    due_at?: number | null
    reason_code?: string | null
    reason_label?: string | null
    impact_summary?: string | null
    next_action_hint?: string | null
    summary_sections?: readonly Readonly<{
        label: string
        value: string
        numeric?: boolean
        object_id?: string | null
    }>[]
    brief_lines?: readonly Readonly<{
        title: string
        quantity?: string | null
        due_label?: string | null
    }>[]
    brief_more_count?: number | null
    list_summary?: string | null
    assigned_at?: number | null
    started_at?: number | null
    current_assignment_at?: number | null
    last_activity_at?: number | null
    completed_at?: number | null
    completed_by?: string | null
    closed_at?: number | null
    closed_by?: string | null
    close_reason?: string | null
    created_at: number
    queue_context_id?: string | null
}>

/** 责任命令 409 的权限安全数据；不可见时服务端固定返回 `null`。 */
export type WorkItemConflictDataDto = Readonly<{
    current_work_item: WorkItemDto | null
}>

/** 前端识别后的责任命令冲突。 */
export type WorkItemConflict = Readonly<{
    code: WorkItemConflictCode
    currentWorkItem: WorkItemDto | null
}>

/** 服务端按当前任务完整责任约束筛出的转交候选人。 */
export type WorkItemReassignCandidate = Readonly<{
    user_id: string
    display_name: string
    account: string
}>

export type WorkItemProjection = Readonly<{
    workItemId: string
    workItemType: string
    handlerKey: string
    destinationWorkspaceId?: string
    routeContext?: { confirmationScope?: string }
    approvalStepInstanceId?: string
    approvalProcessInstanceId?: string
    approvalNodeExecutionId?: string
    approvalContext?: Readonly<{
        instanceId: string
        status: string
        currentRoundNo: number
        currentNodeLabel: string
        currentAssigneeLabel?: string
        latestRejectionReason?: string
        processVersion?: string
    }>
    status: WorkItemStatus
    assignmentSource: string
    ownerRole: string
    ownerRoleLabel: string
    ownerOrganization: { id: string; displayName: string }
    ownerUser?: { id: string; displayName: string }
    processingState: WorkItemProcessingState
    processingBlocker?: { code: string; message: string }
    businessObjectType: string
    businessObjectId: string
    rootBusinessObjectId: string
    businessObjectLabel: string
    counterpartyLabel?: string
    subjectVersion: string
    taskVersion: string
    allowedActions: readonly WorkItemAllowedAction[]
    actionBlockers: readonly string[]
    priority: string | number
    dueAt?: number
    reasonCode?: string
    reasonLabel: string
    impactSummary: string
    nextActionHint: string
    summarySections: readonly Readonly<{
        label: string
        value: string
        numeric?: boolean
        objectId?: string
    }>[]
    briefLines: readonly Readonly<{
        title: string
        quantity?: string
        dueLabel?: string
    }>[]
    briefMoreCount?: number
    listSummary?: string
    createdAt: number
    queueContextId?: string
}>

function blockerMessage(blocker: WorkItemActionBlockerDto): string {
    return typeof blocker === "string" ? blocker : blocker.message
}

/** 把 HTTP 字段转换为页面稳定投影，不增加本地动作或责任推断。 */
export function mapWorkItemDto(dto: WorkItemDto): WorkItemProjection {
    const ownerOrganization = dto.owner_organization ?? {
        id: dto.owner_organization_id,
        display_name: dto.owner_role_label ?? "责任组织",
    }

    return {
        workItemId: dto.id,
        workItemType: dto.work_item_type,
        handlerKey: dto.handler_key,
        destinationWorkspaceId: dto.destination_workspace_id ?? undefined,
        routeContext: dto.route_context
            ? {
                  confirmationScope:
                      dto.route_context.confirmation_scope ?? undefined,
              }
            : undefined,
        approvalStepInstanceId: dto.approval_step_instance_id ?? undefined,
        approvalProcessInstanceId:
            dto.approval_process_instance_id ??
            dto.approval_context?.instance_id ??
            undefined,
        approvalNodeExecutionId: dto.approval_node_execution_id ?? undefined,
        approvalContext: dto.approval_context
            ? {
                  instanceId: dto.approval_context.instance_id,
                  status: dto.approval_context.status,
                  currentRoundNo: dto.approval_context.current_round_no,
                  currentNodeLabel:
                      displayName(dto.approval_context.current_node_label) ||
                      "审批节点未标注",
                  currentAssigneeLabel: displayName(
                      dto.approval_context.current_assignee_label,
                  ),
                  latestRejectionReason:
                      dto.approval_context.latest_rejection_reason?.trim() ||
                      undefined,
                  processVersion:
                      dto.approval_context.process_version == null
                          ? undefined
                          : String(dto.approval_context.process_version),
              }
            : undefined,
        status: dto.status,
        assignmentSource: dto.assignment_source,
        ownerRole: dto.owner_role,
        // 服务端始终下发中文 label；缺失时回退通用称呼，不把角色码上屏（AGENTS.md §5）。
        ownerRoleLabel:
            displayName(dto.owner_role_label, dto.owner_role) || "责任人",
        ownerOrganization: {
            id: ownerOrganization.id,
            displayName:
                displayName(
                    ownerOrganization.display_name,
                    ownerOrganization.id,
                ) || "责任组织名称未提供",
        },
        ownerUser: dto.owner_user
            ? {
                  id: dto.owner_user.id,
                  displayName: displayOwnerName(
                      dto.owner_user.display_name,
                      dto.owner_user.id,
                  ),
              }
            : dto.owner_user_id
              ? { id: dto.owner_user_id, displayName: displayOwnerName() }
              : undefined,
        processingState: dto.processing_state,
        processingBlocker: dto.processing_blocker ?? undefined,
        businessObjectType: dto.business_object_type,
        businessObjectId: dto.business_object_id,
        rootBusinessObjectId: dto.root_business_object_id,
        businessObjectLabel:
            displayBusinessText(
                dto.business_object_label,
                dto.business_object_id,
                dto.root_business_object_id,
                dto.id,
            ) || "业务对象名称未提供",
        counterpartyLabel: displayName(dto.counterparty_label),
        subjectVersion: dto.subject_version,
        taskVersion:
            typeof dto.task_version === "string" &&
            /^[1-9]\d*$/.test(dto.task_version)
                ? dto.task_version
                : "",
        allowedActions: dto.allowed_actions ?? [],
        actionBlockers: (dto.action_blockers ?? []).map(blockerMessage),
        priority: dto.priority,
        dueAt: dto.due_at ?? undefined,
        reasonCode: dto.reason_code ?? undefined,
        reasonLabel: displayReasonLabel({
            reasonLabel: dto.reason_label,
            reasonCode: dto.reason_code,
        }),
        impactSummary: displayImpactSummary({
            impactSummary: dto.impact_summary,
            workItemType: dto.work_item_type,
        }),
        nextActionHint: displayNextActionHint({
            nextActionHint: dto.next_action_hint,
            workItemType: dto.work_item_type,
        }),
        summarySections: (dto.summary_sections ?? []).map((section) => ({
            label: section.label,
            value: section.value,
            numeric: section.numeric ?? undefined,
            objectId: section.object_id?.trim() || undefined,
        })),
        briefLines: (dto.brief_lines ?? []).map((line) => ({
            title: line.title,
            quantity: line.quantity ?? undefined,
            dueLabel: line.due_label ?? undefined,
        })),
        briefMoreCount: dto.brief_more_count ?? undefined,
        listSummary: dto.list_summary?.trim() || undefined,
        createdAt: dto.created_at,
        queueContextId: dto.queue_context_id ?? undefined,
    }
}

export type WorkItemResponsibilityCommand =
    | Readonly<{
          kind: "REASSIGN"
          workItemId: string
          expectedTaskVersion: string
          targetUserId: string
          reason: string
          idempotencyKey: string
      }>
    | Readonly<{
          kind: "CLOSE"
          workItemId: string
          expectedTaskVersion: string
          reasonCode: string
          replacementWorkItemId?: string
          comment?: string
          idempotencyKey: string
      }>
