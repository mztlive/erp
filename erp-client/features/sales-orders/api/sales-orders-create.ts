/**
 * W05 销售单建单 / 草稿读写 / 提交（queryFn / mutationFn 纯函数）。
 *
 * 后端域：sales_order。失败统一抛 ApiError（@/lib/api）。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import {
    PAYMENT_TERM_OPTIONS,
    WELFARE_SCENARIO_OPTIONS,
    welfareScenarioLabel,
} from "@/lib/business-options"
import type {
    BackendSalesOrderDetail,
    BackendWorkingCopy,
    BackendWorkingCopyLine,
} from "@/features/sales-orders/api/contracts"
import {
    dateToUnixSecs,
    formatEpochDate,
    mapCardForm,
    mapCardFormFromBackend,
    mapFulfillmentMode,
    mapFulfillmentModeFromBackend,
    mapWelfareScenarioCode,
    percentToRate,
    rateToPercent,
    throwValidation,
} from "@/features/sales-orders/api/mappers"
import { mapSalesOrderApproval } from "@/features/sales-orders/lib/sales-order-approval"
import { mapVoucherSalesOrderApproval } from "@/features/sales-orders/lib/voucher-sales-order-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type {
    CreateSalesOrderInput,
    CreateSalesOrderResult,
    SalesOrderContractInput,
    SalesOrderDraftLineInput,
    SalesOrderNature,
} from "@/features/sales-orders/types"

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
    requestedContractRevisionId: string,
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
                service_region: line.serviceRegion?.trim() || null,
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
            requested_contract_revision_id: requestedContractRevisionId,
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

export async function createSalesOrder(
    input: CreateSalesOrderInput,
): Promise<CreateSalesOrderResult> {
    if (input.lineItems.length === 0) {
        throwValidation("至少需要一行明细")
    }
    if (input.nature === "card_voucher" && input.lineItems.length !== 1) {
        throwValidation("卡券销售单必须且只能有一行明细")
    }

    const { businessType, draft } = buildDraftPayload(
        input,
        input.contract.requestedContractRevisionId,
    )

    const body = {
        order_no: input.orderNo,
        business_type: businessType,
        contract_id: input.contract.contractId,
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
        approval:
            input.nature === "card_voucher"
                ? mapVoucherSalesOrderApproval(created.approval)
                : mapSalesOrderApproval(created.approval),
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

    const { draft } = buildDraftPayload(
        input,
        input.contract.requestedContractRevisionId,
    )

    const updated = await apiPut<BackendWorkingCopy>(
        `/admin/sales-orders/${input.salesOrderId}/working-copy`,
        {
            version: input.version,
            contract_id: input.contract.contractId,
            draft,
        },
    )

    return { version: updated.version }
}

/** 提交已存在的草稿进入审核轨（继续编辑场景的"提交"动作）。 */
export type SubmitSalesOrderInput = DraftContentInput & {
    salesOrderId: string
    version: number
    idempotencyKey: string
    contract: SalesOrderContractInput
}

export async function submitSalesOrder(
    input: SubmitSalesOrderInput,
): Promise<{ salesOrderId: string }> {
    const { draft } = buildDraftPayload(
        input,
        input.contract.requestedContractRevisionId,
    )
    await apiPost(`/admin/sales-orders/${input.salesOrderId}/submit`, {
        version: input.version,
        idempotency_key: input.idempotencyKey,
        contract_id: input.contract.contractId,
        draft,
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
    /** 续编草稿时带回的只读审批绑定，供创建结果区展示。 */
    approval?: DocumentApprovalView
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
            serviceRegion: isVoucher ? "" : (line.service_region ?? ""),
            quantity: isVoucher
                ? String(line.card_count ?? 1)
                : (line.quantity ?? "1"),
            unit: isVoucher
                ? "张"
                : (line.unit_snapshot ?? line.base_unit_code ?? ""),
            unitPriceGross: line.unit_price_gross ?? "0.00",
            fulfillmentMode: !isVoucher
                ? mapFulfillmentModeFromBackend(line.fulfillment_mode) ||
                  "公司仓发"
                : "",
            dueDate: formatEpochDate(line.fulfillment_due_at),
            faceValue: line.face_value ?? "",
            giftRate: "",
            cardForm: isVoucher ? mapCardFormFromBackend(line.card_form) : "",
        }
    })
}

function isEditableSalesOrder(detail: BackendSalesOrderDetail) {
    return detail.commercial_status === "DRAFT"
}

/**
 * 取回可编辑表单值：草稿继续编辑。
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
        approval:
            nature === "card_voucher"
                ? mapVoucherSalesOrderApproval(detail.approval)
                : mapSalesOrderApproval(detail.approval),
    }
}
