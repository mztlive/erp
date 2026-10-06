"use client"

import type { ReactNode } from "react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { paymentTermCode, paymentTermLabel } from "@/lib/business-options"
import { compareDecimal } from "@/lib/fixed-decimal"
import { periodicSettlement } from "@/lib/supplier-payment-terms"
import { percentageFromRate } from "@/features/supplier-offerings/lib/offering-forms"
import type { PortalApplication } from "@/features/supplier-portal/types"

type Fields = Record<string, unknown>
type FieldKind =
    | "text"
    | "money"
    | "rate"
    | "quantity"
    | "list"
    | "capabilities"
    | "date"
    | "end_date"
    | "settlement"
    | "cycle"
    | "payment"
    | "availability"
    | "relation"
    | "product_kind"
type FieldSpec = { key: string; label: string; kind: FieldKind }
const field = (
    key: string,
    label: string,
    kind: FieldKind = "text",
): FieldSpec => ({ key, label, kind })
const termsFields: FieldSpec[] = [
    field("dropship_supply_price_gross", "代发含税供货价", "money"),
    field("bulk_supply_price_gross", "集采含税供货价", "money"),
    field("input_tax_rate", "税率", "rate"),
    field("bulk_minimum_order_quantity", "集采起订量", "quantity"),
    field("supply_region", "可供区域", "list"),
    field("product_capabilities", "商品能力", "capabilities"),
    field("valid_from", "生效日期", "date"),
    field("valid_to", "失效日期", "end_date"),
    field("dropship_express", "代发快递说明"),
    field("freight_amount", "运费", "money"),
    field("service_fee_amount", "服务费", "money"),
]
const cooperationFields: FieldSpec[] = [
    field("settlement_mode", "结算方式", "settlement"),
    field("reconciliation_cycle", "对账周期", "cycle"),
    field("payment_term", "付款条件", "payment"),
]
const productFields: FieldSpec[] = [
    field("name", "商品名称"),
    field("product_kind", "商品类型", "product_kind"),
    field("brand", "品牌原始资料"),
    field("category", "分类原始完整路径"),
    field("model", "型号"),
    field("description", "商品说明"),
]
const skuFields: FieldSpec[] = [
    field("name", "SKU名称"),
    field("unit", "单位及包装原始资料"),
    field("ordering_code", "供应商订货编码"),
    field("barcode", "条码"),
    field("available_quantity", "可供数量", "quantity"),
]
const capabilityLabels: Record<string, string> = {
    ORDER: "下单",
    REFUND: "退款",
    REDEEM: "核销",
    REDEMPTION: "核销",
    BALANCE: "余额查询",
    INVENTORY: "库存查询",
    STOCK: "库存查询",
    DROPSHIP: "代发",
    BULK: "集采",
    TRACKING: "物流查询",
    CANCEL: "取消订单",
    RETURN: "退货",
    EXCHANGE: "换货",
}
const relationLabels: Record<string, string> = {
    ACTIVE: "合作中",
    PAUSED: "采购已暂停",
    STOPPED: "停止供应",
}
const availabilityLabels: Record<string, string> = {
    AVAILABLE: "有货",
    OUT_OF_STOCK: "临时缺货",
    UNAVAILABLE: "临时缺货",
    STOPPED: "停止供应",
    STALE: "待重新核对",
}
const productKindLabels: Record<string, string> = {
    PHYSICAL: "实物",
    VIRTUAL: "虚拟",
    OFFLINE_SERVICE: "线下服务",
    VOUCHER: "卡券",
}
const record = (value: unknown): Fields =>
    value && typeof value === "object" && !Array.isArray(value)
        ? (value as Fields)
        : {}
