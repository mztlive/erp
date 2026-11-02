import {
    parsePeriodicTerm,
    periodicPaymentTerm,
    periodicSettlement,
} from "@/lib/supplier-payment-terms"
/**
 * 跨工作面复用的码表选项。
 * 业务页用 OptionCombobox 消费；组件本身不取数。
 */

import type { ComboboxOption } from "@/components/business/option-combobox"

const PREPAY_PAYMENT_TERM_OPTIONS: readonly ComboboxOption[] = [
    { value: "PREPAY_100", label: "先款 100%" },
    { value: "PREPAY_50", label: "先款 50%" },
    { value: "PREPAY_30", label: "先款 30%" },
]

const POSTPAY_PAYMENT_TERM_OPTIONS: readonly ComboboxOption[] = [
    { value: "POSTPAY_NET15", label: "货到 15 天" },
    { value: "POSTPAY_NET30", label: "货到 30 天" },
]

/** 供应商采购付款条件；每一项都能确定计划付款日。 */
export const SUPPLIER_PAYMENT_TERM_OPTIONS: readonly ComboboxOption[] = [
    ...PREPAY_PAYMENT_TERM_OPTIONS,
    { value: "CASH_ON_APPROVAL", label: "现结（审批通过日）" },
    ...POSTPAY_PAYMENT_TERM_OPTIONS,
]

/** 销售与合同付款条件。 */
export const PAYMENT_TERM_OPTIONS: readonly ComboboxOption[] = [
    ...PREPAY_PAYMENT_TERM_OPTIONS,
    ...POSTPAY_PAYMENT_TERM_OPTIONS,
    { value: "CONTRACT", label: "按合同约定" },
]

/**
 * 当前可选福利场景（对齐后端 `WelfareScenario`）。
 * value 为稳定码；展示用 label。
 */
export const WELFARE_SCENARIO_OPTIONS: readonly ComboboxOption[] = [
    {
        value: "ANNUAL_GIFT_BAG",
        label: "礼包",
        keywords: "春节 端午 中秋 国庆 节日 礼品",
    },
    {
        value: "ASSISTANCE_PRODUCT",
        label: "帮扶产品",
        keywords: "帮扶 助农 扶贫",
    },
    {
        value: "CAKE_VOUCHER",
        label: "蛋糕券",
        keywords: "蛋糕 生日 烘焙",
    },
    {
        value: "MOVIE_TICKET",
        label: "电影票",
        keywords: "电影 影院 观影",
    },
    {
        value: "UNION_CONSUMPTION_VOUCHER",
        label: "工会消费券",
        keywords: "工会 消费券",
    },
    {
        value: "MEAL_SUBSIDY",
        label: "餐补",
        keywords: "餐饮 饭补 用餐",
    },
    {
        value: "ACTIVITY",
        label: "活动",
        keywords: "活动 团建 文体",
    },
] as const

/** 历史名称与停用场景只用于读取，不加入新建下拉框。 */
const WELFARE_SCENARIO_LOOKUP: readonly ComboboxOption[] = [
    ...WELFARE_SCENARIO_OPTIONS,
    { value: "ANNUAL_GIFT_BAG", label: "年节礼包" },
    { value: "CONDOLENCE_GIFT", label: "慰问品" },
    { value: "CONSUMPTION_FUND", label: "消费金" },
    { value: "OTHER", label: "其他" },
    { value: "OTHER", label: "其它" },
]

/** 入库质量结果。 */
export const QUALITY_RESULT_OPTIONS: readonly ComboboxOption[] = [
    { value: "合格", label: "合格" },
    { value: "部分合格", label: "部分合格" },
    { value: "不合格", label: "不合格" },
    { value: "待检", label: "待检" },
] as const

/** 承运方。 */
export const CARRIER_OPTIONS: readonly ComboboxOption[] = [
    { value: "顺丰速运", label: "顺丰速运" },
    { value: "中通快递", label: "中通快递" },
    { value: "圆通速递", label: "圆通速递" },
    { value: "京东物流", label: "京东物流" },
    { value: "德邦物流", label: "德邦物流" },
    { value: "货拉拉", label: "货拉拉" },
    { value: "供应商自送", label: "供应商自送" },
] as const

