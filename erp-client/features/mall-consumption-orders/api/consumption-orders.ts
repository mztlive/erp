/**
 * W25 商城消费订单 · 真实 HTTP API（queryFn / mutationFn）
 * 路径：/admin/mall-orders、/admin/mall-order-facts、/admin/background-jobs
 * 后端 snake_case + 秒级时间戳 → 前端 camelCase + ISO，适配仅在本文件。
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import type {
    AttributionStatus,
    CostBasis,
    DataSource,
    EmptyReason,
    ExportCommand,
    ExportJobResult,
    FactType,
    FulfillmentChain,
    MallConsumptionOrderListQuery,
    MallConsumptionOrderListResult,
    MallConsumptionOrderMetric,
    MallConsumptionOrderRow,
    MallConsumptionOrderView,
    ProcessingStatus,
    SalesOrderConsumptionSummary,
} from "@/features/mall-consumption-orders/types"
import {
    ATTRIBUTION_STATUS_LABEL,
    COST_BASIS_LABEL,
    DATA_SOURCE_LABEL,
    FACT_TYPE_LABEL,
    FULFILLMENT_CHAIN_LABEL,
    SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"

const BOUNDARY_NOTICE =
    "本页是商城消费记录的只读快照，仅展示支付成功、取消、退款、完成、余额恢复五类结果记录；不是商城可变订单的实时副本，也不是第二个商城订单写入口。"

// ---------------------------------------------------------------------------
// Backend wire types (snake_case)
// ---------------------------------------------------------------------------

type BackendFactSummary = {
    fact_type: string
    latest_occurred_at: number
    count: number
}

type BackendPaymentComposition = {
    card_amount: string
    wechat_amount: string
    source_count: number
}

type BackendCostBasisBreakdown = {
    basis: string
    line_count: number
    cost_amount?: string | null
}

type BackendSupplierOrderSummary = {
    total: number
    statuses: string[]
    has_exception: boolean
}

type BackendListRow = {
    mall_order_id: string
    mall_id: string
    mall_name: string
    external_order_no: string
    customer_id?: string | null
    customer_label?: string | null
    paid_at: number
    paid_amount: string
    payment_composition: BackendPaymentComposition
    fact_summary: BackendFactSummary[]
    fulfillment_chain: string
    supplier_order_summary: BackendSupplierOrderSummary
    attribution_status: string
    cost_basis_breakdown: BackendCostBasisBreakdown[]
    data_source: string
    allowed_actions: string[]
    action_blockers: string[]
    cost_basis_policy_state: string
    normalized_cost_basis?: string | null
}

type BackendFact = {
    fact_id: string
    fact_type: string
    business_fact_key: string
    external_order_version: string
    after_sales_request_id?: string | null
    original_payment_fact_id?: string | null
    occurred_at: number
    received_at: number
    data_source: string
    processing_status: string
}

type BackendItem = {
    mall_order_item_id: string
    external_item_id: string
    sku_id?: string | null
    product_publication_revision_id?: string | null
    supplier_offering_revision_id?: string | null
    name_snapshot: string
    spec_snapshot?: string | null
    quantity: string
    unit_price_gross: string
    line_gross_amount: string
    allocated_discount_amount: string
    allocated_freight_amount: string
    paid_amount: string
    sales_tax_rate: string
    unit_cost_snapshot?: string | null
    cost_snapshot_total?: string | null
    cost_tax_inclusion?: boolean | null
    cost_input_tax_rate?: string | null
    attribution_status: string
}

type BackendPaymentSource = {
    payment_source_id: string
    source_no: number
    source_type: string
    amount: string
    source_reference: string
    mall_card_instance_id?: string | null
    attribution_status: string
    origin?: {
        customer_id?: string | null
        sales_order_id: string
    } | null
}

type BackendFunding = {
    mall_order_item_id: string
    payment_source_id: string
    allocated_payment_amount: string
}

type BackendCostAssessment = {
    assessment_id: string
    assessment_no: number
    cost_basis: string
    basis_source_label: string
    gross_amount?: string | null
    net_amount?: string | null
    tax_amount?: string | null
    tax_inclusion?: boolean | null
    input_tax_rate?: string | null
    assessed_at: number
}

type BackendConsumption = {
    consumption_entry_id: string
    fact_id: string
    item_id: string
    payment_source_id: string
    direction: string
    amount: string
    occurred_at: number
    attribution_status: string
    origin_sales_order_id?: string | null
    reverses_consumption_entry_id?: string | null
    current_cost_assessment?: BackendCostAssessment | null
}

type BackendConservationRow = {
    id: string
    expected: string
    actual: string
    valid: boolean
}

type BackendDetail = {
    identity: {
        mall_order_id: string
        mall_id: string
        mall_name: string
        external_order_no: string
        payment_fact_id: string
    }
    customer: {
        source_customer_ref?: string | null
        customer_id?: string | null
        customer_label?: string | null
        attribution_status: string
    }
    ordered_at: number
    paid_at: number
    amounts: {
        gross: string
        discount: string
        freight: string
        paid: string
        conservation_status: string
    }
    fulfillment: {
        chain: string
        cutover_id?: string | null
        cutover_at?: number | null
        decided_by_occurred_at: number
    }
    facts: BackendFact[]
    items: BackendItem[]
    payment_sources: BackendPaymentSource[]
    funding_allocations: BackendFunding[]
    conservation: {
        item_row_results: BackendConservationRow[]
        source_column_results: BackendConservationRow[]
        order_total: BackendConservationRow
    }
    consumption_entries: BackendConsumption[]
    supplier_orders: Array<{
        supplier_fulfillment_order_id: string
        fulfillment_order_no: string
        supplier_label: string
        item_ids: string[]
        fulfillment_status: string
    }>
    address: { masked_summary: string; reveal_allowed: boolean }
    allowed_actions: string[]
    action_blockers: string[]
}

type BackendBackgroundJob = {
    id: string
    job_no: string
    status: string
    result_expires_at?: number | null
    total_count: number
}

// ---------------------------------------------------------------------------
// Mapping helpers
// ---------------------------------------------------------------------------

function tsToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(secs) || secs <= 0) return ""
    return new Date(secs * 1000).toISOString()
}

function dateToUnixStart(value?: string): number | undefined {
    if (!value) return undefined
    if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
        return Math.floor(new Date(`${value}T00:00:00+08:00`).getTime() / 1000)
    }
    const t = Math.floor(new Date(value).getTime() / 1000)
    return Number.isFinite(t) ? t : undefined
}

function dateToUnixEnd(value?: string): number | undefined {
    if (!value) return undefined
    if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
        return Math.floor(new Date(`${value}T23:59:59+08:00`).getTime() / 1000)
    }
    const t = Math.floor(new Date(value).getTime() / 1000)
    return Number.isFinite(t) ? t : undefined
}

function mapAttribution(raw: string): AttributionStatus {
    switch (raw) {
        case "attributed":
        case "ATTRIBUTED":
            return "ATTRIBUTED"
        case "difference":
        case "DIFFERENCE":
            return "DIFFERENCE"
        case "pending_attribution":
        case "PENDING":
        case "PENDING_ATTRIBUTION":
        default:
            return "PENDING"
    }
}

function attributionToBackend(status: AttributionStatus): string {
    switch (status) {
        case "ATTRIBUTED":
            return "attributed"
        case "DIFFERENCE":
            return "difference"
        case "PENDING":
        default:
            return "pending_attribution"
    }
}

function mapFulfillmentChain(raw: string): FulfillmentChain {
    if (raw === "ERP_AUTOMATED" || raw === "erp_automated")
        return "ERP_AUTOMATED"
    return "LEGACY_MANUAL"
}

function mapDataSource(raw: string): DataSource {
    if (
        raw === "history_backfill" ||
        raw === "BACKFILL" ||
        raw === "HISTORY_BACKFILL"
    )
        return "BACKFILL"
    if (raw === "mixed" || raw === "MIXED") return "MIXED"
    return "REALTIME"
}

function mapDataSourceWire(raw: string): "REALTIME" | "BACKFILL" {
    return mapDataSource(raw) === "BACKFILL" ? "BACKFILL" : "REALTIME"
}

function mapFactType(raw: string): FactType {
    const u = raw.toUpperCase()
    if (u === "ORDER_CANCELED" || u === "ORDER_CANCELLED")
        return "ORDER_CANCELED"
    if (u === "REFUND_SUCCEEDED") return "REFUND_SUCCEEDED"
    if (u === "ORDER_COMPLETED") return "ORDER_COMPLETED"
    if (u === "CARD_BALANCE_RESTORED") return "CARD_BALANCE_RESTORED"
    return "PAYMENT_SUCCEEDED"
}

function mapProcessingStatus(raw: string): ProcessingStatus {
    switch (raw) {
        case "saved":
            return "SAVED"
        case "pending_attribution":
            return "PENDING_ATTRIBUTION"
        case "attributed":
            return "ATTRIBUTED"
        case "difference":
            return "DIFFERENCE"
        case "rejected":
            return "REJECTED"
        default:
            return "SAVED"
    }
}

function mapCostBasis(raw: string): CostBasis {
    const u = raw.toUpperCase()
    if (u === "ACTUAL") return "ACTUAL"
    if (u === "STANDARD") return "STANDARD"
    return "NONE"
}

function mapListRow(row: BackendListRow): MallConsumptionOrderRow {
    const attributionStatus = mapAttribution(row.attribution_status)
    const chain = mapFulfillmentChain(row.fulfillment_chain)
    const costBasisBreakdown = (row.cost_basis_breakdown ?? []).map((b) => ({
        basis: mapCostBasis(b.basis),
        lineCount: b.line_count,
        costAmount: b.cost_amount ?? undefined,
    }))
    const normalized = row.normalized_cost_basis
        ? row.normalized_cost_basis === "MIXED"
            ? ("MIXED" as const)
            : mapCostBasis(row.normalized_cost_basis)
        : undefined

    return {
        mallOrderId: row.mall_order_id,
        mallId: row.mall_id,
        mallName: row.mall_name || row.mall_id,
        externalOrderNo: row.external_order_no,
        customerId: row.customer_id ?? undefined,
        customerLabel: row.customer_label ?? row.customer_id ?? "—",
        paidAt: tsToIso(row.paid_at),
        paidAmount: row.paid_amount,
        paymentComposition: {
            cardAmount: row.payment_composition?.card_amount ?? "0.00",
            wechatAmount: row.payment_composition?.wechat_amount ?? "0.00",
            sourceCount: row.payment_composition?.source_count ?? 0,
        },
        factSummary: (row.fact_summary ?? []).map((f) => ({
            factType: mapFactType(f.fact_type),
            latestOccurredAt: tsToIso(f.latest_occurred_at),
            count: f.count,
        })),
        fulfillmentChain: chain,
        supplierOrderSummary: {
            total: row.supplier_order_summary?.total ?? 0,
            statuses: row.supplier_order_summary?.statuses ?? [],
            hasException: row.supplier_order_summary?.has_exception ?? false,
        },
        attributionStatus,
        costBasisBreakdown,
        dataSource: mapDataSource(row.data_source),
        allowedActions: row.allowed_actions?.length
            ? row.allowed_actions
            : ["OPEN_CENTER", "EXPORT"],
        actionBlockers: (row.action_blockers ?? []).map((message) => ({
            action: "UNKNOWN",
            code: "BACKEND",
            message,
        })),
        costBasisPolicyState:
            row.cost_basis_policy_state === "UNCONFIGURED"
                ? "UNCONFIGURED"
                : "CONFIGURED",
        normalizedCostBasis: normalized,
    }
}

function mapCostAssessment(
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

function mapDetail(d: BackendDetail): MallConsumptionOrderView {
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

function emptyMetrics(): MallConsumptionOrderMetric[] {
    return [
        { key: "paid", label: "支付成功", value: 0, detail: "有支付记录" },
        { key: "pending_attr", label: "待归集", value: 0 },
        { key: "fact_diff", label: "记录差异", value: 0 },
        { key: "auto_exception", label: "自动履约异常", value: 0 },
        { key: "cost_none", label: "成本未覆盖", value: 0 },
    ]
}

function filterSummary(
    query: MallConsumptionOrderListQuery,
    total: number,
): string {
    const parts: string[] = []
    if (query.metric && query.metric !== "all") {
        const labels: Record<string, string> = {
            paid: "支付成功",
            pending_attr: "待归集",
            fact_diff: "记录差异",
            auto_exception: "自动履约异常",
            cost_none: "成本未覆盖",
        }
        parts.push(labels[query.metric] ?? query.metric)
    }
    if (query.mallIds?.length) parts.push(`商城 ${query.mallIds.join("/")}`)
    if (query.fulfillmentChains?.length) {
        parts.push(
            query.fulfillmentChains
                .map((c) => FULFILLMENT_CHAIN_LABEL[c])
                .join("/"),
        )
    }
    if (query.attributionStatuses?.length) {
        parts.push(
            query.attributionStatuses
                .map((s) => ATTRIBUTION_STATUS_LABEL[s])
                .join("/"),
        )
    }
    if (query.occurredFrom || query.occurredTo) {
        parts.push(
            `记录发生 ${query.occurredFrom ?? "…"} ~ ${query.occurredTo ?? "…"}`,
        )
    }
    if (query.factTypes?.length) {
        parts.push(query.factTypes.map((t) => FACT_TYPE_LABEL[t]).join("/"))
    }
    if (query.supplierStatuses?.length) {
        parts.push(
            query.supplierStatuses
                .map((s) => SUPPLIER_STATUS_LABEL[s] ?? s)
                .join("/"),
        )
    }
    if (query.dataSources?.length) {
        parts.push(query.dataSources.map((d) => DATA_SOURCE_LABEL[d]).join("/"))
    }
    if (query.costBases?.length) {
        parts.push(query.costBases.map((b) => COST_BASIS_LABEL[b]).join("/"))
    }
    if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
    parts.push(`${total} 条`)
    return parts.join(" · ")
}

// ---------------------------------------------------------------------------
// Public API (signatures stable for queries.ts)
// ---------------------------------------------------------------------------

/**
 * 销售单协同摘要。后端 P3 未提供按 sales_order_id 聚合接口 → 返回空摘要。
 * 缺口：GET /admin/mall-orders?origin_sales_order_id= 或专用 summary 端点。
 */
