"use client"

import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business/detail-workspace"
import { useRef, useState, type ReactNode } from "react"
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    SearchIcon,
    XIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { paymentTermCode, paymentTermLabel } from "@/lib/business-options"
import { compareDecimal } from "@/lib/fixed-decimal"
import {
    periodicSettlement,
    reconciliationCycleLabel,
} from "@/lib/supplier-payment-terms"
import { percentageFromRate } from "@/features/supplier-offerings/lib/offering-forms"
import type { PortalApplication } from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import {
    reviewRecord as record,
    reviewSnapshot as unpack,
} from "../lib/review-overview"

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
    field("product_capabilities", "商品能力", "capabilities"),
    field("valid_from", "生效日期", "date"),
    field("valid_to", "失效日期", "end_date"),
]
const cooperationFields: FieldSpec[] = [
    field("settlement_mode", "结算方式", "settlement"),
    field("reconciliation_cycle", "对账频率", "cycle"),
    field("payment_term", "付款条件", "payment"),
]
const productFields: FieldSpec[] = [
    field("name", "商品名称"),
    field("product_kind", "商品类型", "product_kind"),
    field("brand", "品牌名称"),
    field("category", "建议商品分类"),
    field("model", "型号"),
    field("description", "商品说明"),
]
const skuFields: FieldSpec[] = [
    field("name", "规格名称"),
    field("unit", "计量单位与包装规格"),
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
            reconciliationCycleLabel(raw) ??
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
const NUMERIC_KINDS = new Set<FieldKind>(["money", "rate", "quantity"])

function changeInfo(
    before: unknown,
    after: unknown,
    kind: FieldKind,
    existing: boolean,
): {
    label: string
    tone: "neutral" | "info" | "success" | "warning"
} {
    if (after === undefined) {
        return { label: "不在本次修改范围", tone: "neutral" }
    }
    if (before === undefined) {
        return existing
            ? { label: "当前值待核对", tone: "warning" }
            : { label: "本次新增", tone: "info" }
    }
    if (NUMERIC_KINDS.has(kind) && text(before) && text(after)) {
        try {
            const compared = compareDecimal(
                text(after),
                text(before),
                kind === "money" ? 4 : 6,
            )
            if (compared === 0) return { label: "保持不变", tone: "neutral" }
            return compared > 0
                ? { label: "提高", tone: "warning" }
                : { label: "降低", tone: "success" }
        } catch {
            return { label: "需要核对", tone: "warning" }
        }
    }
    return normalized(before, kind) === normalized(after, kind)
        ? { label: "保持不变", tone: "neutral" }
        : { label: "调整", tone: "info" }
}

function comparison(
    title: string,
    before: Fields,
    after: Fields,
    fields: FieldSpec[],
    existing: boolean,
    unit = "",
    compact = false,
) {
    return (
        <section className="space-y-2">
            <h3 className="text-body-compact font-semibold">{title}</h3>
            <div
                className={
                    compact
                        ? "min-w-0 overflow-hidden rounded-lg"
                        : detailWorkspaceStyles.dataTableWrapper
                }
            >
                <Table className={cn(compact && "w-full table-fixed")}>
                    <TableHeader
                        className={cn(
                            compact &&
                                "bg-surface-sunken [&_tr]:border-0 [&_th]:whitespace-normal [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md",
                        )}
                    >
                        <TableRow>
                            <TableHead>核对项目</TableHead>
                            <TableHead>当前生效值</TableHead>
                            <TableHead>本次拟生效值</TableHead>
                            <TableHead>变化</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {fields.map((item) => {
                            const isNumeric = NUMERIC_KINDS.has(item.kind)
                            const isCodeOrDate =
                                item.kind === "date" ||
                                item.kind === "end_date" ||
                                item.key === "ordering_code" ||
                                item.key === "supplier_sku_code" ||
                                item.key === "barcode"
                            const change = changeInfo(
                                before[item.key],
                                after[item.key],
                                item.kind,
                                existing,
                            )
                            return (
                                <TableRow key={item.key}>
                                    <TableCell className="wrap-anywhere whitespace-normal font-medium">
                                        {item.label}
                                    </TableCell>
                                    <TableCell
                                        data-align={
                                            isNumeric ? "end" : undefined
                                        }
                                        className={cn(
                                            "wrap-anywhere whitespace-normal",
                                            !compact && "min-w-36",
                                            (isNumeric || isCodeOrDate) &&
                                                "num",
                                        )}
                                    >
                                        {valueContent(
                                            before[item.key],
                                            item.kind,
                                            unit,
                                        )}
                                    </TableCell>
                                    <TableCell
                                        data-align={
                                            isNumeric ? "end" : undefined
                                        }
                                        className={cn(
                                            "wrap-anywhere whitespace-normal",
                                            !compact && "min-w-36",
                                            (isNumeric || isCodeOrDate) &&
                                                "num",
                                        )}
                                    >
                                        {valueContent(
                                            after[item.key],
                                            item.kind,
                                            unit,
                                        )}
                                    </TableCell>
                                    <TableCell className="whitespace-normal">
                                        <StatusBadge
                                            tone={change.tone}
                                            label={change.label}
                                            appearance={
                                                compact ? "text" : undefined
                                            }
                                            className={cn(
                                                compact &&
                                                    "inline whitespace-normal",
                                            )}
                                        />
                                    </TableCell>
                                </TableRow>
                            )
                        })}
                    </TableBody>
                </Table>
            </div>
        </section>
    )
}

/** 审核对比只读取明确列出的商务与商品字段，不展开内部原始对象。 */
export function PortalReviewDiff({
    application,
    compact = false,
}: {
    application: PortalApplication
    compact?: boolean
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
        <DetailWorkspaceSection
            title="逐项核对及生效影响"
            className={cn(compact && "py-3.5 first:pt-3.5 last:pb-3.5")}
        >
            {!compact ? (
                <p className="text-xs text-muted-foreground">
                    本次拟生效值采用供应商本次提交内容。采购确认前，当前价格、供货条款及合作条件继续有效。
                </p>
            ) : null}
            {!currentLoaded &&
                (application.kind === "terms" ||
                    application.kind === "cooperation" ||
                    application.kind === "stop") && (
                    <p className="border-l-2 border-warning-soft-foreground/60 bg-warning-soft/25 px-3.5 py-2.5 text-body-compact text-warning-soft-foreground">
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
                    compact,
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
                    compact,
                )}
            {application.kind === "cooperation" &&
                comparison(
                    "合作付款条件",
                    cooperationCurrent,
                    proposal,
                    cooperationFields,
                    true,
                    "",
                    compact,
                )}
            {application.kind === "stop" &&
                comparison(
                    "供给关系",
                    current,
                    { status: "STOPPED" },
                    [field("status", "供给关系", "relation")],
                    true,
                    "",
                    compact,
                )}
            {newProduct && (
                <>
                    {comparison(
                        "商品公共资料",
                        current,
                        proposal,
                        productFields,
                        currentLoaded,
                        "",
                        compact,
                    )}
                    {compact ? (
                        <SubmittedSkuTable
                            applicationId={application.id}
                            skus={productSkus}
                            currentSkus={currentSkus}
                        />
                    ) : (
                        productSkus.map((sku, index) => {
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
                                    className="flex flex-col gap-4 border-t border-border/60 pt-4"
                                >
                                    <h3 className="font-semibold">
                                        规格{index + 1} ·{" "}
                                        {text(sku.name) || "未填写名称"}
                                    </h3>
                                    {comparison(
                                        "提报规格明细",
                                        previous,
                                        sku,
                                        skuFields,
                                        Object.keys(previous).length > 0,
                                        skuUnit,
                                    )}
                                    {comparison(
                                        "首次供货条款",
                                        record(
                                            previous.supply_terms ??
                                                previous.terms,
                                        ),
                                        record(sku.supply_terms),
                                        termsFields,
                                        Object.keys(previous).length > 0,
                                        skuUnit,
                                    )}
                                </section>
                            )
                        })
                    )}
                </>
            )}
            <div className="space-y-1.5 border-l-2 border-border/80 pl-3.5 text-xs text-muted-foreground">
                {(application.kind === "quote" || newProduct) && (
                    <p>
                        复用已上架的公司商品规格并增加有效供货后，可能立即参与或恢复销售及采购供应选择；须确认目标规格、基础单位及供货资格。
                    </p>
                )}
                {newProduct && (
                    <p>
                        本次新增规格保持未上架，后续由采购方完成合作上架；复用已有商品或规格时，原商品资料及上架状态保持不变。
                    </p>
                )}
                {application.kind === "stop" && (
                    <p>
                        采购确认后停止当前供货关系。该申请不会自动取消已生效的历史采购单或解除已有的履约义务；在途业务需另行处理。
                    </p>
                )}
                <p>
                    更新可供或恢复有货不会解除采购暂停、停止合作或其他供货限制。已生效的历史采购单继续采用原价格、条款及付款条件。
                </p>
                {application.kind === "cooperation" && (
                    <p>
                        新付款条件通过确认后用于后续业务；已生效的历史采购单付款条件及已登记付款保持原处理口径。
                    </p>
                )}
            </div>
        </DetailWorkspaceSection>
    )
}

