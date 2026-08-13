import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    MASTER_DATA_RESOURCES,
    PRODUCT_KIND_LABELS,
    PRODUCT_KIND_VALUES,
    type MasterDataResource,
    type ProductListSkuSummary,
    type SupplierQualificationHealth,
} from "@/features/master-data/types"

const VALID = new Set(MASTER_DATA_RESOURCES.map((item) => item.key))

const CNY_FORMATTER = new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    minimumFractionDigits: 2,
    maximumFractionDigits: 4,
})

const PRODUCT_KIND_FILTER_OPTIONS = PRODUCT_KIND_VALUES.map((value) => ({
    value,
    label: PRODUCT_KIND_LABELS[value],
}))

const PRODUCT_KIND_RADIO_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    ...PRODUCT_KIND_FILTER_OPTIONS,
] as const

const PRODUCT_LISTING_FILTER_OPTIONS = [
    { value: "listed", label: "全部已上架" },
    { value: "partially_listed", label: "部分已上架" },
    { value: "unlisted", label: "全部未上架" },
] as const

const PRODUCT_COVERAGE_FILTER_OPTIONS = [
    { value: "complete", label: "全部 SKU 有供给" },
    { value: "partial", label: "部分 SKU 有供给" },
    { value: "none", label: "所有 SKU 均无供给" },
] as const

const PRODUCT_LISTING_RADIO_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    ...PRODUCT_LISTING_FILTER_OPTIONS,
] as const

const PRODUCT_COVERAGE_RADIO_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    ...PRODUCT_COVERAGE_FILTER_OPTIONS,
] as const

const LIFECYCLE_RADIO_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "enabled", label: masterDataCopy.lifecycleEnabled },
    { value: "disabled", label: masterDataCopy.lifecycleDisabled },
] as const

const REVISION_TIMING_RADIO_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "current", label: "当前生效" },
    { value: "future", label: "待生效" },
] as const

const SUPPLIER_CAPABILITY_OPTIONS = [
    { value: "physical", label: "实物商品" },
    { value: "virtual", label: "虚拟商品" },
    { value: "offline_service", label: "线下服务" },
    { value: "api", label: "API" },
    { value: "printing", label: "印刷" },
] as const

const SUPPLIER_QUALIFICATION_TYPE_OPTIONS = [
    { value: "certificate", label: "资质证照" },
    { value: "contract", label: "合同" },
    { value: "authorization", label: "授权书" },
    { value: "food_license", label: "食品经营许可证" },
    { value: "legal_person_id", label: "法人身份证" },
] as const

const SUPPLIER_QUALIFICATION_HEALTH_OPTIONS = [
    { value: "all", label: "资质状态：全部" },
    { value: "valid", label: "资质状态：有效" },
    { value: "expiring_30", label: "资质状态：30 天内到期" },
    { value: "expired", label: "资质状态：已过期" },
    { value: "not_registered", label: "资质状态：未登记" },
] as const

/** 读取 URL 中逗号分隔的多选条件，去空、去重并固定排序。 */
function selectedCsvValues(value: string | null): string[] {
    if (!value) return []
    return [
        ...new Set(
            value
                .split(",")
                .map((item) => item.trim())
                .filter(Boolean),
        ),
    ].sort()
}

/** 将多选条件压缩为 URL 与接口共用的逗号分隔值。 */
function csvFilterValue(values: readonly string[]): string | null {
    const normalized = [
        ...new Set(values.map((item) => item.trim()).filter(Boolean)),
    ].sort()
    return normalized.length > 0 ? normalized.join(",") : null
}

/** 返回资质状态的业务文案。 */
function qualificationHealthLabel(
    value: SupplierQualificationHealth | undefined,
): string {
    return (
        SUPPLIER_QUALIFICATION_HEALTH_OPTIONS.find(
            (option) => option.value === value,
        )?.label.replace("资质状态：", "") ?? "全部"
    )
}

