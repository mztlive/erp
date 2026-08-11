/**
 * W05 销售单 HTTP API（queryFn / mutationFn 纯函数）。
 *
 * 后端域：sales_order / sales_review / work_item / bulk_job。
 * 在本文件内将后端 DTO 适配为既有前端视图契约（types.ts / queries.ts 不变）。
 * 失败统一抛 ApiError（@/lib/api），禁止 throw new Error("string")。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import {
    PAYMENT_TERM_OPTIONS,
    WELFARE_SCENARIO_OPTIONS,
    welfareScenarioLabel,
} from "@/lib/business-options"
import type {
    CreateSalesOrderInput,
    CreateSalesOrderResult,
    SalesChangeOrderSummary,
    SalesOrderContractInput,
    SalesOrderDraftLineInput,
    SalesOrderNature,
} from "@/features/sales-orders/types"
import {
    PERMISSION_VERSION,
    type BackendContractDetail,
    type BackendCustomerDetail,
    type BackendPartyContact,
    type BackendProcurementConfirmation,
    type BackendSalesChangeOrder,
    type BackendSalesOrderDetail,
    type BackendSalesOrderReview,
    type BackendSalesOrderView,
    type BackendWorkingCopy,
    type PageView,
    type ProcurementResolutionOutcome,
    type SalesOrderDetailView,
    type SalesOrderListView,
    type SalesOrdersListQuery,
} from "@/features/sales-orders/api/contracts"
import {
    dateToUnixSecs,
    formatEpochDate,
    formatInstant,
    formatIsoNow,
    localOrderNo,
    mapCardForm,
    mapCardFormFromBackend,
    mapChangeOrder,
    mapDetailToListItem,
    mapFulfillmentMode,
    mapListItemFromBackend,
    mapNature,
    mapRejectedProcurement,
    mapReviewToCardApproval,
    mapSortBy,
    mapStatusFilterToBackend,
    mapWelfareScenarioCode,
    percentToRate,
    rateToPercent,
    throwValidation,
} from "@/features/sales-orders/api/mappers"

export type {
    SalesOrderDetailView,
    SalesOrderListView,
    SalesOrdersListQuery,
} from "@/features/sales-orders/api/contracts"

export {
    claimCardSalesApproval,
    completeCardSalesApproval,
} from "@/features/sales-orders/api/card-approval"
export { createSalesOrderExportJob } from "@/features/sales-orders/api/export"

// ─── 列表 / 详情 ─────────────────────────────────────────────────────────────

export async function fetchSalesOrders(
    query: SalesOrdersListQuery,
): Promise<SalesOrderListView> {
    const statusMap = mapStatusFilterToBackend(
        query.status && query.status !== "all" ? query.status : undefined,
    )
    const businessType =
        query.nature === "card_voucher"
            ? "VOUCHER"
            : query.nature === "physical_service"
              ? "GOODS_SERVICE"
              : undefined

    // "待我处理"/"我创建的" 都按创建人过滤；"待我处理"额外限定草稿/驳回回销售，
    // "异常"限定审核轨被驳回。三者与 commercial_status/review_status 互斥，
    // 与 status 高级筛选是两回事（summary 是固定视图，status 是任意阶段码）。
    const createdBy =
        query.summary === "mine" || query.summary === "createdByMe"
            ? query.currentUserId?.trim() || undefined
            : undefined
    const myTodo = query.summary === "mine"
    const exceptionOnly = query.summary === "exception"

    const page = await apiGet<PageView<BackendSalesOrderView>>(
        "/admin/sales-orders",
        {
            page: query.page,
            page_size: query.pageSize,
            order_no: query.search?.trim() || undefined,
            business_type: businessType,
            commercial_status:
                myTodo || exceptionOnly
                    ? undefined
                    : statusMap.commercial_status,
            review_status:
                myTodo || exceptionOnly ? undefined : statusMap.review_status,
            created_by: createdBy,
            my_todo: myTodo || undefined,
            exception_only: exceptionOnly || undefined,
            sort_by: mapSortBy(query.sortBy),
            sort_dir: query.sortDir,
        },
    )

    // origin 筛选后端未提供，客户端对当前页做展示过滤（缺口登记，不在本次范围）。
    let items = page.items.map((row) => mapListItemFromBackend(row))
    if (query.origin && query.origin !== "all") {
        items = items.filter((o) => o.originSystem === query.origin)
    }

    return {
        items,
        total: page.total,
        page: page.page,
        pageSize: page.page_size,
        queriedAt: formatIsoNow(),
    }
}

async function loadDetailExtras(
    salesOrderId: string,
    nature: SalesOrderNature,
) {
    const [reviewsPage, confirmationsPage, changeOrdersPage] =
        await Promise.all([
            apiGet<PageView<BackendSalesOrderReview>>(
                "/admin/sales-order-reviews",
                {
                    sales_order_id: salesOrderId,
                    status: "PENDING",
                    page: 1,
                    page_size: 20,
                },
            ).catch(() => ({
                items: [] as BackendSalesOrderReview[],
                total: 0,
                page: 1,
                page_size: 20,
            })),
            apiGet<PageView<BackendProcurementConfirmation>>(
                "/admin/procurement-confirmations",
                {
                    page: 1,
                    page_size: 50,
                },
            ).catch(() => ({
                items: [] as BackendProcurementConfirmation[],
                total: 0,
                page: 1,
                page_size: 50,
            })),
            apiGet<PageView<BackendSalesChangeOrder>>(
                "/admin/sales-change-orders",
                {
                    sales_order_id: salesOrderId,
                    page: 1,
                    page_size: 10,
                },
            ).catch(() => ({
                items: [] as BackendSalesChangeOrder[],
                total: 0,
                page: 1,
                page_size: 10,
            })),
        ])

    const pendingReview =
        reviewsPage.items.find(
            (r) =>
                r.sales_order_id === salesOrderId &&
                r.status === "PENDING" &&
                (r.review_stage === "SALES_LEADER" ||
                    r.review_stage === "OPERATIONS"),
        ) ?? null

    const rejected =
        confirmationsPage.items.find(
            (c) => c.sales_order_id === salesOrderId && c.status === "REJECTED",
        ) ?? null

    const activeChange =
        changeOrdersPage.items.find(
            (c) =>
                c.sales_order_id === salesOrderId &&
                c.status !== "EFFECTIVE" &&
                c.status !== "VOIDED" &&
                c.status !== "REJECTED",
        ) ?? null

    return {
        activeCardSalesApproval: pendingReview
            ? mapReviewToCardApproval(pendingReview)
            : null,
        procurementRejection: rejected
            ? mapRejectedProcurement(rejected)
            : null,
        activeChangeOrder: activeChange
            ? mapChangeOrder(activeChange, nature)
            : null,
    }
}

async function loadCustomerDisplay(customerId: string): Promise<{
    customerName?: string
    customerContact?: string
}> {
    try {
        const customer = await apiGet<BackendCustomerDetail>(
            `/admin/customers/${customerId}`,
        )
        const contacts = await apiGet<PageView<BackendPartyContact>>(
            `/admin/parties/${customer.party_id}/contacts`,
            {
                status: "active",
                page: 1,
                page_size: 100,
                sort_by: "created_at",
                sort_dir: "desc",
            },
        ).catch(() => null)
        const contact =
            contacts?.items.find((item) => item.is_default) ??
            contacts?.items[0]

        return {
            customerName: customer.legal_name || customer.customer_no,
            customerContact: contact?.contact_name,
        }
    } catch {
        return {}
    }
}

export async function fetchSalesOrderDetail(
    id: string,
): Promise<SalesOrderDetailView | null> {
    let detail: BackendSalesOrderDetail
    try {
        detail = await apiGet<BackendSalesOrderDetail>(
            `/admin/sales-orders/${id}`,
        )
    } catch (err) {
        const apiErr = err as ApiError
        if (apiErr?.status === 404) return null
        throw err
    }

    const customerDisplay = await loadCustomerDisplay(detail.customer_id)
    let contractNumber = ""
    let customerName = customerDisplay.customerName || detail.customer_id
    if (detail.contract_id) {
        try {
            const contract = await apiGet<BackendContractDetail>(
                `/admin/contracts/${detail.contract_id}`,
            )
            contractNumber = contract.contract_no
            const rev =
                contract.revisions.find(
                    (r) => r.id === contract.current_revision_id,
                ) ?? contract.revisions[0]
            if (rev?.customer_name) customerName = rev.customer_name
        } catch {
            // 合同域缺口时保留 id 展示
        }
    }

    const extras = await loadDetailExtras(id, mapNature(detail.business_type))
    const order = mapDetailToListItem(detail, {
        customerName,
        contractNumber,
        ownerName: detail.owner_user_name || "—",
        customerContact: customerDisplay.customerContact,
        ...extras,
    })

    // 最近验收摘要（可选）
    let acceptance: SalesOrderDetailView["acceptance"] = null
    try {
        const accPage = await apiGet<
            PageView<{
                id: string
                acceptance_no: string
                sales_order_id: string
                accepted_at: number
                result: string
                status: string
                version: number
                created_at: number
            }>
        >("/admin/customer-acceptances", {
            sales_order_id: id,
            status: "POSTED",
            page: 1,
            page_size: 1,
            sort_by: "accepted_at",
            sort_dir: "desc",
        })
        const latest = accPage.items[0]
        if (latest) {
            acceptance = {
                acceptedQuantity: "",
                note: latest.result,
                reference: latest.acceptance_no,
                postedAt: formatInstant(latest.accepted_at),
            }
        }
    } catch {
        // 验收域失败不阻塞详情
    }

    const queriedAt = formatIsoNow()
    return {
        ...order,
        acceptance,
        permissionVersion: PERMISSION_VERSION,
        sourceAsOf: queriedAt,
        queriedAt,
    }
}

// ─── 建单 ────────────────────────────────────────────────────────────────────

type DraftContentInput = {
    nature: SalesOrderNature
    ownerUserId: string
    ownerName: string
    welfareScene: string
    paymentTerms: string
    fulfillmentDeadline: string
    taxRatePercent: string
    remark: string
    lineItems: CreateSalesOrderInput["lineItems"]
}

/**
 * 建单与草稿更新共用的表头+明细快照构造（分别对应 `POST /sales-orders` 与
 * `PUT /sales-orders/{id}/working-copy` 的 `draft` 请求体，字段形状完全一致）。
 */
