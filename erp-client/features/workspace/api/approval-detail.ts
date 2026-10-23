import { getApprovalInstance } from "@/features/approval-workflow/api"
import { fetchApprovalMaterials } from "@/features/approval-workflow/api/materials"
import type { ApprovalDocumentSummary } from "@/features/approval-workflow/types"
import { startedInstanceToWorkItem } from "./dashboard"

/** 按实例授权读取当前审批状态及冻结资料，不从当前队列寻找详情。 */
export async function fetchWorkspaceApprovalDetail(instanceId: string) {
    const [instance, materials] = await Promise.all([
        getApprovalInstance(instanceId),
        fetchApprovalMaterials(instanceId),
    ])
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
    return startedInstanceToWorkItem(
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
}