/** 仅保留当前页面已声明的多选枚举值，避免 URL 中的无效值成为隐形状态。 */
function selectedSupplierOptionValues(
    value: string | null,
    options: readonly { value: string; label: string }[],
): string[] {
    return selectedCsvValues(value).filter((item) =>
        options.some((option) => option.value === item),
    )
}

/** 把已选固定枚举代码转换为业务文案，用于导出筛选摘要。 */
function selectedSupplierOptionLabels(
    values: readonly string[],
    options: readonly { value: string; label: string }[],
): string[] {
    return values.flatMap((value) => {
        const label = options.find((option) => option.value === value)?.label
        return label ? [label] : []
    })
}

/** 校验销售价输入，并使用分值整数比较上下界，避免浮点误差。 */
function productSalesPriceRangeError(
    minimum: string,
    maximum: string,
): string | null {
    const pricePattern = /^\d+(?:\.\d{1,2})?$/
    if (minimum && !pricePattern.test(minimum)) {
        return "最低价应为最多两位小数的非负金额"
    }
    if (maximum && !pricePattern.test(maximum)) {
        return "最高价应为最多两位小数的非负金额"
    }
    const normalizedParts = (value: string): readonly [string, string] => {
        const [yuan, fraction = ""] = value.split(".")
        return [yuan.replace(/^0+(?=\d)/, ""), fraction.padEnd(2, "0")]
    }
    if (minimum && maximum) {
        const [minimumYuan, minimumFraction] = normalizedParts(minimum)
        const [maximumYuan, maximumFraction] = normalizedParts(maximum)
        const minimumIsHigher =
            minimumYuan.length > maximumYuan.length ||
            (minimumYuan.length === maximumYuan.length &&
                (minimumYuan > maximumYuan ||
                    (minimumYuan === maximumYuan &&
                        minimumFraction > maximumFraction)))
        if (minimumIsHigher) return "最低价不能高于最高价"
    }
    return null
}

function productSkuPriceRange(skus: readonly ProductListSkuSummary[]): string {
    const prices = skus
        .flatMap((sku) => {
            const raw = sku.salesVisiblePriceGross?.trim()
            if (!raw) return []
            const price = Number(raw)
            return Number.isFinite(price) ? [price] : []
        })
        .sort((left, right) => left - right)
    if (prices.length === 0) return "未填写"
    const minimum = CNY_FORMATTER.format(prices[0])
    const maximum = CNY_FORMATTER.format(prices[prices.length - 1])
    return prices[0] === prices[prices.length - 1]
        ? minimum
        : `${minimum}–${maximum}`
}

const CREATE_PERMISSION_BY_RESOURCE: Partial<
    Record<MasterDataResource, string>
> = {
    products: "product:create",
    categories: "product_category:create",
    brands: "product_brand:create",
    "unit-of-measures": "unit_of_measure:create",
    "voucher-categories": "voucher_category_profile:create",
    suppliers: "supplier:create",
    warehouses: "warehouse:create",
}

function isResource(value: string): value is MasterDataResource {
    return VALID.has(value as MasterDataResource)
}

export {
    CREATE_PERMISSION_BY_RESOURCE,
    csvFilterValue,
    isResource,
    LIFECYCLE_RADIO_FILTER_OPTIONS,
    productSalesPriceRangeError,
    productSkuPriceRange,
    PRODUCT_COVERAGE_FILTER_OPTIONS,
    PRODUCT_COVERAGE_RADIO_FILTER_OPTIONS,
    PRODUCT_KIND_FILTER_OPTIONS,
    PRODUCT_KIND_RADIO_FILTER_OPTIONS,
    PRODUCT_LISTING_FILTER_OPTIONS,
    PRODUCT_LISTING_RADIO_FILTER_OPTIONS,
    qualificationHealthLabel,
    REVISION_TIMING_RADIO_FILTER_OPTIONS,
    selectedCsvValues,
    selectedSupplierOptionLabels,
    selectedSupplierOptionValues,
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_QUALIFICATION_HEALTH_OPTIONS,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
}
