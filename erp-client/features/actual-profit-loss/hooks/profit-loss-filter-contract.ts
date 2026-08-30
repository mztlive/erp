export const FULFILLMENT_MODE_CHIP_PREFIX = "fulfillmentMode:"
export const COST_TYPE_CHIP_PREFIX = "costType:"

/** 已生效条件 chip：key 可被 removeFilter 单独撤销。 */
export type ProfitLossAppliedChip = Readonly<{
    key: string
    label: string
}>
