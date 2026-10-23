import { apiGet } from "@/lib/api"
import { compareDecimal } from "@/lib/fixed-decimal"
import { formatEpochDate, throwValidation } from "./mappers"
import type {
    SalesDeliveryAllocation,
    SalesOrderDraftLineInput,
} from "../types"

export type InquirySalesSelection = {
    line_no: number
    inquiry_line_id: string
    price_id: string
    quantity: string
    allocations: Array<{ allocation_id: string; quantity: string }>
}
export type InquirySalesSource = {
    inquiry_id: string
    feedback_round: number
    selections: InquirySalesSelection[]
}
export type InquirySalesSourceSnapshot = {
    inquiry_id: string
    inquiry_no: string
    requirement_version: number
    feedback_round: number
    customer_id: string
    lines: Array<{
        price: { unit_price_gross: string | null }
        sales_order_line_id: string
        inquiry_line_id: string
        quantity: string
        allocations: Array<{
            allocation_id: string
            delivery: SalesDeliveryAllocation
        }>
    }>
}
type BackendInquirySalesBasis = {
    inquiry_id: string
    inquiry_no: string
    requirement_version: number
    feedback_round: number
    customer_id: string
    customer_name: string
    lines: Array<{
        requirement: {
            line_id: string
            sku_id: string
            sku_revision_id: string
            product_name: string
            specification: string
            unit_code: string
            delivery_due_at: number
            sales_reference_price_gross: string | null
            allocations: Array<
                SalesDeliveryAllocation & { allocation_id: string }
            >
        }
        price: { price_id: string; unit_price_gross: string }
        selection: InquirySalesSelection
    }>
}
export type InquirySalesBasis = BackendInquirySalesBasis & {
    customerId: string
    customerName: string
    lineItems: SalesOrderDraftLineInput[]
}

export async function fetchInquirySalesBasis(
    id: string,
    round: number,
): Promise<InquirySalesBasis> {
    const basis = await apiGet<BackendInquirySalesBasis>(
        `/admin/sales-inquiries/${encodeURIComponent(id)}/sales-basis?round=${round}`,
    )
    return {
        ...basis,
        customerId: basis.customer_id,
        customerName: basis.customer_name,
        lineItems: basis.lines.map(({ requirement, price, selection }) => ({
            rowKey: inquiryRowKey(selection),
            name: requirement.product_name,
            sku: requirement.sku_id,
            skuRevisionId: requirement.sku_revision_id,
            specification: requirement.specification,
            unit: requirement.unit_code,
            quantity: selection.quantity,
            unitPriceGross: requirement.sales_reference_price_gross ?? "",
            pricingMode: "MANUAL",
            procurementSupplyPrice: price.unit_price_gross,
            dueDate: formatEpochDate(requirement.delivery_due_at),
            fulfillmentDueAt: requirement.delivery_due_at,
            deliveryAllocations: selection.allocations.map((allocation) => {
                const original = requirement.allocations.find(
                    (row) => row.allocation_id === allocation.allocation_id,
                )
                if (!original)
                    throwValidation("询价配送范围已变化，请返回询价重新核对。")
                return { ...original!, quantity: allocation.quantity }
            }),
            faceValue: "",
            giftRate: "",
            cardForm: "",
        })),
    }
}
export function inquiryRowKey(selection: InquirySalesSelection) {
    return `inquiry-${selection.inquiry_line_id}-${selection.price_id}`
}
function sameAddress(
    left: SalesDeliveryAllocation,
    right: SalesDeliveryAllocation,
) {
    return (
        left.recipient === right.recipient &&
        left.phone === right.phone &&
        left.region_code === right.region_code &&
        left.address === right.address
    )
}

/** 用稳定来源选择重建本次部分开单；服务器仍重新验证全部条件及余额。 */
export function inquirySourceForLines(
    basis: InquirySalesBasis,
    lines: SalesOrderDraftLineInput[],
): InquirySalesSource {
    return {
        inquiry_id: basis.inquiry_id,
        feedback_round: basis.feedback_round,
        selections: lines.map((line, index) => {
            const original = basis.lines.find(
                (item) => inquiryRowKey(item.selection) === line.rowKey,
            )
            const initial = basis.lineItems.find(
                (item) => item.rowKey === line.rowKey,
            )
            if (
                !original ||
                !initial ||
                line.sku !== initial.sku ||
                line.skuRevisionId !== initial.skuRevisionId ||
                line.dueDate !== initial.dueDate
            ) {
                throwValidation(
                    "询价来源商品或交期已变化，请返回询价重新确认。",
                )
            }
            if (
                compareDecimal(line.quantity, original!.selection.quantity, 6) >
                0
            )
                throwValidation(
                    "开单数量超过本次可转数量，请返回询价重新核对。",
                )
            const allocations = (line.deliveryAllocations ?? []).map(
                (delivery) => {
                    const candidates = original!.selection.allocations.filter(
                        (selected) => {
                            const address =
                                original!.requirement.allocations.find(
                                    (row) =>
                                        row.allocation_id ===
                                        selected.allocation_id,
                                )
                            return address && sameAddress(address, delivery)
                        },
                    )
                    if (candidates.length !== 1)
                        throwValidation(
                            "询价来源收货地址已变化，请返回询价重新确认。",
                        )
                    const selected = candidates[0]!
                    if (
                        compareDecimal(
                            delivery.quantity,
                            selected.quantity,
                            6,
                        ) > 0
                    )
                        throwValidation("收货地址数量超过本次可转数量。")
                    return {
                        allocation_id: selected.allocation_id,
                        quantity: delivery.quantity,
                    }
                },
            )
            return {
                ...original!.selection,
                line_no: index + 1,
                quantity: line.quantity,
                allocations,
            }
        }),
    }
}