function buildDraftPayload(
    input: DraftContentInput,
    contract: BackendContractDetail,
    requested: BackendContractDetail["revisions"][number],
): {
    businessType: "VOUCHER" | "GOODS_SERVICE"
    draft: Record<string, unknown>
} {
    const taxRate = percentToRate(input.taxRatePercent || "0")
    const businessType =
        input.nature === "card_voucher" ? "VOUCHER" : "GOODS_SERVICE"

    const lines = input.lineItems.map((line, index) => {
        const base = {
            line_no: index + 1,
            line_type: businessType,
            sales_tax_rate: taxRate,
            item_name_snapshot: line.name.trim(),
            spec_snapshot: line.sku.trim() || null,
            unit_snapshot: line.unit.trim() || null,
            goods: null as null | Record<string, unknown>,
            voucher: null as null | Record<string, unknown>,
        }

        if (input.nature === "card_voucher") {
            const cardCount = Math.max(
                1,
                Math.floor(Number(line.quantity) || 1),
            )
            const unitPrice = line.unitPriceGross || "0.0000"
            const face = line.faceValue || "0.00"
            // 金额由后端按约定舍入；此处按字符串原样提交并给后端校验三元组
            const faceTotal = (Number(face) * cardCount).toFixed(2)
            const txn = (Number(unitPrice) * cardCount).toFixed(2)
            const gift = (Number(faceTotal) - Number(txn)).toFixed(2)
            base.voucher = {
                face_value: face,
                card_count: cardCount,
                unit_price_gross: unitPrice,
                face_value_total: faceTotal,
                transaction_amount: txn,
                gift_amount: gift,
                // 配赠率由服务端按 gift_amount / transaction_amount 推导，前端不手输
                gift_rate: null,
                card_form: mapCardForm(line.cardForm || "电子卡"),
            }
        } else {
            // 公司商品池同时返回稳定 SKU 与精确当前修订，销售草稿锁定这一对身份。
            const skuId = line.sku.trim()
            const skuRevisionId = line.skuRevisionId.trim()
            if (!skuId || !skuRevisionId) {
                throwValidation("实物明细须从公司商品池选择有效 SKU")
            }
            base.goods = {
                sku_id: skuId,
                sku_revision_id: skuRevisionId,
                welfare_scenario: mapWelfareScenarioCode(input.welfareScene),
                fulfillment_mode: mapFulfillmentMode(
                    line.fulfillmentMode || "公司仓发",
                ),
                fulfillment_due_at: dateToUnixSecs(line.dueDate),
                quantity: line.quantity || "0",
                base_unit_code: line.unit.trim() || "EA",
                unit_price_gross: line.unitPriceGross || "0.0000",
            }
        }
        return base
    })

    return {
        businessType,
        draft: {
            editor_user_id:
                input.ownerUserId.trim() || input.ownerName.trim() || "unknown",
            customer_name: requested.customer_name,
            contract_no: contract.contract_no,
            settlement_party_name: requested.settlement_party_name,
            payment_term_code: requested.payment_term_code || "CUSTOM",
            payment_term_name:
                input.paymentTerms.trim() ||
                requested.payment_term_name ||
                "合同约定",
            invoice_type: requested.invoice_type || "SPECIAL",
            tax_point: requested.tax_point || input.taxRatePercent || "0",
            // 表头项目名称存中文快照，便于列表/纸质件直接展示
            project_name: welfareScenarioLabel(input.welfareScene) || null,
            business_remark: input.remark.trim() || null,
            voucher_category_sku_id:
                input.nature === "card_voucher"
                    ? input.lineItems[0]?.sku.trim() || null
                    : null,
            voucher_expiry_at:
                input.nature === "card_voucher"
                    ? dateToUnixSecs(input.fulfillmentDeadline)
                    : null,
            lines,
        },
    }
}

