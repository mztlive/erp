/**
 * W25 商城消费订单 · 详情视图映射（BackendDetail → MallConsumptionOrderView）。
 * 列表与共享映射见 mapping.ts。
 */

import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import { BOUNDARY_NOTICE } from "./constants"
import {
    mapAttribution,
    mapCostBasis,
    mapDataSourceWire,
    mapFactType,
    mapFulfillmentChain,
    mapProcessingStatus,
    tsToIso,
} from "./mapping"
import type {
    BackendCostAssessment,
    BackendDetail,
} from "./wire-types"

export function mapCostAssessment(
    a: BackendCostAssessment | null | undefined,
): MallConsumptionOrderView["consumptionEntries"][number]["currentCostAssessment"] {
    if (!a) {
        return {
            assessmentId: "",
            assessmentNo: 0,
            costBasis: "NONE",
            basisSourceLabel: "—",
            assessedAt: "",
        }
    }
    return {
        assessmentId: a.assessment_id,
        assessmentNo: a.assessment_no,
        costBasis: mapCostBasis(a.cost_basis),
        basisSourceLabel: a.basis_source_label,
        grossAmount: a.gross_amount ?? undefined,
        netAmount: a.net_amount ?? undefined,
        taxAmount: a.tax_amount ?? undefined,
        taxInclusion:
            a.tax_inclusion == null
                ? undefined
                : a.tax_inclusion
                  ? "含税"
                  : "不含税",
        inputTaxRate: a.input_tax_rate ?? undefined,
        assessedAt: tsToIso(a.assessed_at),
    }
}

