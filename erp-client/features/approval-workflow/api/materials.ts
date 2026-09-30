import { apiGet, apiGetBlob } from "@/lib/api"

/** 仅提交时冻结的展示与文件白名单；关联对象标识不构成读取入口。 */
export type ApprovalMaterials = {
    subject_version: number
    document_type: string
    document_id: string
    display: {
        root_document_id: string
        counterparty_label: string | null
        impact_summary: string | null
        source: {
            customer: string | null
            amount_label: string | null
            submitter_name: string | null
            list_summary: string
            lines: {
                title: string
                quantity: string | null
                due_label: string | null
            }[]
            more_count: number
            extra_sections: {
                label: string
                value: string
                numeric: boolean
                object_id: string | null
            }[]
        }
    }
    attachments: {
        file_asset_id: string
        file_name: string
        content_type: string
        byte_size: number
    }[]
}

const materialsPath = (instanceId: string) =>
    `/admin/approval-instances/${encodeURIComponent(instanceId)}/materials`

export const fetchApprovalMaterials = (instanceId: string) =>
    apiGet<ApprovalMaterials>(materialsPath(instanceId))

export const fetchApprovalMaterialPreview = (
    instanceId: string,
    assetId: string,
) =>
    apiGetBlob(
        `${materialsPath(instanceId)}/${encodeURIComponent(assetId)}/preview`,
        { timeoutMs: 30_000, cache: "no-store" },
    )

/** 每次下载重新核验审批关系与冻结文件，不调用通用附件授权接口。 */
export async function downloadApprovalMaterial(
    instanceId: string,
    assetId: string,
    fileName: string,
): Promise<void> {
    const blob = await apiGetBlob(
        `${materialsPath(instanceId)}/${encodeURIComponent(assetId)}/download`,
        { timeoutMs: 30_000, cache: "no-store" },
    )
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = fileName
    document.body.append(anchor)
    anchor.click()
    anchor.remove()
    URL.revokeObjectURL(url)
}