async function resolveContractRevision(input: {
    contractId: string
    requestedContractRevisionId?: string
}): Promise<{
    contract: BackendContractDetail
    requested: BackendContractDetail["revisions"][number]
}> {
    const contract = await apiGet<BackendContractDetail>(
        `/admin/contracts/${input.contractId}`,
    )
    const requested =
        contract.revisions.find(
            (r) => r.id === input.requestedContractRevisionId,
        ) ??
        contract.revisions.find((r) => r.id === contract.current_revision_id) ??
        contract.revisions[0]

    if (!requested) {
        throwValidation("合同修订不存在")
    }
    return { contract, requested }
}

export async function createSalesOrder(
    input: CreateSalesOrderInput,
): Promise<CreateSalesOrderResult> {
    if (input.lineItems.length === 0) {
        throwValidation("至少需要一行明细")
    }
    if (input.nature === "card_voucher" && input.lineItems.length !== 1) {
        throwValidation("卡券销售单必须且只能有一行明细")
    }

    const { contract, requested } = await resolveContractRevision(
        input.contract,
    )
    const { businessType, draft } = buildDraftPayload(
        input,
        contract,
        requested,
    )

    const orderNo = localOrderNo()
    const body = {
        order_no: orderNo,
        business_type: businessType,
        customer_id: contract.customer_id,
        contract_id: contract.id,
        settlement_party_id: contract.settlement_party_id,
        idempotency_key: input.idempotencyKey,
        intent: input.intent,
        draft,
    }

    const created = await apiPost<BackendSalesOrderDetail>(
        "/admin/sales-orders",
        body,
    )

    return {
        salesOrderId: created.id,
        documentNumber: created.order_no,
        statusLabel: created.stage.label,
        createdAt: new Date(created.created_at * 1000).toISOString(),
        reference: `SO-CREATE-${created.order_no}`,
        workingCopyVersion: created.working_copy?.version,
    }
}

