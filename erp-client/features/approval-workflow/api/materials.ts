import type { InvoiceRequestData } from "@/lib/invoice-request-data"
import type { SalesLinePrices } from "@/lib/sales-line-prices"
import { apiGet, apiGetBlob, apiPost } from "@/lib/api"
import { downloadBlob } from "@/lib/download-blob"

/** 仅提交时冻结的展示与文件白名单；关联对象标识不构成读取入口。 */
export type ApprovalMaterials = {
    document_no: string
    /** 最新登记评估，独立于提交时冻结的供应商名称。 */
    latest_supplier_rating?: {
        rating_label: string | null
        initial_score: number | null
        current_score: number | null
        revision_no: number | null
        valid_from: string | null
        valid_to: string | null
    } | null
    sales_order?: ApprovalSalesSubmission | null
    purchase_lines?: ApprovalPurchaseLine[] | null
    /** 费用采购单：来源销售单已生效的附加费用合计（不含本次提交）。 */
    extra_cost_summary?: {
        sales_order_id: string
        occurred_gross_total: string
        pending_gross_total: string
    } | null
    source_sales_orders?: {
        document_no: string
        revision: ApprovalSalesRevision
    }[]
    current_sales_basis_error?: string | null
    current_sales_basis?: {
        source: NonNullable<
            ApprovalMaterials["display"]["source_sales"]
        >[number]
        sales_order: { document_no: string; revision: ApprovalSalesRevision }
        attachments: ApprovalMaterials["attachments"]
        unavailable_reason: string | null
    } | null
    subject_version: number
    document_type: string
    document_id: string
    display: {
        invoice_request?: InvoiceRequestData | null
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
    /** 行含税金额；费用行按此核对。 */
    gross_amount?: string
    expected_delivery_date: string | null
    /** 本次提交保存的税率（小数）与采购行承担的交付安排。 */
    input_tax_rate?: string | null
    delivery_allocations?: {
        recipient: string
        phone: string
        region_code: string
        region_name: string
        address: string
        quantity: string
    }[]
    source: {
        sales_prices?: SalesLinePrices | null
        revision_id: string
        title: string
        specification: string | null
        quantity: string
        unit: string
        unit_price_gross: string
        /** 销售填写的采购供给价（含税单价）；未填写为空。 */
        procurement_supply_price_gross?: string | null
        original_procurement_supply_price_gross?: string | null
        procurement_price_basis?: "BULK" | "DROPSHIP" | null
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
        unit_name?: string
        welfare_scenario?: string | null
        pricing_mode?: "MANUAL" | "GIFT"
        quantity: string | null
        delivery_allocations?: {
            recipient: string
            phone: string
            region_code: string
            region_name?: string
            address: string
            quantity: string
        }[]
        unit_price_gross: string | null
        gross_amount: string
        net_amount: string
        tax_amount: string
        sales_tax_rate: string
        fulfillment_due_at: number | null
        /** 销售填写的采购供给价（含税单价）；未填写时缺省。 */
        procurement_supply_price_gross?: string | null
        original_procurement_supply_price_gross?: string | null
        procurement_price_basis?: "BULK" | "DROPSHIP" | null
        face_value: string | null
        card_count: number | null
        card_form: string | null
    }[]
}

const materialsPath = (instanceId: string) =>
    `/admin/approval-instances/${encodeURIComponent(instanceId)}/materials`

export type MaterialReference = Readonly<{
    businessObjectType: string
    businessObjectId: string
    subjectVersion: string
}>

export type ApprovalListMaterialReference = MaterialReference & {
    instanceId: string
}

export type ApprovalListMaterial = {
    document_no: string
    document_type: string
    document_id: string
    subject_version: number
    counterparty_label: string | null
    impact_summary: string | null
    source: ApprovalMaterials["display"]["source"]
    source_sales: Array<{ document_id: string; document_no: string }>
}

type ApprovalListMaterialResult =
    | { status: "available"; materials: ApprovalListMaterial }
    | { status: "unavailable" | "reference_mismatch" | "failed" }

/** 批量读取本页审批引用的冻结摘要，返回顺序与引用一致。 */
export async function fetchApprovalListMaterials(
    references: readonly ApprovalListMaterialReference[],
    signal?: AbortSignal,
): Promise<ApprovalListMaterialResult[]> {
    const results = await apiPost<ApprovalListMaterialResult[]>(
        "/admin/approval-instances/materials/batch",
        {
            references: references.map((reference) => ({
                instance_id: reference.instanceId,
                document_type: reference.businessObjectType,
                document_id: reference.businessObjectId,
                subject_version: reference.subjectVersion,
            })),
        },
        { signal, cache: "no-store" },
    )
    if (results.length !== references.length)
        throw new Error("审批资料返回不完整，请重试")

    return results.map((result, index) => {
        if (result.status !== "available") return result
        const reference = references[index]
        const materials = result.materials
        return materials.document_type === reference.businessObjectType &&
            materials.document_id === reference.businessObjectId &&
            String(materials.subject_version) === reference.subjectVersion
            ? result
            : { status: "reference_mismatch" }
    })
}

/** 独立授权的冻结材料必须匹配任务绑定的单据及提交版本。 */
export async function fetchApprovalMaterials(
    instanceId: string,
    reference?: MaterialReference,
): Promise<ApprovalMaterials> {
    const materials = await apiGet<ApprovalMaterials>(
        materialsPath(instanceId),
        {
            cache: "no-store",
        },
    )
    if (
        reference &&
        (materials.document_id !== reference.businessObjectId ||
            materials.document_type !== reference.businessObjectType ||
            String(materials.subject_version) !== reference.subjectVersion)
    )
        throw new Error("关联资料与当前任务版本不一致，请刷新任务后重试")
    return materials
}

export const fetchApprovalMaterialPreview = (
    instanceId: string,
    assetId: string,
    sourceRevisionId?: string,
) =>
    apiGetBlob(
        materialContentPath(instanceId, assetId, "preview", sourceRevisionId),
        { timeoutMs: 30_000, cache: "no-store" },
    )

/** 每次下载重新核验审批关系与冻结文件，不调用通用附件授权接口。 */
export async function downloadApprovalMaterial(
    instanceId: string,
    assetId: string,
    fileName: string,
    sourceRevisionId?: string,
): Promise<void> {
    const blob = await apiGetBlob(
        materialContentPath(instanceId, assetId, "download", sourceRevisionId),
        { timeoutMs: 30_000, cache: "no-store" },
    )
    downloadBlob(blob, fileName)
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

function materialContentPath(
    instanceId: string,
    assetId: string,
    action: "preview" | "download",
    sourceRevisionId?: string,
) {
    const query = sourceRevisionId
        ? `?${new URLSearchParams({ source_revision_id: sourceRevisionId })}`
        : ""
    return `${materialsPath(instanceId)}/${encodeURIComponent(assetId)}/${action}${query}`
}
