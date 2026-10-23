import type { SupplierOfferingView } from "@/features/supplier-offerings/types"
import { compareDecimal } from "@/lib/fixed-decimal"
import type { InquiryLine, ProcurementInquiry } from "../types"
import type { InternalQuote } from "./internal-quotes"
import { inquiryDateInput } from "./format"

/** 两类通用供给价是系统参考，不伪造供应商的运费或交期承诺。 */
export function systemOfferingQuotes(
    inquiry: ProcurementInquiry,
    lines: InquiryLine[],
    offerings: SupplierOfferingView[],
): InternalQuote[] {
    const today = new Date(Date.now() + 8 * 60 * 60 * 1000)
        .toISOString()
        .slice(0, 10)
    return lines.flatMap((line) =>
        offerings
            .filter(
                (offering) =>
                    offering.sku_id === line.sku_id &&
                    offering.status === "ACTIVE" &&
                    offering.current_revision_id &&
                    offering.valid_from &&
                    offering.valid_from <= today &&
                    (!offering.valid_to || offering.valid_to > today) &&
                    offering.availability_status === "AVAILABLE",
            )
            .flatMap((offering) =>
                (["dropship", "bulk"] as const).flatMap((kind) => {
                    const price =
                        kind === "dropship"
                            ? offering.dropship_supply_price_gross
                            : offering.bulk_supply_price_gross
                    if (!price || !offering.input_tax_rate) return []
                    const quantity =
                        offering.available_quantity &&
                        compareDecimal(
                            offering.available_quantity,
                            line.quantity,
                            6,
                        ) < 0
                            ? offering.available_quantity
                            : line.quantity
                    if (
                        compareDecimal(price, "0", 4) <= 0 ||
                        compareDecimal(quantity, "0", 6) <= 0
                    )
                        return []
                    const id = `system:${line.line_id}:${offering.id}:${offering.current_revision_id}:${kind}`
                    return [
                        {
                            id,
                            quote_id: id,
                            revision_no: offering.current_revision_no ?? 1,
                            requirement_version: inquiry.requirement_version,
                            feedback_round: inquiry.feedback_round + 1,
                            created_at: offering.created_at,
                            line_id: line.line_id,
                            supplier_id: offering.supplier_id,
                            supplier_name:
                                offering.supplier_name ?? "供应商名称未匹配",
                            supplier_offering_id: offering.id,
                            system_price_kind: kind,
                            source_revision_id: offering.current_revision_id!,
                            source_issue:
                                kind === "bulk" &&
                                offering.bulk_minimum_order_quantity &&
                                compareDecimal(
                                    line.quantity,
                                    offering.bulk_minimum_order_quantity,
                                    6,
                                ) < 0
                                    ? `未达到集采起订量 ${offering.bulk_minimum_order_quantity}`
                                    : undefined,
                            terms: {
                                unit_cost_gross: price,
                                input_tax_rate: offering.input_tax_rate,
                                available_quantity: quantity,
                                expected_delivery_date: inquiryDateInput(
                                    line.delivery_due_at,
                                ),
                                freight_gross: null,
                                unavailable: false,
                            },
                            internal_note:
                                "系统供给参考价；运费、交期及实际可供数量须由采购核对。",
                        },
                    ]
                }),
            ),
    )
}