/**
 * 更新已存在草稿的工作副本（继续编辑场景；乐观锁按工作副本自身版本比对，
 * 不是销售单版本）。字段形状与 `createSalesOrder` 的 `draft` 完全一致。
 */
export async function saveSalesOrderDraft(
    input: DraftContentInput & {
        salesOrderId: string
        /** 工作副本乐观锁版本，来自上一次 `fetchSalesOrderDetail`/本函数返回值。 */
        version: number
        contract: SalesOrderContractInput
    },
): Promise<{ version: number }> {
    if (input.lineItems.length === 0) {
        throwValidation("至少需要一行明细")
    }
    if (input.nature === "card_voucher" && input.lineItems.length !== 1) {
        throwValidation("卡券销售单必须且只能有一行明细")
    }

    const { contract, requested } = await resolveContractRevision(
        input.contract,
    )
    const { draft } = buildDraftPayload(input, contract, requested)

    const updated = await apiPut<BackendWorkingCopy>(
        `/admin/sales-orders/${input.salesOrderId}/working-copy`,
        { version: input.version, draft },
    )

    return { version: updated.version }
}

/** 提交已存在的草稿进入审核轨（继续编辑场景的"提交"动作）。 */
export async function submitSalesOrder(input: {
    salesOrderId: string
    version: number
    idempotencyKey: string
}): Promise<{ salesOrderId: string }> {
    await apiPost(`/admin/sales-orders/${input.salesOrderId}/submit`, {
        version: input.version,
        idempotency_key: input.idempotencyKey,
    })
    return { salesOrderId: input.salesOrderId }
}

