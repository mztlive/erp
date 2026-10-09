import { apiGet, apiGetBlob } from "@/lib/api"

/** 仅提交时冻结的展示与文件白名单；关联对象标识不构成读取入口。 */
export type ApprovalMaterials = {
    document_no: string
    sales_order?: ApprovalSalesSubmission | null
    purchase_lines?: ApprovalPurchaseLine[] | null
    source_sales_orders?: {
        document_no: string
        revision: ApprovalSalesRevision
    }[]
    subject_version: number
    document_type: string
    document_id: string
    display: {
        root_document_id: string
        counterparty_label: string | null
        impact_summary: string | null
        source_sales?: {
            document_id: string
            document_no: string
            revision_id: string
            revision_no: number
            source: ApprovalMaterials["display"]["source"]
        }[]
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

/** 精确采购提交与冻结销售版本的逐行对照，不按商品名称拼接。 */
export type ApprovalPurchaseLine = {
    id: string
    line_no: number
    title: string
    specification: string | null
    quantity: string | null
    unit: string | null
    unit_cost_gross: string | null
    expected_delivery_date: string | null
    source: {
        revision_id: string
        title: string
        specification: string | null
        quantity: string
        unit: string
        unit_price_gross: string
        fulfillment_due_at: number
    } | null
}

/** 审批实例授权返回的精确销售提交，不依赖普通销售单详情。 */
export type ApprovalSalesSubmission = {
    submission_no: number
    business_type: "GOODS_SERVICE" | "VOUCHER"
    customer_name: string
    contract_no: string | null
    settlement_party_name: string | null
    payment_term_name: string
    invoice_type: string
    tax_point: string
    project_name: string | null
    business_remark: string | null
    voucher_expiry_at: number | null
    receivable_due_date: string | null
    gross_amount: string
    net_amount: string
    tax_amount: string
    submitted_by: string
    submitted_at: number
    lines: {
        id: string
        line_no: number
        item_name_snapshot: string
        sku_id: string | null
        spec_snapshot: string | null
        unit_snapshot: string | null
        quantity: string | null
        unit_price_gross: string | null
        gross_amount: string
        net_amount: string
        tax_amount: string
        sales_tax_rate: string
        fulfillment_due_at: number | null
        service_region: string | null
        face_value: string | null
        card_count: number | null
        card_form: string | null
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

/** 采购审批锁定的完整销售版本，复用销售单纸质预览。 */
export type ApprovalSalesRevision = Omit<
    ApprovalSalesSubmission,
    | "submission_no"
    | "business_type"
    | "submitted_by"
    | "submitted_at"
    | "receivable_due_date"
    | "lines"
> & {
    id: string
    revision_no: number
    effective_at: number
    voucher_category_sku_id: string | null
    commercial_lines: ApprovalSalesSubmission["lines"]
}
