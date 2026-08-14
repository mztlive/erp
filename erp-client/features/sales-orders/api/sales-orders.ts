/**
 * W05 销售单 HTTP API（queryFn / mutationFn 纯函数）。
 *
 * 后端域：sales_order / sales_review / work_item / bulk_job。
 * 在本文件内将后端 DTO 适配为既有前端视图契约（types.ts / hooks/queries.ts 不变）。
 * 失败统一抛 ApiError（@/lib/api），禁止 throw new Error("string")。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import { downloadFileAsset } from "@/features/file-assets/api"
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
    type BackendLowMarginManagerDecisionResult,
    type BackendProcurementRejectionResolutionResult,
    type BackendContractDetail,
    type BackendCustomerDetail,
    type BackendPartyContact,
    type BackendSalesChangeOrder,
    type BackendSalesOrderDetail,
    type BackendSalesOrderView,
    type BackendWorkingCopy,
    type BackendWorkingCopyLine,
    type PageView,
    type LowMarginManagerDecisionOutcome,
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
    mapCardForm,
    mapCardFormFromBackend,
    mapChangeOrder,
    mapDetailToListItem,
    mapFulfillmentMode,
    mapListItemFromBackend,
    mapCloseFilterToBackend,
    mapCollectionFilterToBackend,
    mapCommercialStatusFilterToBackend,
    mapFulfillmentFilterToBackend,
    mapInvoiceFilterToBackend,
    mapNature,
    mapReviewStatusFilterToBackend,
    mapSortBy,
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
    cancelCardSalesApproval,
    submitCardSalesApprovalDecision,
} from "@/features/sales-orders/api/card-approval"
export { createSalesOrderExportJob } from "@/features/sales-orders/api/export"

// ─── 列表 / 详情 ─────────────────────────────────────────────────────────────

export async function fetchSalesOrders(
    query: SalesOrdersListQuery,
): Promise<SalesOrderListView> {
    const businessType =
        query.nature === "card_voucher"
            ? "VOUCHER"
            : query.nature === "physical_service"
              ? "GOODS_SERVICE"
              : undefined

    // "待我处理"/"我创建的" 都按创建人过滤；"待我处理"额外限定草稿/驳回回销售，
    // "异常"限定审核轨被驳回。前两类视图与精细商业/审核状态互斥，避免形成
    // 同字段冲突条件；其余结构化筛选可继续与固定视图 AND 组合。
    const createdBy =
        query.createdBy?.trim() ||
        (query.summary === "mine" || query.summary === "createdByMe"
            ? query.currentUserId?.trim() || undefined
            : undefined)
    const myTodo = query.summary === "mine"
    const exceptionOnly = query.summary === "exception"

    const page = await apiGet<PageView<BackendSalesOrderView>>(
        "/admin/sales-orders",
        {
            page: query.page,
            page_size: query.pageSize,
            order_no: query.search?.trim() || undefined,
            customer_id: query.customerId,
            contract_id: query.contractId,
            business_type: businessType,
            origin_system:
                query.origin === "erp"
                    ? "ERP"
                    : query.origin === "mall"
                      ? "MALL"
                      : undefined,
            commercial_status:
                myTodo || exceptionOnly
                    ? undefined
                    : mapCommercialStatusFilterToBackend(
                          query.commercialStatus,
                      ),
            review_status:
                myTodo || exceptionOnly
                    ? undefined
                    : mapReviewStatusFilterToBackend(query.reviewStatus),
            fulfillment_progress: mapFulfillmentFilterToBackend(
                query.fulfillment,
            ),
            collection_progress: mapCollectionFilterToBackend(query.collection),
            invoice_progress: mapInvoiceFilterToBackend(query.invoice),
            close_status: mapCloseFilterToBackend(query.closeStatus),
            created_from: query.createdFrom,
            created_to: query.createdTo,
            created_by: createdBy,
            my_todo: myTodo || undefined,
            exception_only: exceptionOnly || undefined,
            sort_by: mapSortBy(query.sortBy),
            sort_dir: query.sortDir,
        },
    )

    const contractDisplays = await loadContractDisplays(
        page.items
            .map((row) => row.contract_id)
            .filter((id): id is string => Boolean(id)),
    )
    const items = page.items.map((row) => {
        const display = row.contract_id
            ? contractDisplays.get(row.contract_id)
            : undefined
        return mapListItemFromBackend(row, {
            contractNumber: display?.contractNumber,
            contractCompanyName: display?.companyName,
            customerName: display?.companyName || undefined,
        })
    })

    return {
        items,
        total: page.total,
        page: page.page,
        pageSize: page.page_size,
        queriedAt: formatIsoNow(),
    }
}

type ContractDisplay = {
    contractNumber: string
    companyName: string
}

/**
 * 按合同 ID 批量补齐编号与公司名称；当前页去重后分批拉取，单份失败不拖垮整表。
 */