const SKU_PAGE_SIZE = 20

function SubmittedSkuTable({
    applicationId,
    skus,
    currentSkus,
}: {
    applicationId: string
    skus: readonly Fields[]
    currentSkus: readonly Fields[]
}) {
    const prefix = `supplier-portal-submitted-skus-${toAutomationIdSegment(applicationId)}`
    const [filter, setFilter] = useState("")
    const [page, setPage] = useState(0)
    const [selectedKey, setSelectedKey] = useState<string | null>(null)
    const searchRef = useRef<HTMLInputElement>(null)
    const needle = filter.trim().toLocaleLowerCase()
    const rows = skus.map((sku) => ({
        sku,
        key:
            text(sku.row_id) ||
            JSON.stringify([sku.ordering_code, sku.name, sku.barcode]),
        name: text(sku.name),
        unit: text(sku.unit),
        orderingCode: text(sku.ordering_code),
    }))
    const counts = new Map<string, number>()
    for (const { key } of rows) counts.set(key, (counts.get(key) ?? 0) + 1)
    const filtered = rows.filter(
        (row) =>
            !needle ||
            [row.name, row.unit, row.orderingCode].some((value) =>
                value.toLocaleLowerCase().includes(needle),
            ),
    )
    const pageCount = Math.max(1, Math.ceil(filtered.length / SKU_PAGE_SIZE))
    const currentPage = Math.min(page, pageCount - 1)
    const visible = filtered.slice(
        currentPage * SKU_PAGE_SIZE,
        (currentPage + 1) * SKU_PAGE_SIZE,
    )
    const selected = visible.find((row) => row.key === selectedKey)
    const previous =
        currentSkus.find(
            (sku) =>
                selected &&
                ((sku.row_id && sku.row_id === selected.sku.row_id) ||
                    (sku.ordering_code &&
                        sku.ordering_code === selected.sku.ordering_code)),
        ) ?? {}
    const hasCurrent = Object.keys(previous).length > 0

    return (
        <section className="space-y-2" aria-label="提报规格与资料">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-body-compact font-semibold">
                    提报规格{" "}
                    <span className="num text-xs font-normal text-muted-foreground">
                        {skus.length} 项
                    </span>
                </h3>
                {skus.length > 8 ? (
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                        {needle ? (
                            <Button
                                id={`${prefix}-clear-filter`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                onClick={() => {
                                    setFilter("")
                                    setPage(0)
                                    setSelectedKey(null)
                                }}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                        <div className="relative flex w-full max-w-64 items-center">
                            <SearchIcon
                                className="pointer-events-none absolute left-2.5 size-3.5 text-muted-foreground"
                                aria-hidden="true"
                            />
                            <Input
                                ref={searchRef}
                                id={`${prefix}-filter`}
                                type="text"
                                aria-label="筛选规格"
                                placeholder="筛选规格、单位或订货编码"
                                className="h-control-sm w-full pr-7 pl-8 text-xs"
                                value={filter}
                                onChange={(event) => {
                                    setFilter(event.target.value)
                                    setPage(0)
                                    setSelectedKey(null)
                                }}
                            />
                            {filter ? (
                                <Button
                                    id={`${prefix}-filter-clear`}
                                    type="button"
                                    size="icon-xs"
                                    variant="ghost"
                                    aria-label="清除规格筛选"
                                    className="absolute right-0.5"
                                    onClick={() => {
                                        setFilter("")
                                        setPage(0)
                                        setSelectedKey(null)
                                        searchRef.current?.focus()
                                    }}
                                >
                                    <XIcon aria-hidden="true" />
                                </Button>
                            ) : null}
                        </div>
                    </div>
                ) : null}
            </div>
            {visible.length ? (
                <div className="min-w-0 space-y-2">
                    <Table className="w-full table-fixed">
                        <TableHeader className="bg-surface-sunken [&_tr]:border-0 [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md">
                            <TableRow>
                                <TableHead className="w-2/5 whitespace-normal">
                                    规格
                                </TableHead>
                                <TableHead className="whitespace-normal">
                                    单位
                                </TableHead>
                                <TableHead className="whitespace-normal">
                                    订货编码
                                </TableHead>
                                <TableHead
                                    className="whitespace-normal"
                                    data-align="end"
                                >
                                    含税单价
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {visible.map(
                                (
                                    { sku, key, name, unit, orderingCode },
                                    index,
                                ) => {
                                    const supply = record(sku.supply_terms)
                                    const amount =
                                        supply.dropship_supply_price_gross ??
                                        supply.bulk_supply_price_gross
                                    const unique = counts.get(key) === 1
                                    return (
                                        <TableRow
                                            key={
                                                unique ? key : `${key}-${index}`
                                            }
                                        >
                                            <TableCell className="wrap-anywhere whitespace-normal">
                                                {unique ? (
                                                    <button
                                                        id={`${prefix}-row-${toAutomationIdSegment(encodeURIComponent(key))}`}
                                                        type="button"
                                                        aria-expanded={
                                                            key === selectedKey
                                                        }
                                                        aria-controls={
                                                            key === selectedKey
                                                                ? `${prefix}-detail`
                                                                : undefined
                                                        }
                                                        className="cursor-pointer text-left font-medium text-primary underline-offset-2 hover:underline"
                                                        onClick={() =>
                                                            setSelectedKey(
                                                                key ===
                                                                    selectedKey
                                                                    ? null
                                                                    : key,
                                                            )
                                                        }
                                                    >
                                                        {name ||
                                                            "未填写规格名称"}
                                                    </button>
                                                ) : (
                                                    name || "未填写规格名称"
                                                )}
                                            </TableCell>
                                            <TableCell className="wrap-anywhere whitespace-normal">
                                                {unit || "—"}
                                            </TableCell>
                                            <TableCell className="num wrap-anywhere whitespace-normal">
                                                {orderingCode || "—"}
                                            </TableCell>
                                            <TableCell
                                                className="num wrap-anywhere whitespace-normal"
                                                data-align="end"
                                            >
                                                {valueContent(
                                                    amount,
                                                    "money",
                                                    "",
                                                )}
                                                {amount != null ? (
                                                    <p className="text-tiny text-muted-foreground">
                                                        {supply.dropship_supply_price_gross !=
                                                        null
                                                            ? "代发"
                                                            : "集采"}
                                                    </p>
                                                ) : null}
                                            </TableCell>
                                        </TableRow>
                                    )
                                },
                            )}
                        </TableBody>
                        <TableFooter className="border-0 bg-surface-sunken [&_tr]:border-b-0">
                            <TableRow className="hover:bg-transparent">
                                <TableCell
                                    colSpan={4}
                                    className="num rounded-md text-xs whitespace-normal"
                                >
                                    全部合计 {skus.length} 项规格
                                    {needle ? (
                                        <span className="ml-1 text-tiny text-muted-foreground">
                                            （当前显示 {visible.length} 项
                                            {filtered.length > visible.length
                                                ? `，筛选命中 ${filtered.length} 项`
                                                : ""}
                                            ，合计按全部）
                                        </span>
                                    ) : null}
                                </TableCell>
                            </TableRow>
                        </TableFooter>
                    </Table>
                    {filtered.length > SKU_PAGE_SIZE ? (
                        <nav
                            aria-label="提报规格分页"
                            className="flex flex-wrap items-center justify-end gap-2 text-xs text-muted-foreground"
                        >
                            <span className="num" aria-live="polite">
                                第 {currentPage + 1} / {pageCount} 页 ·{" "}
                                {filtered.length} 项
                            </span>
                            <Button
                                id={`${prefix}-prev`}
                                type="button"
                                size="icon-xs"
                                variant="outline"
                                aria-label="上一页"
                                disabled={currentPage === 0}
                                onClick={() => setPage(currentPage - 1)}
                            >
                                <ChevronLeftIcon aria-hidden="true" />
                            </Button>
                            <Button
                                id={`${prefix}-next`}
                                type="button"
                                size="icon-xs"
                                variant="outline"
                                aria-label="下一页"
                                disabled={currentPage + 1 >= pageCount}
                                onClick={() => setPage(currentPage + 1)}
                            >
                                <ChevronRightIcon aria-hidden="true" />
                            </Button>
                        </nav>
                    ) : null}
                </div>
            ) : (
                <p role="status" className="text-xs text-muted-foreground">
                    {skus.length
                        ? "没有符合筛选条件的规格。"
                        : "本次申请未提报规格。"}
                </p>
            )}
            {selected ? (
                <section
                    id={`${prefix}-detail`}
                    className="space-y-3 border-t border-border/60 pt-3"
                    aria-label={`核对 ${selected.name || "提报规格"}`}
                >
                    {comparison(
                        "提报规格明细",
                        previous,
                        selected.sku,
                        skuFields,
                        hasCurrent,
                        selected.unit,
                        true,
                    )}
                    {comparison(
                        "首次供货条款",
                        record(previous.supply_terms ?? previous.terms),
                        record(selected.sku.supply_terms),
                        termsFields,
                        hasCurrent,
                        selected.unit,
                        true,
                    )}
                </section>
            ) : null}
        </section>
    )
}
