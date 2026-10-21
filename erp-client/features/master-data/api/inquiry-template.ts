import { apiPost } from "@/lib/api"
import type { InquiryCustomerWorkbookProduct } from "@/lib/inquiry-workbook"

/** 客户模板只使用重新授权后的商品白名单，不复用内部导出数据。 */
export async function fetchInquiryTemplateProducts(
    skuIds: readonly string[],
): Promise<readonly InquiryCustomerWorkbookProduct[]> {
    const rows = await apiPost<
        Array<{
            sku_code: string
            product_name: string
            specification: string | null
            unit: string
        }>
    >("/admin/sales-inquiries/template-products", {
        sku_ids: [...new Set(skuIds)],
    })
    return rows.map((row) => ({
        sku_code: row.sku_code,
        product_name: row.product_name,
        specification: row.specification ?? "",
        unit: row.unit,
    }))
}
