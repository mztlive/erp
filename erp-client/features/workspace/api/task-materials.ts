import type {
    ApprovalListMaterial,
    ApprovalListMaterialReference,
} from "@/features/approval-workflow/api/materials"
import type {
    WorkspaceDocumentFacts,
    WorkspaceFactSection,
} from "../lib/document-facts"
import type { WorkspaceWorkItem } from "../types"

/** 审批类列表行读取任务绑定的冻结提交资料。 */
export function usesApprovalListMaterials(item: WorkspaceWorkItem): boolean {
    return [
        "DOCUMENT_APPROVAL",
        "APPROVAL_REWORK",
        "APPROVAL_INSTANCE",
    ].includes(item.workItemType)
}

/** 从列表行提取当前任务已绑定的精确审批引用。 */
export function approvalListMaterialReference(
    item: WorkspaceWorkItem,
): ApprovalListMaterialReference | undefined {
    const instanceId =
        item.approvalProcessInstanceId ?? item.approval?.instanceId
    if (!instanceId) return undefined
    return {
        instanceId,
        businessObjectType: item.businessObjectType,
        businessObjectId: item.businessObjectId,
        subjectVersion: item.subjectVersion,
    }
}

/** 将审批资料摘要转成任务列表展示字段。 */
export function workspaceApprovalListFacts(
    materials: ApprovalListMaterial,
): WorkspaceDocumentFacts {
    const source = materials.source
    const sections: WorkspaceFactSection[] = source.extra_sections.map(
        (section) => ({
            label: section.label,
            value: section.value,
            numeric: section.numeric,
            objectId: section.object_id ?? undefined,
        }),
    )
    for (const [label, value, numeric] of [
        ["客户", source.customer, false],
        ["含税金额", source.amount_label, true],
        ["提交人", source.submitter_name, false],
    ] as const) {
        if (value && !sections.some((section) => section.label === label))
            sections.push({ label, value, numeric })
    }
    for (const sales of materials.source_sales) {
        if (!sections.some((section) => section.objectId === sales.document_id))
            sections.push({
                label: "来源销售单",
                value: sales.document_no,
                objectId: sales.document_id,
            })
    }
    return {
        documentNumber: materials.document_no,
        counterparty:
            materials.counterparty_label ?? source.customer ?? undefined,
        impact: materials.impact_summary ?? "",
        listSummary: source.list_summary,
        sections,
        lines: source.lines.map((line) => ({
            title: line.title,
            quantity: line.quantity ?? undefined,
            dueLabel: line.due_label ?? undefined,
        })),
        moreCount: source.more_count,
    }
}