async function loadContractDisplays(
    contractIds: string[],
): Promise<Map<string, ContractDisplay>> {
    const unique = [
        ...new Set(contractIds.map((id) => id.trim()).filter(Boolean)),
    ]
    const displays = new Map<string, ContractDisplay>()
    const chunkSize = 8
    for (let index = 0; index < unique.length; index += chunkSize) {
        const chunk = unique.slice(index, index + chunkSize)
        const loaded = await Promise.all(
            chunk.map(async (contractId) => {
                try {
                    const contract = await apiGet<BackendContractDetail>(
                        `/admin/contracts/${contractId}`,
                    )
                    const revision = contract.revisions.find(
                        (item) => item.id === contract.current_revision_id,
                    )
                    return [
                        contractId,
                        {
                            contractNumber: contract.contract_no,
                            companyName: revision?.customer_name ?? "",
                        },
                    ] as const
                } catch {
                    return [
                        contractId,
                        { contractNumber: "", companyName: "" },
                    ] as const
                }
            }),
        )
        for (const [contractId, display] of loaded) {
            displays.set(contractId, display)
        }
    }
    return displays
}

/**
 * 下载销售单关联合同的当前修订 PDF。
 *
 * @param contractId 合同稳定身份
 */
export async function downloadSalesOrderContractPdf(
    contractId: string,
): Promise<void> {
    const id = contractId.trim()
    if (!id) {
        throwValidation("该销售单没有关联合同")
    }
    const contract = await apiGet<BackendContractDetail>(
        `/admin/contracts/${id}`,
    )
    const revision = contract.revisions.find(
        (item) => item.id === contract.current_revision_id,
    )
    const fileId = revision?.contract_pdf_file_id?.trim()
    if (!fileId) {
        throwValidation("合同尚未归档 PDF，无法下载")
    }
    await downloadFileAsset(fileId, `${contract.contract_no}.pdf`)
}

/**
 * 详情页附属信息：卡券审批与在途改单。
 *
 * 采购驳回摘要由销售单详情字段 `open_procurement_rejection` 权威下发，
 * 不再侧查采购确认列表（销售角色通常无 `procurement_confirmation:list`，
 * 且旧实现仅取全局第 1 页 50 条，会静默丢入口）。
 */
async function loadDetailExtras(
    salesOrderId: string,
    nature: SalesOrderNature,
) {
    const changeOrdersPage = await apiGet<PageView<BackendSalesChangeOrder>>(
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
    }))

    const activeChange =
        changeOrdersPage.items.find(
            (c) =>
                c.sales_order_id === salesOrderId &&
                c.status !== "EFFECTIVE" &&
                c.status !== "VOIDED" &&
                c.status !== "REJECTED",
        ) ?? null

    return {
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
            const rev = contract.revisions.find(
                (r) => r.id === contract.current_revision_id,
            )
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
    targetMallId: string
    receivableDueDate: string
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
            requested_contract_revision_id: requested.id,
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
            target_mall_id:
                input.nature === "card_voucher"
                    ? input.targetMallId.trim() || null
                    : null,
            receivable_due_date:
                input.nature === "card_voucher"
                    ? input.receivableDueDate.trim() || null
                    : null,
            lines,
        },
    }
}