export async function fetchSalesOrderConsumptionSummary(
    salesOrderId: string,
): Promise<SalesOrderConsumptionSummary> {
    return {
        salesOrderId,
        orderCount: 0,
        paidAmount: "0.00",
        refundedAmount: "0.00",
        restoredBalanceAmount: "0.00",
    }
}

export async function fetchConsumptionOrderList(
    query: MallConsumptionOrderListQuery,
): Promise<MallConsumptionOrderListResult> {
    const queriedAt = new Date().toISOString()
    const pageSize = Math.max(1, query.pageSize ?? 8)
    const page = Math.max(1, query.page ?? 1)

    // 期间门禁：未选完整起止时不请求全量（与 W25 一致）
    if (!query.occurredFrom || !query.occurredTo) {
        return {
            rows: [],
            pageInfo: { page: 1, pageSize, total: 0 },
            metrics: [],
            malls: [],
            filterSummary: "请先选择记录发生起止时间后查询",
            emptyReason: "FILTER_EMPTY",
            hasModulePermission: true,
            hasDataScope: true,
            permissionVersion: "server",
            dataScopeVersion: "server",
            factWatermark: queriedAt,
            queriedAt,
            boundaryNotice: BOUNDARY_NOTICE,
        }
    }

    const sortParts = (query.sort ?? "").split(".")
    const sortBy =
        sortParts[0] === "occurredAt"
            ? "paid_at"
            : sortParts[0] === "paidAt"
              ? "paid_at"
              : "paid_at"
    const sortDir = sortParts[1] === "asc" ? "asc" : "desc"

    const pageRes = await apiGet<Page<BackendListRow>>("/admin/mall-orders", {
        page,
        page_size: pageSize,
        q: query.q?.trim() || undefined,
        mall_id: query.mallIds?.[0],
        fulfillment_chain: query.fulfillmentChains?.[0],
        attribution_status: query.attributionStatuses?.[0]
            ? attributionToBackend(query.attributionStatuses[0])
            : undefined,
        paid_at_from: dateToUnixStart(query.occurredFrom),
        paid_at_to: dateToUnixEnd(query.occurredTo),
        sort_by: sortBy,
        sort_dir: sortDir,
    })

    const rows = (pageRes.items ?? []).map(mapListRow)
    const total = pageRes.total ?? 0
    const mallsMap = new Map<string, string>()
    for (const r of rows) mallsMap.set(r.mallId, r.mallName)

    let emptyReason: EmptyReason | undefined
    if (total === 0) {
        emptyReason =
            Boolean(query.q?.trim()) ||
            Boolean(query.mallIds?.length) ||
            Boolean(query.fulfillmentChains?.length) ||
            Boolean(query.attributionStatuses?.length)
                ? "FILTER_EMPTY"
                : "NO_DATA"
    }

    return {
        rows,
        pageInfo: {
            page: pageRes.page ?? page,
            pageSize: pageRes.page_size ?? pageSize,
            total,
        },
        // 指标聚合端点未交付 → 零值（backend_gap）
        metrics: emptyMetrics(),
        malls: Array.from(mallsMap.entries()).map(([id, name]) => ({
            id,
            name,
        })),
        filterSummary: filterSummary(query, total),
        emptyReason,
        hasModulePermission: true,
        hasDataScope: true,
        permissionVersion: "server",
        dataScopeVersion: "server",
        factWatermark: queriedAt,
        queriedAt,
        boundaryNotice: BOUNDARY_NOTICE,
    }
}