export type SalesOrderDraftResumeData = {
    salesOrderId: string
    documentNumber: string
    version: number
    contractId: string
    nature: SalesOrderNature
    welfareScene: string
    paymentTerms: string
    fulfillmentDeadline: string
    taxRatePercent: string
    remark: string
    lineItems: SalesOrderDraftLineInput[]
}

/**
 * 取回草稿的可编辑表单值（继续编辑场景）；非草稿或没有有效工作副本时返回
 * `null`（详情页只在草稿态显示"继续编辑"，理论上不会命中，仍做防御）。
 *
 * 直接读原始详情接口而不是 `fetchSalesOrderDetail`——后者已经把明细压缩成
 * 展示用的 `SalesOrderLineItem`（丢了 `skuRevisionId`/`fulfillmentMode` 等
 * 建单表单必需的原始字段），这里需要 `working_copy` 的完整快照。
 */
export async function fetchSalesOrderDraftForResume(
    salesOrderId: string,
): Promise<SalesOrderDraftResumeData | null> {
    const detail = await apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${salesOrderId}`,
    )
    const wc = detail.working_copy
    if (detail.commercial_status !== "DRAFT" || !wc) return null

    const nature: SalesOrderNature =
        detail.business_type === "VOUCHER" ? "card_voucher" : "physical_service"

    const welfareScene =
        WELFARE_SCENARIO_OPTIONS.find((o) => o.label === wc.project_name)
            ?.value ?? ""
    const paymentTerms =
        PAYMENT_TERM_OPTIONS.find((o) => o.label === wc.payment_term_name)
            ?.value ?? "CONTRACT"

    const lineItems: SalesOrderDraftLineInput[] = (wc.lines ?? []).map(
        (line) => {
            const isVoucher = line.line_type === "VOUCHER"
            return {
                rowKey: line.sales_order_line_id || line.id,
                name: line.item_name_snapshot,
                // 卡券类目 SKU 存在表头快照 voucher_category_sku_id，不在行内 sku_id；
                // 卡券也不追踪精确修订（只有实物/服务需要锁定 SKU 修订）。
                sku: isVoucher
                    ? (wc.voucher_category_sku_id ?? "")
                    : (line.sku_id ?? ""),
                skuRevisionId: isVoucher ? "" : (line.sku_revision_id ?? ""),
                quantity: isVoucher
                    ? String(line.card_count ?? 1)
                    : (line.quantity ?? "1"),
                unit: isVoucher
                    ? "张"
                    : (line.unit_snapshot ?? line.base_unit_code ?? ""),
                unitPriceGross: line.unit_price_gross ?? "0.00",
                // 建单页不提供仓发/直发选择；沿用 createEmptyLine 的固定占位值。
                fulfillmentMode: !isVoucher ? "公司仓发" : "",
                dueDate: formatEpochDate(line.fulfillment_due_at),
                faceValue: line.face_value ?? "",
                giftRate: "",
                cardForm: isVoucher
                    ? mapCardFormFromBackend(line.card_form)
                    : "",
            }
        },
    )

    return {
        salesOrderId: detail.id,
        documentNumber: detail.order_no,
        version: wc.version,
        contractId: detail.contract_id ?? "",
        nature,
        welfareScene,
        paymentTerms,
        fulfillmentDeadline: formatEpochDate(wc.voucher_expiry_at),
        taxRatePercent: rateToPercent(wc.lines?.[0]?.sales_tax_rate),
        remark: wc.business_remark ?? "",
        lineItems,
    }
}

// ─── 采购拒绝处理 ────────────────────────────────────────────────────────────

export async function adjustProcurementRejectionDraft(input: {
    salesOrderId: string
    unitPriceGross: string
    note: string
}): Promise<{ ok: true }> {
    const detail = await apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${input.salesOrderId}`,
    )
    const wc = detail.working_copy
    if (!wc) {
        throwValidation("当前销售单无可用草稿，无法改价")
    }

    const lines = (wc.lines ?? []).map((line, index) => {
        const isVoucher = line.line_type === "VOUCHER"
        const unitPrice =
            index === 0
                ? input.unitPriceGross
                : (line.unit_price_gross ?? "0.0000")
        const base: Record<string, unknown> = {
            line_no: line.line_no,
            line_type: line.line_type,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot,
            spec_snapshot: line.spec_snapshot ?? null,
            unit_snapshot: line.unit_snapshot ?? null,
            goods: null,
            voucher: null,
        }
        if (isVoucher) {
            const cardCount = line.card_count ?? 1
            const face = line.face_value ?? "0.00"
            const faceTotal = (Number(face) * cardCount).toFixed(2)
            const txn = (Number(unitPrice) * cardCount).toFixed(2)
            const gift = (Number(faceTotal) - Number(txn)).toFixed(2)
            base.voucher = {
                face_value: face,
                card_count: cardCount,
                unit_price_gross: unitPrice,
                face_value_total: faceTotal,
                transaction_amount: txn,
                gift_amount: gift,
                gift_rate: null,
                card_form: line.card_form ?? "ELECTRONIC",
            }
        } else {
            const skuId = line.sku_id?.trim()
            const skuRevisionId = line.sku_revision_id?.trim()
            if (!skuId || !skuRevisionId) {
                throwValidation(
                    "历史草稿缺少精确 SKU 修订，请重新从公司商品池选择商品",
                )
            }
            base.goods = {
                sku_id: skuId,
                sku_revision_id: skuRevisionId,
                welfare_scenario: null,
                fulfillment_mode: "COMPANY_WAREHOUSE",
                fulfillment_due_at: Math.floor(Date.now() / 1000),
                quantity: line.quantity ?? "0",
                base_unit_code:
                    line.base_unit_code ?? line.unit_snapshot ?? "EA",
                unit_price_gross: unitPrice,
            }
        }
        return base
    })

    await apiPut(`/admin/sales-orders/${input.salesOrderId}/working-copy`, {
        version: detail.version,
        draft: {
            editor_user_id: wc.editor_user_id,
            customer_name: "", // 后端 Save 会用草稿覆盖；名称由服务端实体保留时可能校验
            contract_no: null,
            settlement_party_name: null,
            payment_term_code: "CUSTOM",
            payment_term_name: "合同约定",
            invoice_type: "SPECIAL",
            tax_point: "0",
            project_name: null,
            business_remark: input.note || null,
            voucher_category_sku_id: null,
            voucher_expiry_at: null,
            lines,
        },
    })

    return { ok: true }
}