async function resolveContractRevision(input: {
    contractId: string
    requestedContractRevisionId: string
}): Promise<{
    contract: BackendContractDetail
    requested: BackendContractDetail["revisions"][number]
}> {
    const contract = await apiGet<BackendContractDetail>(
        `/admin/contracts/${input.contractId}`,
    )
    const requested = contract.revisions.find(
        (r) => r.id === input.requestedContractRevisionId,
    )

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

    const body = {
        order_no: input.orderNo,
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
export type SubmitSalesOrderInput = {
    salesOrderId: string
    version: number
    idempotencyKey: string
}

export async function submitSalesOrder(
    input: SubmitSalesOrderInput,
): Promise<{ salesOrderId: string }> {
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
    targetMallId: string
    receivableDueDate: string
    taxRatePercent: string
    remark: string
    lineItems: SalesOrderDraftLineInput[]
}

function mapDraftLines(
    lines: BackendWorkingCopyLine[],
    voucherCategorySkuId?: string | null,
): SalesOrderDraftLineInput[] {
    return lines.map((line) => {
        const isVoucher = line.line_type === "VOUCHER"
        return {
            rowKey: line.sales_order_line_id || line.id,
            name: line.item_name_snapshot,
            sku: isVoucher ? (voucherCategorySkuId ?? "") : (line.sku_id ?? ""),
            skuRevisionId: isVoucher ? "" : (line.sku_revision_id ?? ""),
            quantity: isVoucher
                ? String(line.card_count ?? 1)
                : (line.quantity ?? "1"),
            unit: isVoucher
                ? "张"
                : (line.unit_snapshot ?? line.base_unit_code ?? ""),
            unitPriceGross: line.unit_price_gross ?? "0.00",
            fulfillmentMode: !isVoucher ? "公司仓发" : "",
            dueDate: formatEpochDate(line.fulfillment_due_at),
            faceValue: line.face_value ?? "",
            giftRate: "",
            cardForm: isVoucher ? mapCardFormFromBackend(line.card_form) : "",
        }
    })
}

function isEditableSalesOrder(detail: BackendSalesOrderDetail) {
    return (
        detail.commercial_status === "DRAFT" ||
        Boolean(detail.open_procurement_rejection) ||
        detail.stage?.code === "awaiting_sales"
    )
}

/**
 * 取回可编辑表单值：草稿继续编辑，或采购驳回后改整单再报。
 * 优先工作副本；没有副本时回退到最近一次提交快照。
 */
export async function fetchSalesOrderDraftForResume(
    salesOrderId: string,
): Promise<SalesOrderDraftResumeData | null> {
    const detail = await apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${salesOrderId}`,
    )
    if (!isEditableSalesOrder(detail)) return null

    const wc = detail.working_copy
    const submissions = [...(detail.submissions ?? [])].sort(
        (a, b) => (b.submission_no ?? 0) - (a.submission_no ?? 0),
    )
    const latestSubmission = submissions[0]
    const source = wc ?? latestSubmission
    if (!source) return null

    const nature: SalesOrderNature =
        detail.business_type === "VOUCHER" ? "card_voucher" : "physical_service"
    const projectName = wc?.project_name ?? latestSubmission?.project_name
    const paymentTermName =
        wc?.payment_term_name ?? latestSubmission?.payment_term_name
    const voucherCategorySkuId =
        wc?.voucher_category_sku_id ?? latestSubmission?.voucher_category_sku_id
    const voucherExpiry =
        wc?.voucher_expiry_at ?? latestSubmission?.voucher_expiry_at
    const remark = wc?.business_remark ?? latestSubmission?.business_remark
    const lines = source.lines ?? []

    const welfareScene =
        WELFARE_SCENARIO_OPTIONS.find((o) => o.label === projectName)?.value ??
        ""
    const paymentTerms =
        PAYMENT_TERM_OPTIONS.find((o) => o.label === paymentTermName)?.value ??
        "CONTRACT"

    return {
        salesOrderId: detail.id,
        documentNumber: detail.order_no,
        version: wc?.version ?? detail.version,
        contractId: detail.contract_id ?? "",
        nature,
        welfareScene,
        paymentTerms,
        fulfillmentDeadline: formatEpochDate(voucherExpiry),
        targetMallId: source.target_mall_id ?? "",
        receivableDueDate: source.receivable_due_date ?? "",
        taxRatePercent: rateToPercent(lines[0]?.sales_tax_rate),
        remark: remark ?? "",
        lineItems: mapDraftLines(lines, voucherCategorySkuId),
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

export type ResolveProcurementRejectionIntent = {
    salesOrderId: string
} & (
    | {
          action: "RESUBMIT_CHANGED_TERMS"
          customerReconfirmationEvidenceIds: string[]
      }
    | {
          action: "REQUEST_LOW_MARGIN_ACCEPTANCE"
          lowMarginAcceptanceReason: string
          evidenceReferenceIds: string[]
      }
    | {
          action: "VOID_AFTER_REJECTION"
          voidReasonCode: string
          comment: string
      }
)

export type ResolveProcurementRejectionPayload =
    ResolveProcurementRejectionIntent & {
        rejectedProcurementConfirmationId: string
        rejectedSubmissionId: string
        expectedSalesOrderLockVersion: number
        expectedDraftVersion?: number
    }

export type ResolveProcurementRejectionInput =
    ResolveProcurementRejectionPayload & {
        idempotencyKey: string
    }

/**
 * 冻结采购驳回处置所需的对象身份与版本。调用方必须把返回值和命令键一起保存；
 * 结果未知后的重试不得重新读取并拼装另一份命令。
 */
export async function prepareProcurementRejectionResolution(
    intent: ResolveProcurementRejectionIntent,
): Promise<ResolveProcurementRejectionPayload> {
    const detail = await apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${intent.salesOrderId}`,
    )
    const rejection = detail.open_procurement_rejection
    if (!rejection) {
        throwValidation("当前销售单没有可处理的采购驳回")
    }
    if (intent.action !== "VOID_AFTER_REJECTION" && !detail.working_copy) {
        throwValidation("当前销售单没有可处理的工作副本")
    }
    return {
        ...intent,
        rejectedProcurementConfirmationId:
            rejection.procurement_confirmation_id,
        rejectedSubmissionId: rejection.submission_id,
        expectedSalesOrderLockVersion: detail.version,
        expectedDraftVersion: detail.working_copy?.version,
    }
}

/**
 * 处置采购驳回的唯一入口。对象身份和版本必须来自同一次 prepare 并被冻结；
 * 客户证据只能由调用方显式提供，禁止补造引用或结果编号。
 */
export async function resolveProcurementRejection(
    input: ResolveProcurementRejectionInput,
): Promise<ProcurementResolutionOutcome> {
    const common = {
        action: input.action,
        sales_order_id: input.salesOrderId,
        rejected_procurement_confirmation_id:
            input.rejectedProcurementConfirmationId,
        rejected_submission_id: input.rejectedSubmissionId,
        expected_sales_order_lock_version: input.expectedSalesOrderLockVersion,
        operation_id: input.idempotencyKey,
        idempotency_key: input.idempotencyKey,
    }
    let command: Record<string, unknown>
    if (input.action === "RESUBMIT_CHANGED_TERMS") {
        if (input.customerReconfirmationEvidenceIds.length === 0) {
            throwValidation("改品或改价重提必须登记客户重新确认依据")
        }
        command = {
            ...common,
            expected_draft_version: input.expectedDraftVersion,
            customer_reconfirmation_evidence_ids:
                input.customerReconfirmationEvidenceIds,
        }
    } else if (input.action === "REQUEST_LOW_MARGIN_ACCEPTANCE") {
        if (!input.lowMarginAcceptanceReason.trim()) {
            throwValidation("请填写低毛利承接理由")
        }
        if (input.evidenceReferenceIds.length === 0) {
            throwValidation("申请低毛利承接必须登记证据依据")
        }
        command = {
            ...common,
            expected_draft_version: input.expectedDraftVersion,
            low_margin_acceptance_reason:
                input.lowMarginAcceptanceReason.trim(),
            evidence_reference_ids: input.evidenceReferenceIds,
        }
    } else {
        if (!input.voidReasonCode.trim() || !input.comment.trim()) {
            throwValidation("作废原因代码和说明不能为空")
        }
        command = {
            ...common,
            void_reason_code: input.voidReasonCode.trim(),
            comment: input.comment.trim(),
        }
    }

    const result = await apiPost<BackendProcurementRejectionResolutionResult>(
        `/admin/sales-orders/${input.salesOrderId}/procurement-rejection-resolution`,
        command,
    )
    if (result.outcome === "CHANGED_TERMS_RESUBMITTED") {
        return {
            outcome: result.outcome,
            reference: result.new_procurement_confirmation_id,
            detail: "已冻结新提交并创建新的采购确认待办；旧驳回记录保持历史。",
            newSubmissionNo: result.new_submission_no,
            newSubjectHash: result.new_submission_id,
            newWorkItemId: result.new_procurement_work_item_id,
            reviewStatus: "RESOLVED",
            primaryStatusLabel: "待二次确认",
        }
    }
    if (result.outcome === "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED") {
        return {
            outcome: result.outcome,
            reference: result.low_margin_confirmation_id,
            detail: "已冻结原商业条件并转交销售上级确认低毛利承接。",
            newSubmissionNo: result.new_submission_no,
            newSubjectHash: result.new_submission_id,
            newWorkItemId: result.low_margin_manager_work_item_id,
            reviewStatus: "RESOLVED",
            primaryStatusLabel: "待销售上级确认",
        }
    }
    return {
        outcome: result.outcome,
        reference: result.workflow_action_id,
        detail: "销售单已作废，采购驳回与历史提交记录已保留。",
        reviewStatus: "VOIDED",
        primaryStatusLabel: "已作废",
    }
}

type CompleteLowMarginManagerConfirmationInput = {
    salesOrderId: string
    workItemId: string
    taskVersion: string
    subjectVersion: string
    lowMarginSubmissionId: string
    rejectedProcurementConfirmationId: string
    expectedSalesOrderLockVersion: number
    idempotencyKey: string
} & (
    | { decision: "APPROVE"; comment?: string }
    | {
          decision: "REJECT"
          reasonCode: string
          comment: string
      }
)

/** 提交低毛利上级确认的唯一强类型决定。 */
export async function completeLowMarginManagerConfirmation(
    input: CompleteLowMarginManagerConfirmationInput,
): Promise<LowMarginManagerDecisionOutcome> {
    const decision =
        input.decision === "APPROVE"
            ? {
                  decision: "APPROVE",
                  work_item_type: "LOW_MARGIN_MANAGER_CONFIRMATION",
                  sales_order_id: input.salesOrderId,
                  rejected_procurement_confirmation_id:
                      input.rejectedProcurementConfirmationId,
                  low_margin_submission_id: input.lowMarginSubmissionId,
                  expected_sales_order_lock_version:
                      input.expectedSalesOrderLockVersion,
                  comment: input.comment?.trim() || null,
              }
            : {
                  decision: "REJECT",
                  work_item_type: "LOW_MARGIN_MANAGER_CONFIRMATION",
                  sales_order_id: input.salesOrderId,
                  rejected_procurement_confirmation_id:
                      input.rejectedProcurementConfirmationId,
                  low_margin_submission_id: input.lowMarginSubmissionId,
                  expected_sales_order_lock_version:
                      input.expectedSalesOrderLockVersion,
                  reason_code: input.reasonCode.trim(),
                  comment: input.comment.trim(),
              }
    const result = await apiPost<BackendLowMarginManagerDecisionResult>(
        "/admin/sales-order-reviews/low-margin-decisions",
        {
            work_item_id: input.workItemId,
            expected_task_version: input.taskVersion,
            expected_subject_version: input.subjectVersion,
            decision,
            idempotency_key: input.idempotencyKey,
        },
    )
    const business = result.business_result
    if (
        business.outcome === "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED"
    ) {
        return {
            outcome: business.outcome,
            salesOrderId: business.sales_order_id,
            lowMarginSubmissionId: business.low_margin_submission_id,
            salesOrderReviewId: business.sales_order_review_id,
            workflowActionId: business.workflow_action_id,
            newProcurementConfirmationId:
                business.new_procurement_confirmation_id,
            newProcurementWorkItemId: business.new_procurement_work_item_id,
        }
    }
    return {
        outcome: business.outcome,
        salesOrderId: business.sales_order_id,
        lowMarginSubmissionId: business.low_margin_submission_id,
        salesOrderReviewId: business.sales_order_review_id,
        workflowActionId: business.workflow_action_id,
    }
}

// ─── 销售变更单 ──────────────────────────────────────────────────────────────

export type StartSalesChangeOrderIntent = {
    salesOrderId: string
    baseRevisionNo: number
    nature: "physical_service" | "card_voucher"
}

export type StartSalesChangeOrderPayload = StartSalesChangeOrderIntent & {
    command: Record<string, unknown>
}

export type StartSalesChangeOrderInput = StartSalesChangeOrderPayload & {
    idempotencyKey: string
}

/** 冻结改单完整载荷；正式请求结果未知后禁止重新从可变详情拼装。 */
export async function prepareStartSalesChangeOrder(
    input: StartSalesChangeOrderIntent,
): Promise<StartSalesChangeOrderPayload> {
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
            const rev = contract.revisions.find(
                (r) => r.id === contract.current_revision_id,
            )
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

    return {
        ...input,
        baseRevisionNo: input.baseRevisionNo || latestRev?.revision_no || 0,
        command: {
            sales_order_id: input.salesOrderId,
            change_type: input.nature === "card_voucher" ? "OTHER" : "AMOUNT",
            reason: "销售发起变更",
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
    }
}

/** 使用已冻结载荷创建改单。 */
export async function startSalesChangeOrder(
    input: StartSalesChangeOrderInput,
): Promise<SalesChangeOrderSummary> {
    const created = await apiPost<BackendSalesChangeOrder>(
        "/admin/sales-change-orders",
        {
            ...input.command,
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        ...mapChangeOrder(created, input.nature),
        baseRevisionNo: input.baseRevisionNo,
    }
}

export type SalesChangeReviewDecisionInput = Readonly<{
    salesChangeOrderId: string
    handlerKey: "sales_change_impact_review" | "sales_change_finance_review"
    decision: "APPROVE" | "REJECT"
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    decisionReason?: string
    idempotencyKey: string
}>

/** 提交销售变更复核强命令；任务处理器与决定共同固定唯一业务端点。 */
export async function submitSalesChangeReviewDecision(
    input: SalesChangeReviewDecisionInput,
): Promise<BackendSalesChangeOrder> {
    const taskVersion = Number(input.expectedTaskVersion)
    if (!Number.isSafeInteger(taskVersion) || taskVersion <= 0) {
        throwValidation("待办版本无效，请刷新任务后重试")
    }
    const action =
        input.handlerKey === "sales_change_impact_review"
            ? input.decision === "APPROVE"
                ? "impact-confirm"
                : "impact-reject"
            : input.decision === "APPROVE"
              ? "finance-confirm"
              : "finance-reject"
    return apiPost<BackendSalesChangeOrder>(
        `/admin/sales-change-orders/${encodeURIComponent(input.salesChangeOrderId)}/${action}`,
        {
            work_item_id: input.workItemId,
            expected_task_version: taskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            decision_reason: input.decisionReason?.trim() || null,
            idempotency_key: input.idempotencyKey,
        },
    )
}