function unpack(value: unknown): Fields {
    const source = record(value)
    return Object.keys(record(source.snapshot)).length
        ? unpack(source.snapshot)
        : Object.keys(record(source.input)).length
          ? unpack(source.input)
          : Object.keys(record(source.proposal)).length
            ? unpack(source.proposal)
            : source
}
function text(value: unknown): string {
    if (typeof value === "string") return value
    const input = record(value)
    return typeof input.raw_name === "string"
        ? input.raw_name
        : typeof input.name === "string"
          ? input.name
          : ""
}
function chineseValue(value: string): string {
    return /[\u3400-\u9fff]/u.test(value) ? value : "待核对"
}
function valueContent(
    value: unknown,
    kind: FieldKind,
    unit: string,
): ReactNode {
    const raw = text(value)
    if (value === undefined)
        return <span className="text-muted-foreground">未读取</span>
    if (kind === "end_date") return raw || "长期有效"
    if (kind === "money")
        return <MoneyValue value={raw || null} taxBasis="gross" />
    if (kind === "rate") {
        try {
            return raw ? (
                <RateValue value={percentageFromRate(raw)} precision={2} />
            ) : (
                "未填写"
            )
        } catch {
            return "待核对"
        }
    }
    if (kind === "quantity")
        return raw ? <QuantityValue value={raw} unit={unit} /> : "未提供"
    if (kind === "list" || kind === "capabilities") {
        const values = Array.isArray(value)
            ? value.filter((item): item is string => typeof item === "string")
            : raw
              ? [raw]
              : []
        return values.length
            ? values
                  .map((item) =>
                      kind === "capabilities"
                          ? (capabilityLabels[item.toUpperCase()] ??
                            chineseValue(item))
                          : item,
                  )
                  .join("、")
            : "未标注"
    }
    if (kind === "settlement")
        return (
            periodicSettlement(raw)?.label ??
            {
                prepayment: "预付款",
                cash_settlement: "现结",
                pay_after_use: "货到后付（历史）",
            }[raw] ??
            (raw ? chineseValue(raw) : "未填写")
        )
    if (kind === "cycle")
        return (
            periodicSettlement(raw)?.label.replace(/结$/, "对账") ??
            { none: "无固定周期", NONE: "无固定周期" }[raw] ??
            (raw ? chineseValue(raw) : "未填写")
        )
    if (kind === "payment")
        return raw
            ? paymentTermCode(raw)
                ? paymentTermLabel(raw)
                : chineseValue(raw)
            : "未填写"
    if (kind === "availability")
        return (
            availabilityLabels[raw.toUpperCase()] ??
            (raw ? chineseValue(raw) : "未填写")
        )
    if (kind === "relation")
        return (
            relationLabels[raw.toUpperCase()] ??
            (raw ? chineseValue(raw) : "未填写")
        )
    if (kind === "product_kind")
        return (
            productKindLabels[raw.toUpperCase()] ??
            (raw ? chineseValue(raw) : "未填写")
        )
    return raw || "未填写"
}
function normalized(value: unknown, kind: FieldKind): string {
    if (kind === "list" || kind === "capabilities")
        return JSON.stringify(
            Array.isArray(value)
                ? [
                      ...new Set(value.map(text).map((item) => item.trim())),
                  ].sort()
                : [],
        )
    return text(value).trim()
}
function changeLabel(
    before: unknown,
    after: unknown,
    kind: FieldKind,
    existing: boolean,
): string {
    if (after === undefined) return "不在本次修改范围"
    if (before === undefined) return existing ? "当前值待核对" : "本次新增"
    if (
        ["money", "rate", "quantity"].includes(kind) &&
        text(before) &&
        text(after)
    ) {
        try {
            const compared = compareDecimal(
                text(after),
                text(before),
                kind === "money" ? 4 : 6,
            )
            return compared === 0 ? "保持不变" : compared > 0 ? "提高" : "降低"
        } catch {
            return "需要核对"
        }
    }
    return normalized(before, kind) === normalized(after, kind)
        ? "保持不变"
        : "调整"
}
function comparison(
    title: string,
    before: Fields,
    after: Fields,
    fields: FieldSpec[],
    existing: boolean,
    unit = "",
) {
    return (
        <section className="space-y-3">
            <h3 className="font-semibold">{title}</h3>
            <div className="rounded-lg border">
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>核对项目</TableHead>
                            <TableHead>当前生效值</TableHead>
                            <TableHead>本次拟生效值</TableHead>
                            <TableHead>变化</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {fields.map((item) => (
                            <TableRow key={item.key}>
                                <TableCell className="font-medium">
                                    {item.label}
                                </TableCell>
                                <TableCell className="min-w-36 whitespace-normal">
                                    {valueContent(
                                        before[item.key],
                                        item.kind,
                                        unit,
                                    )}
                                </TableCell>
                                <TableCell className="min-w-36 whitespace-normal">
                                    {valueContent(
                                        after[item.key],
                                        item.kind,
                                        unit,
                                    )}
                                </TableCell>
                                <TableCell className="whitespace-normal">
                                    {changeLabel(
                                        before[item.key],
                                        after[item.key],
                                        item.kind,
                                        existing,
                                    )}
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </div>
        </section>
    )
}

/** 审核对比只读取明确列出的商务与商品字段，不展开内部原始对象。 */
export function PortalReviewDiff({
    application,
}: {
    application: PortalApplication
}) {
    const proposal = unpack(application.submitted_snapshot ?? application.input)
    const current = unpack(application.current)
    const currentLoaded = Object.keys(current).length > 0
    const unit =
        text(current.unit_name) ||
        text(current.base_unit) ||
        text(proposal.unit_name)
    const proposedTerms = Object.keys(record(proposal.terms)).length
        ? record(proposal.terms)
        : proposal
    const currentTerms = Object.keys(record(current.terms)).length
        ? record(current.terms)
        : current
    const cooperationCurrent = Object.keys(record(current.profile)).length
        ? record(current.profile)
        : Object.keys(record(current.commercial_profile)).length
          ? record(current.commercial_profile)
          : current
    const newProduct = application.kind === "new_product"
    const productSkus = Array.isArray(proposal.skus)
        ? proposal.skus.map(record)
        : []
    const currentSkus = Array.isArray(current.skus)
        ? current.skus.map(record)
        : []
    return (
        <section className="space-y-5 rounded-xl border bg-card p-5">
            <div>
                <h2 className="font-semibold">逐项核对及生效影响</h2>
                <p className="mt-1 text-sm text-muted-foreground">
                    本次拟生效值采用供应商本次提交内容。采购确认前，当前价格、供货条款及合作条件继续有效。
                </p>
            </div>
            {!currentLoaded &&
                (application.kind === "terms" ||
                    application.kind === "cooperation" ||
                    application.kind === "stop") && (
                    <p className="rounded-lg border p-3 text-sm text-warning-soft-foreground">
                        当前生效资料尚未读取，请重新读取申请并核对当前资料后再确认；不能据此认定条款没有变化。
                    </p>
                )}
            {(application.kind === "quote" || application.kind === "terms") &&
                comparison(
                    "供货条款",
                    currentTerms,
                    proposedTerms,
                    termsFields,
                    application.kind === "terms" || currentLoaded,
                    unit,
                )}
            {application.kind === "quote" &&
                comparison(
                    "首次报价与可供情况",
                    current,
                    proposal,
                    [
                        field("supplier_sku_code", "供应商订货编码"),
                        field(
                            "availability_status",
                            "可供状态",
                            "availability",
                        ),
                        field("available_quantity", "可供数量", "quantity"),
                    ],
                    currentLoaded,
                    unit,
                )}
            {application.kind === "cooperation" &&
                comparison(
                    "合作付款条件",
                    cooperationCurrent,
                    proposal,
                    cooperationFields,
                    true,
                )}
            {application.kind === "stop" &&
                comparison(
                    "供给关系",
                    current,
                    { status: "STOPPED" },
                    [field("status", "供给关系", "relation")],
                    true,
                )}
            {newProduct && (
                <>
                    {comparison(
                        "商品公共资料",
                        current,
                        proposal,
                        productFields,
                        currentLoaded,
                    )}
                    {productSkus.map((sku, index) => {
                        const previous =
                            currentSkus.find(
                                (item) =>
                                    (item.row_id &&
                                        item.row_id === sku.row_id) ||
                                    (item.ordering_code &&
                                        item.ordering_code ===
                                            sku.ordering_code),
                            ) ?? {}
                        const skuUnit = text(sku.unit)
                        return (
                            <section
                                key={
                                    typeof sku.row_id === "string"
                                        ? sku.row_id
                                        : String(index)
                                }
                                className="space-y-4 rounded-lg border p-4"
                            >
                                <h3 className="font-semibold">
                                    规格{index + 1} ·{" "}
                                    {text(sku.name) || "未填写名称"}
                                </h3>
                                {comparison(
                                    "规格原稿",
                                    previous,
                                    sku,
                                    skuFields,
                                    Object.keys(previous).length > 0,
                                    skuUnit,
                                )}
                                {comparison(
                                    "首次供货条款",
                                    record(
                                        previous.supply_terms ?? previous.terms,
                                    ),
                                    record(sku.supply_terms),
                                    termsFields,
                                    Object.keys(previous).length > 0,
                                    skuUnit,
                                )}
                            </section>
                        )
                    })}
                </>
            )}
            <div className="space-y-2 rounded-lg border bg-muted/30 p-4 text-sm">
                {(application.kind === "quote" || newProduct) && (
                    <p>
                        复用已上架的公司SKU并增加有效供给后，可能立即参与或恢复销售及采购选源；须确认目标规格、基础单位及供货资格。
                    </p>
                )}
                {newProduct && (
                    <p>
                        本次新建SKU保持未上架，由内部定价并上架；复用已有商品或SKU时，原商品资料、原规格及原上架状态保持不变。
                    </p>
                )}
                {application.kind === "stop" && (
                    <p>
                        采购确认后停止当前供给关系。该申请不会自动取消已冻结的采购或解除已经存在的履约义务；请另行处理在途业务。
                    </p>
                )}
                <p>
                    更新可供或恢复有货不会解除采购暂停、停止关系或其他供货限制。已冻结采购单继续采用原价格、条款及付款条件。
                </p>
                {application.kind === "cooperation" && (
                    <p>
                        新付款条件通过确认后用于后续业务；现有采购单的冻结付款条件及已登记付款保持原处理口径。
                    </p>
                )}
            </div>
        </section>
    )
}
