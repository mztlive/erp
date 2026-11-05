import { apiGet } from "@/lib/api"
import type { BackendDeliveryDetail } from "./documents"
import { deliveryDetailToOperation } from "./hydrate"

/** 按既有发货详情权限读取冻结包裹，并补全商品显示名称。 */
export async function getDeliveryPackingDetail(deliveryId: string) {
    const detail = await apiGet<BackendDeliveryDetail>(
        `/admin/deliveries/${encodeURIComponent(deliveryId)}`,
    )
    const operation = await deliveryDetailToOperation(detail)
    return { detail, operation }
}
