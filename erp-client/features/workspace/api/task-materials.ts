import { fetchApprovalMaterials } from "@/features/approval-workflow/api/materials"
import {
    fetchWorkspaceDocumentFacts,
    type WorkspaceDocumentFacts,
    type WorkspaceFactSection,
} from "../lib/document-facts"
import type { WorkspaceWorkItem } from "../types"

/** 列表仅按任务给出的引用读取材料，不追加任务详情权限或使用最新版本替代。 */
export async function fetchWorkspaceTaskMaterials(
    item: WorkspaceWorkItem,
): Promise<WorkspaceDocumentFacts | null> {
    if (
        !["DOCUMENT_APPROVAL", "APPROVAL_REWORK", "APPROVAL_INSTANCE"].includes(
            item.workItemType,
        )
    )
        return fetchWorkspaceDocumentFacts(item)
    const instanceId =
        item.approvalProcessInstanceId ?? item.approval?.instanceId
    if (!instanceId) throw new Error("审批资料引用缺失，请刷新任务后重试")
    const materials = await fetchApprovalMaterials(instanceId, item)
    const source = materials.display.source
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
    for (const sales of materials.display.source_sales ?? []) {
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
            materials.display.counterparty_label ??
            source.customer ??
            undefined,
        impact: materials.display.impact_summary ?? "",
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