export function mapDetail(d: BackendDetail): MallConsumptionOrderView {
    const queriedAt = new Date().toISOString()
    const conservationStatus =
        d.amounts.conservation_status === "DIFFERENCE" ||
        d.amounts.conservation_status === "difference"
            ? "DIFFERENCE"
            : "VALID"

    return {
        identity: {
            mallOrderId: d.identity.mall_order_id,
            mallId: d.identity.mall_id,
            mallName: d.identity.mall_name || d.identity.mall_id,
            externalOrderNo: d.identity.external_order_no,
            paymentFactId: d.identity.payment_fact_id,
        },
        customer: {
            sourceCustomerRef: d.customer.source_customer_ref ?? "",
            customerId: d.customer.customer_id ?? undefined,
            customerLabel:
                d.customer.customer_label ?? d.customer.customer_id ?? "—",
            attributionStatus: mapAttribution(d.customer.attribution_status),
        },
        orderedAt: tsToIso(d.ordered_at),
        paidAt: tsToIso(d.paid_at),
        amounts: {
            gross: d.amounts.gross,
            discount: d.amounts.discount,
            freight: d.amounts.freight,
            paid: d.amounts.paid,
            conservationStatus,
        },
        fulfillment: {
            chain: mapFulfillmentChain(d.fulfillment.chain),
            cutoverId: d.fulfillment.cutover_id ?? "",
            cutoverAt: tsToIso(d.fulfillment.cutover_at ?? undefined),
            decidedByOccurredAt: tsToIso(d.fulfillment.decided_by_occurred_at),
        },
        facts: (d.facts ?? []).map((f) => ({
            factId: f.fact_id,
            factType: mapFactType(f.fact_type),
            businessFactKeySummary: f.business_fact_key,
            externalOrderVersion: f.external_order_version,
            afterSalesRequestId: f.after_sales_request_id ?? undefined,
            originalPaymentFactId: f.original_payment_fact_id ?? undefined,
            occurredAt: tsToIso(f.occurred_at),
            receivedAt: tsToIso(f.received_at),
            dataSource: mapDataSourceWire(f.data_source),
            processingStatus: mapProcessingStatus(f.processing_status),
            resultDetails: {},
        })),
        items: (d.items ?? []).map((it) => ({
            mallOrderItemId: it.mall_order_item_id,
            externalItemId: it.external_item_id,
            skuId: it.sku_id ?? undefined,
            productPublicationRevisionId:
                it.product_publication_revision_id ?? undefined,
            supplierOfferingRevisionId:
                it.supplier_offering_revision_id ?? undefined,
            nameSnapshot: it.name_snapshot,
            specSnapshot: it.spec_snapshot ?? "",
            quantity: it.quantity,
            unitPriceGross: it.unit_price_gross,
            lineGrossAmount: it.line_gross_amount,
            allocatedDiscountAmount: it.allocated_discount_amount,
            allocatedFreightAmount: it.allocated_freight_amount,
            paidAmount: it.paid_amount,
            salesTaxRate: it.sales_tax_rate,
            unitCostSnapshot: it.unit_cost_snapshot ?? undefined,
            costSnapshotTotal: it.cost_snapshot_total ?? undefined,
            costTaxInclusion:
                it.cost_tax_inclusion == null
                    ? undefined
                    : it.cost_tax_inclusion
                      ? "含税"
                      : "不含税",
            costInputTaxRate: it.cost_input_tax_rate ?? undefined,
            attributionStatus: mapAttribution(it.attribution_status),
        })),
        paymentSources: (d.payment_sources ?? []).map((ps) => ({
            paymentSourceId: ps.payment_source_id,
            sourceNo: ps.source_no,
            sourceType: ps.source_type === "WECHAT" ? "WECHAT" : "CARD",
            amount: ps.amount,
            sourceReference: ps.source_reference,
            mallCardInstanceId: ps.mall_card_instance_id ?? undefined,
            attributionStatus: mapAttribution(ps.attribution_status),
            origin: ps.origin
                ? {
                      customerId: ps.origin.customer_id ?? "",
                      customerLabel: ps.origin.customer_id ?? "—",
                      salesOrderId: ps.origin.sales_order_id,
                      salesOrderNo: ps.origin.sales_order_id,
                      salesOrderLineId: "",
                  }
                : undefined,
        })),
        fundingAllocations: (d.funding_allocations ?? []).map((fa) => ({
            mallOrderItemId: fa.mall_order_item_id,
            paymentSourceId: fa.payment_source_id,
            allocatedPaymentAmount: fa.allocated_payment_amount,
        })),
        conservation: {
            itemRowResults: (d.conservation?.item_row_results ?? []).map(
                (r) => ({
                    mallOrderItemId: r.id,
                    expected: r.expected,
                    actual: r.actual,
                    valid: r.valid,
                }),
            ),
            sourceColumnResults: (
                d.conservation?.source_column_results ?? []
            ).map((r) => ({
                paymentSourceId: r.id,
                expected: r.expected,
                actual: r.actual,
                valid: r.valid,
            })),
            orderTotal: {
                expected: d.conservation?.order_total?.expected ?? "0.00",
                actual: d.conservation?.order_total?.actual ?? "0.00",
                valid: d.conservation?.order_total?.valid ?? true,
            },
        },
        consumptionEntries: (d.consumption_entries ?? []).map((ce) => ({
            consumptionEntryId: ce.consumption_entry_id,
            factId: ce.fact_id,
            itemId: ce.item_id,
            paymentSourceId: ce.payment_source_id,
            direction:
                ce.direction === "reversal" || ce.direction === "REVERSAL"
                    ? "REVERSAL"
                    : "CONSUMPTION",
            amount: ce.amount,
            occurredAt: tsToIso(ce.occurred_at),
            attributionStatus: mapAttribution(ce.attribution_status),
            originSalesOrderId: ce.origin_sales_order_id ?? undefined,
            reversesConsumptionEntryId:
                ce.reverses_consumption_entry_id ?? undefined,
            currentCostAssessment: mapCostAssessment(
                ce.current_cost_assessment,
            ),
        })),
        supplierOrders: (d.supplier_orders ?? []).map((so) => ({
            supplierFulfillmentOrderId: so.supplier_fulfillment_order_id,
            fulfillmentOrderNo: so.fulfillment_order_no,
            supplierLabel: so.supplier_label,
            itemIds: so.item_ids ?? [],
            fulfillmentStatus:
                so.fulfillment_status as MallConsumptionOrderView["supplierOrders"][number]["fulfillmentStatus"],
            cancelStatus: "NONE",
            refundStatus: "NONE",
        })),
        address: {
            maskedSummary: d.address?.masked_summary ?? "—",
            revealAllowed: d.address?.reveal_allowed ?? false,
        },
        phoneMasked: "—",
        paymentRefMasked: "—",
        freshness: {
            factWatermark: queriedAt,
            attributionUpdatedAt: queriedAt,
            queriedAt,
        },
        allowedActions: d.allowed_actions?.length
            ? d.allowed_actions
            : ["OPEN_CENTER"],
        actionBlockers: (d.action_blockers ?? []).map((message) => ({
            action: "UNKNOWN",
            code: "BACKEND",
            message,
        })),
        fieldPermissions: {},
        boundaryNotice: BOUNDARY_NOTICE,
        workItemIds: [],
    }
}