/** 把稳定代码、展示名称或历史别名规范为付款条件代码。 */
export function paymentTermCode(value: string): string | undefined {
    const trimmed = value.trim()
    if (!trimmed) return undefined
    const periodic = parsePeriodicTerm(trimmed)
    if (periodic) return periodic.code
    const option = [
        ...SUPPLIER_PAYMENT_TERM_OPTIONS,
        ...PAYMENT_TERM_OPTIONS,
    ].find(
        (candidate) =>
            candidate.value === trimmed || candidate.label === trimmed,
    )
    if (option) return option.value
    const upper = trimmed.toUpperCase()
    if (["PREPAY-100", "预付款", "先款"].includes(upper)) {
        return "PREPAY_100"
    }
    if (upper === "PREPAY-50") return "PREPAY_50"
    if (upper === "PREPAY-30") return "PREPAY_30"
    if (["CASH-ON-APPROVAL", "现结"].includes(upper)) {
        return "CASH_ON_APPROVAL"
    }
    if (["POSTPAY-NET15", "NET15", "NET-15"].includes(upper)) {
        return "POSTPAY_NET15"
    }
    if (["POSTPAY-NET30", "NET30", "NET-30"].includes(upper)) {
        return "POSTPAY_NET30"
    }
    return undefined
}

/** 判断代码是否属于可形成采购计划付款日的供应商付款条件。 */
export function isSupplierPaymentTermCode(value: string): boolean {
    if (parsePeriodicTerm(value)) return true
    const code = paymentTermCode(value)
    return SUPPLIER_PAYMENT_TERM_OPTIONS.some((option) => option.value === code)
}

/** 按结算方式返回允许维护的供应商付款条件。 */
export function supplierPaymentTermOptionsFor(
    settlementMode: string,
): readonly ComboboxOption[] {
    const cycle = periodicSettlement(settlementMode)
    if (cycle)
        return [
            {
                value: periodicPaymentTerm(cycle.value),
                label: `${cycle.label}，期末后 15 天付款`,
            },
        ]
    const normalized = settlementMode.trim()
    if (["prepayment", "预付款"].includes(normalized)) {
        return PREPAY_PAYMENT_TERM_OPTIONS
    }
    if (["pay_after_use", "先用后付"].includes(normalized)) {
        return POSTPAY_PAYMENT_TERM_OPTIONS
    }
    if (["cash_settlement", "现结"].includes(normalized)) {
        return SUPPLIER_PAYMENT_TERM_OPTIONS.filter(
            (option) => option.value === "CASH_ON_APPROVAL",
        )
    }
    return []
}

/** 判断付款条件是否与结算方式匹配。 */
export function paymentTermMatchesSettlement(
    paymentTerm: string,
    settlementMode: string,
): boolean {
    const cycle = parsePeriodicTerm(paymentTerm)
    if (cycle) return cycle.value === periodicSettlement(settlementMode)?.value
    const code = paymentTermCode(paymentTerm)
    return supplierPaymentTermOptionsFor(settlementMode).some(
        (option) => option.value === code,
    )
}

/** 付款条件代码或历史别名转中文名称。 */
export function paymentTermLabel(code: string): string {
    const cycle = parsePeriodicTerm(code)
    if (cycle) return `${cycle.label}，期末后 ${cycle.days} 天付款`
    const normalized = paymentTermCode(code)
    return (
        [...SUPPLIER_PAYMENT_TERM_OPTIONS, ...PAYMENT_TERM_OPTIONS].find(
            (option) => option.value === normalized,
        )?.label ?? code
    )
}

/** 福利场景码或中文名称转稳定代码，兼容历史名称。 */
export function welfareScenarioCode(raw: string): string | undefined {
    const value = raw.trim()
    if (!value) return undefined
    return WELFARE_SCENARIO_LOOKUP.find(
        (option) => option.value === value || option.label === value,
    )?.value
}

/** 福利场景码 → 中文；未知值原样返回（兼容历史自由文本）。 */
export function welfareScenarioLabel(code: string): string {
    const trimmed = code.trim()
    if (!trimmed) return ""
    const normalized = welfareScenarioCode(trimmed)
    return (
        WELFARE_SCENARIO_LOOKUP.find((option) => option.value === normalized)
            ?.label ?? trimmed
    )
}