export async function fetchConsumptionOrderDetail(
    mallOrderId: string,
): Promise<MallConsumptionOrderView | null> {
    try {
        const detail = await apiGet<BackendDetail>(
            `/admin/mall-orders/${encodeURIComponent(mallOrderId)}`,
        )
        return mapDetail(detail)
    } catch (err) {
        const status =
            err && typeof err === "object" && "status" in err
                ? (err as { status?: number }).status
                : undefined
        if (status === 404) return null
        throw err
    }
}

export async function createConsumptionOrderExportJob(
    command: ExportCommand,
): Promise<ExportJobResult> {
    const job = await apiPost<BackendBackgroundJob>("/admin/background-jobs", {
        job_no: `EXP-W25-${command.requestId.slice(-12)}`,
        job_type: "export",
        domain_job_type: "mall_consumption_order_export",
        selection_snapshot_id: command.selectionSnapshotId || null,
        request_id: command.requestId,
        total_count: Math.max(1, command.rowCount || 1),
        items: [
            {
                object_type: "mall_order",
                object_id: command.selectionSnapshotId || command.requestId,
            },
        ],
    })

    return {
        jobId: job.id,
        requestId: command.requestId,
        rowCount: command.rowCount,
        permissionVersion: "server",
        fieldSetId: command.fieldSetId,
        maskDisclaimer:
            "导出使用系统筛选结果与字段权限打码：地址、手机号、完整支付引用、卡号/卡密、未授权成本金额不会以明文写入文件。下载时重新鉴权。",
        expiresAt: job.result_expires_at
            ? tsToIso(job.result_expires_at)
            : new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
        downloadLabel: `商城消费订单_${job.job_no ?? job.id}.csv`,
        status: job.status === "completed" ? "succeeded" : "queued",
    }
}
