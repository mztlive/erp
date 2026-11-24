import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayName } from "@/lib/display-name"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"

import {
    purchaseDeliveryComparison,
    purchaseSpecification,
} from "./workspace-purchase-comparison-model"

export type PurchaseRequirementCheck = {
    field: "品名与规格" | "品名" | "规格" | "数量" | "交期" | "销售来源"
    status: "difference" | "partial" | "unavailable" | "match"
    result: string
    sales?: string
    purchase?: string
}

type SalesLine = NonNullable<ApprovalPurchaseLine["source"]>

export function checkPurchaseRequirements(
    line: ApprovalPurchaseLine,
): PurchaseRequirementCheck[] {
    if (!line.source) {
        return [
            {
                field: "销售来源",
                status: "unavailable",
                result: "缺少来源明细",
            },
        ]
    }

    return [
        checkProduct(line, line.source),
        checkQuantity(line, line.source),
        checkDelivery(line),
    ]
}

function checkProduct(
    line: ApprovalPurchaseLine,
    source: SalesLine,
): PurchaseRequirementCheck {
    const purchaseTitle = displayName(line.title)
    const salesTitle = displayName(source.title)
    const purchaseSpec = purchaseSpecification(line.specification)
    const salesSpec = purchaseSpecification(source.specification)
    const values = {
        field: "品名与规格" as const,
        sales: [salesTitle ?? "品名未记录", salesSpec]
            .filter(Boolean)
            .join(" · "),
        purchase: [purchaseTitle ?? "品名未记录", purchaseSpec]
            .filter(Boolean)
            .join(" · "),
    }
    if (
        !salesTitle ||
        !purchaseTitle ||
        salesTitle === "未记录商品名称" ||
        purchaseTitle === "未记录商品名称"
    ) {
        return { ...values, status: "unavailable", result: "品名缺失" }
    }
    const nameChanged = purchaseTitle !== salesTitle
    const specChanged = purchaseSpec !== salesSpec
    if (nameChanged && specChanged) {
        return {
            ...values,
            status: "difference",
            result: "品名和规格不符",
        }
    }
    if (nameChanged) {
        return {
            field: "品名",
            status: "difference",
            result: "品名不符",
            sales: salesTitle,
            purchase: purchaseTitle,
        }
    }
    if (specChanged) {
        return {
            field: "规格",
            status: "difference",
            result: "规格不符",
            sales: salesSpec || "未填写",
            purchase: purchaseSpec || "未填写",
        }
    }
    return { ...values, status: "match", result: "品规一致" }
}

function checkQuantity(
    line: ApprovalPurchaseLine,
    source: SalesLine,
): PurchaseRequirementCheck {
    const salesUnit = displayName(source.unitName) ?? displayName(source.unit)
    const purchaseUnit = displayName(line.unitName) ?? displayName(line.unit)
    const values = {
        field: "数量" as const,
        sales: quantityLabel(source.quantity, salesUnit),
        purchase: quantityLabel(line.quantity, purchaseUnit),
    }
    if (!line.unit?.trim() || !source.unit?.trim()) {
        return { ...values, status: "unavailable", result: "单位缺失" }
    }
    if (line.unit.trim() !== source.unit.trim()) {
        return { ...values, status: "unavailable", result: "计量单位不同" }
    }
    if (!line.quantity || !source.quantity) {
        return { ...values, status: "unavailable", result: "数量缺失" }
    }
    try {
        if (
            compareDecimal(line.quantity, "0", 6) <= 0 ||
            compareDecimal(source.quantity, "0", 6) <= 0
        ) {
            return { ...values, status: "unavailable", result: "数量无效" }
        }
        const difference = compactFixed(
            subtractFixed(line.quantity, source.quantity, {
                maxScale: 6,
                outputScale: 6,
            }),
        )
        if (difference.startsWith("-")) {
            return { ...values, status: "partial", result: "部分采购" }
        }
        if (difference !== "0") {
            return {
                ...values,
                status: "difference",
                result: `超需求 ${quantityLabel(difference, salesUnit)}`,
            }
        }
        return { ...values, status: "match", result: "数量一致" }
    } catch {
        return { ...values, status: "unavailable", result: "数量格式无法核对" }
    }
}

function quantityLabel(value: string | null, unit?: string) {
    if (!value?.trim()) return "未填写"
    return [compactFixed(value.trim()), unit].filter(Boolean).join(" ")
}

function checkDelivery(line: ApprovalPurchaseLine): PurchaseRequirementCheck {
    const { salesDate, daysFromDeadline } = purchaseDeliveryComparison(line)
    const values = {
        field: "交期" as const,
        sales: salesDate ?? "未记录",
        purchase: line.expected_delivery_date || "未填写",
    }
    if (daysFromDeadline == null) {
        return { ...values, status: "unavailable", result: "交期无法核对" }
    }
    if (daysFromDeadline > 0) {
        return {
            ...values,
            status: "difference",
            result: `晚 ${daysFromDeadline} 天`,
        }
    }
    return {
        ...values,
        status: "match",
        result:
            daysFromDeadline < 0 ? `提前 ${-daysFromDeadline} 天` : "日期一致",
    }
}
