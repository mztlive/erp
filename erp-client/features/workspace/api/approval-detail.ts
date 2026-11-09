import { getApprovalInstance } from "@/features/approval-workflow/api"
import { fetchApprovalMaterials } from "@/features/approval-workflow/api/materials"
import type { ApprovalDocumentSummary } from "@/features/approval-workflow/types"
import { mapWorkspaceWorkItem, startedInstanceToWorkItem } from "./dashboard"
import { getWorkItem } from "@/features/work-items/api"
import type { WorkspaceWorkItem } from "../types"

export type WorkspaceDetail = {
    item: WorkspaceWorkItem
    reworkTask?: WorkspaceWorkItem
}

/** 按实例授权读取当前审批状态及冻结资料，不从当前队列寻找详情。 */
export async function fetchWorkspaceApprovalDetail(
    instanceId: string,
    {
        task,
        canReadRework = true,
    }: { task?: WorkspaceWorkItem; canReadRework?: boolean } = {},
): Promise<WorkspaceDetail> {
    const [instance, materials] = await Promise.all([
        getApprovalInstance(instanceId),
        fetchApprovalMaterials(instanceId),
    ])
    if (
        instance.instanceId !== instanceId ||
        instance.documentId !== materials.document_id ||
        instance.documentType !== materials.document_type ||
        instance.subjectVersion !== String(materials.subject_version)
    )
        throw new Error("审批状态与提交资料不一致，请刷新后重试")
    const source = materials.display.source
    const sections: Array<ApprovalDocumentSummary["summarySections"][number]> =
        source.extra_sections.map((section) => ({
            label: section.label,
            value: section.value,
            numeric: section.numeric,
            objectId: section.object_id ?? undefined,
        }))
    for (const [label, value, numeric] of [
        ["客户", source.customer, false],
        ["含税金额", source.amount_label, true],
        ["提交人", source.submitter_name, false],
    ] as const) {
        if (value && !sections.some((section) => section.label === label))
            sections.push({ label, value, numeric })
    }
    const item = startedInstanceToWorkItem(
        {
            ...instance,
            documentType: materials.document_type,
            documentId: materials.document_id,
            documentLabel: materials.document_no,
            subjectVersion: String(materials.subject_version),
            documentSummaryResolved: true,
            documentSummary: {
                rootBusinessObjectId: materials.display.root_document_id,
                counterpartyName:
                    materials.display.counterparty_label ?? undefined,
                impactSummary: materials.display.impact_summary ?? undefined,
                listSummary: source.list_summary,
                summarySections: sections,
                briefLines: source.lines.map((line) => ({
                    title: line.title,
                    quantity: line.quantity ?? undefined,
                    dueLabel: line.due_label ?? undefined,
                })),
                briefMoreCount: source.more_count,
            },
        },
        "Asia/Shanghai",
    )

    if (task && !matchesSubmission(task, item))
        throw new Error("驳回任务与本次审批资料不一致，请刷新后重试")
    const reworkTask =
        instance.reworkTaskId && canReadRework
            ? task?.workItemId === instance.reworkTaskId
                ? task
                : mapWorkspaceWorkItem(
                      await getWorkItem(instance.reworkTaskId),
                      "Asia/Shanghai",
                  )
            : undefined
    if (reworkTask && !matchesSubmission(reworkTask, item))
        throw new Error("驳回待办与本次审批不一致，请刷新后重试")
    return {
        item,
        reworkTask: reworkTask?.status === "OPEN" ? reworkTask : undefined,
    }
}

/** 任务身份与展示身份分别保留；只允许同一次提交的办理动作。 */
function matchesSubmission(task: WorkspaceWorkItem, item: WorkspaceWorkItem) {
    return (
        task.workItemType === "APPROVAL_REWORK" &&
        task.handlerKey === "approval_rework" &&
        task.approvalProcessInstanceId === item.approvalProcessInstanceId &&
        task.businessObjectType === item.businessObjectType &&
        task.businessObjectId === item.businessObjectId &&
        task.subjectVersion === item.subjectVersion
    )
}

/** 任务详情只读取任务权限；关联审批材料由页面按身份及版本独立加载。 */
export async function fetchWorkspaceTaskDetail(
    workItemId: string,
): Promise<WorkspaceDetail> {
    const item = mapWorkspaceWorkItem(
        await getWorkItem(workItemId),
        "Asia/Shanghai",
    )
    return {
        item,
        reworkTask: item.workItemType === "APPROVAL_REWORK" ? item : undefined,
    }
}
