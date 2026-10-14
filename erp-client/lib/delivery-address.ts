/** 收货地址行的地区与详细地址。 */
export type DeliveryAddressParts = Readonly<{
    region_name?: string | null
    address: string
}>

/** 拼接「省市区 + 详细地址」；缺少地区名称时只显示详细地址。 */
export function fullDeliveryAddress(row: DeliveryAddressParts): string {
    return `${row.region_name ?? ""}${row.address}`
}
