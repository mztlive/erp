/**
 * 消费订单客户名展示：客户字段按权限打码时，任何用户可见位置
 * （页头标题 / 摘要网格）都不得绕过打码渲染完整客户名。
 */
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"

export function customerLabelFor(view: MallConsumptionOrderView): string {
  return view.fieldPermissions.customer === "masked"
    ? "客户（已打码）"
    : view.customer.customerLabel
}
