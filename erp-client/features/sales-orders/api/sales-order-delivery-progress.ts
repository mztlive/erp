import { apiGet } from "@/lib/api"
import { buildOrderProgress } from "../lib/acceptance-model"
import {
    mapSalesLine,
    type BackendEligibilityView,
} from "../lib/acceptance-mappers"

/** 读取实际交付与净验收数量；失败向上传递，不以零数量替代。 */
export async function fetchSalesOrderDeliveryProgress(salesOrderId: string) {
    const view = await apiGet<BackendEligibilityView>(
        "/admin/customer-acceptances/eligible",
        { sales_order_id: salesOrderId },
    )
    return buildOrderProgress(view.sales_lines.map(mapSalesLine))
}