export async function resolveProcurementRejection(input: {
    salesOrderId: string
    action: "RESUBMIT_CHANGED_TERMS" | "VOID_AFTER_REJECTION"
    idempotencyKey: string
    voidReason?: string
}): Promise<ProcurementResolutionOutcome> {
    const detail = await apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${input.salesOrderId}`,
    )

    if (input.action === "VOID_AFTER_REJECTION") {
        await apiPost(`/admin/sales-orders/${input.salesOrderId}/void`, {
            version: detail.version,
        })
        return {
            outcome: "VOIDED_AFTER_PROCUREMENT_REJECTION",
            reference: `PR-VOID-${input.idempotencyKey.slice(0, 8).toUpperCase()}`,
            detail: `销售单已作废并保留驳回历史。原因：${input.voidReason ?? "不做"}`,
            reviewStatus: "VOIDED",
            primaryStatusLabel: "已作废",
        }
    }

    const submission = await apiPost<{
        id: string
        submission_no: number
    }>(`/admin/sales-orders/${input.salesOrderId}/submit`, {
        version: detail.version,
        idempotency_key: input.idempotencyKey,
    })
    return {
        outcome: "CHANGED_TERMS_RESUBMITTED",
        reference: `PR-RESUB-${input.idempotencyKey.slice(0, 8).toUpperCase()}`,
        detail: "已提交新版本进入采购二次确认；旧驳回记录保持历史。",
        newSubmissionNo: submission.submission_no,
        newSubjectHash: submission.id,
        reviewStatus: "RESOLVED",
        primaryStatusLabel: "待二次确认",
    }
}

// ─── 销售变更单 ──────────────────────────────────────────────────────────────

export async function startSalesChangeOrder(input: {
    salesOrderId: string
    baseRevisionNo: number
    nature: "physical_service" | "card_voucher"
}): Promise<SalesChangeOrderSummary> {
    const detail = await apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${input.salesOrderId}`,
    )
    const wc = detail.working_copy
    const latestRev = detail.revisions?.[0]

    // 变更单创建需要完整 draft；以当前工作副本/最新版本金额行作为目标草稿骨架。
    // 字段不足时后端会校验失败并经 ApiError 抛出。
    let contractNo: string | null = null
    let customerName = detail.customer_id
    let settlementName: string | null = null
    let paymentCode = "CUSTOM"
    let paymentName = "合同约定"
    let invoiceType = "SPECIAL"
    let taxPoint = "0"

    if (detail.contract_id) {
        try {
            const contract = await apiGet<BackendContractDetail>(
                `/admin/contracts/${detail.contract_id}`,
            )
            contractNo = contract.contract_no
            const rev =
                contract.revisions.find(
                    (r) => r.id === contract.current_revision_id,
                ) ?? contract.revisions[0]
            if (rev) {
                customerName = rev.customer_name
                settlementName = rev.settlement_party_name
                paymentCode = rev.payment_term_code
                paymentName = rev.payment_term_name
                invoiceType = rev.invoice_type
                taxPoint = rev.tax_point
            }
        } catch {
            // ignore
        }
    }

    const lines =
        wc?.lines?.map((line) => {
            const isVoucher = line.line_type === "VOUCHER"
            const row: Record<string, unknown> = {
                line_no: line.line_no,
                line_type: line.line_type,
                sales_tax_rate: line.sales_tax_rate,
                item_name_snapshot: line.item_name_snapshot,
                spec_snapshot: line.spec_snapshot ?? null,
                unit_snapshot: line.unit_snapshot ?? null,
                goods: null,
                voucher: null,
            }
            if (isVoucher) {
                const cardCount = line.card_count ?? 1
                const face = line.face_value ?? "0.00"
                const unitPrice = line.unit_price_gross ?? "0.0000"
                const faceTotal = (Number(face) * cardCount).toFixed(2)
                const txn = (Number(unitPrice) * cardCount).toFixed(2)
                const gift = (Number(faceTotal) - Number(txn)).toFixed(2)
                row.voucher = {
                    face_value: face,
                    card_count: cardCount,
                    unit_price_gross: unitPrice,
                    face_value_total: faceTotal,
                    transaction_amount: txn,
                    gift_amount: gift,
                    gift_rate: null,
                    card_form: line.card_form ?? "ELECTRONIC",
                }
            } else {
                const skuId = line.sku_id?.trim()
                const skuRevisionId = line.sku_revision_id?.trim()
                if (!skuId || !skuRevisionId) {
                    throwValidation(
                        "历史草稿缺少精确 SKU 修订，请重新从公司商品池选择商品",
                    )
                }
                row.goods = {
                    sku_id: skuId,
                    sku_revision_id: skuRevisionId,
                    welfare_scenario: null,
                    fulfillment_mode: "COMPANY_WAREHOUSE",
                    fulfillment_due_at: Math.floor(Date.now() / 1000),
                    quantity: line.quantity ?? "0",
                    base_unit_code: line.base_unit_code ?? "EA",
                    unit_price_gross: line.unit_price_gross ?? "0.0000",
                }
            }
            return row
        }) ?? []

    if (lines.length === 0) {
        throwValidation("无法发起变更：缺少可变更的明细草稿")
    }

    const created = await apiPost<BackendSalesChangeOrder>(
        "/admin/sales-change-orders",
        {
            sales_order_id: input.salesOrderId,
            change_type: input.nature === "card_voucher" ? "OTHER" : "AMOUNT",
            reason: "销售发起变更",
            idempotency_key: `sco-${input.salesOrderId}-${Date.now()}`,
            draft: {
                editor_user_id: wc?.editor_user_id ?? "unknown",
                customer_name: customerName,
                contract_no: contractNo,
                settlement_party_name: settlementName,
                payment_term_code: paymentCode,
                payment_term_name: paymentName,
                invoice_type: invoiceType,
                tax_point: taxPoint,
                project_name: null,
                business_remark: null,
                voucher_category_sku_id: null,
                voucher_expiry_at: null,
                lines,
            },
        },
    )

    return {
        ...mapChangeOrder(created, input.nature),
        baseRevisionNo: input.baseRevisionNo || latestRev?.revision_no || 0,
    }
}
