import { kindLabels } from "@/features/supplier-portal/lib/presentation"
import type { PortalApplication } from "@/features/supplier-portal/types"
import {
    compareDecimal,
    formatCurrencyFixed,
    normalizeFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"

export function reviewRecord(value: unknown): Record<string, unknown> {
    return value && typeof value === "object" && !Array.isArray(value)
        ? (value as Record<string, unknown>)
        : {}
}

export function reviewSnapshot(value: unknown): Record<string, unknown> {
    const source = reviewRecord(value)
    for (const next of [source.snapshot, source.input, source.proposal]) {
        if (Object.keys(reviewRecord(next)).length) return reviewSnapshot(next)
    }
    return source
}

export function reviewText(value: unknown): string {
    if (typeof value === "string") return value.trim()
    const source = reviewRecord(value)
    return reviewText(source.raw_name ?? source.name ?? "")
}

function price(value: unknown): string | null {
    if (typeof value !== "string" && typeof value !== "number") return null
    try {
        return normalizeFixed(String(value), { maxScale: 8, outputScale: 4 })
    } catch {
        return null
    }
}

function currency(value: string) {
    return formatCurrencyFixed(value, {
        maxScale: 4,
        minimumFractionDigits: 2,
        maximumFractionDigits: 4,
    })
}

export function portalReviewOverview(application: PortalApplication) {
    const proposal = reviewSnapshot(
        application.submitted_snapshot ?? application.input,
    )
    const current = reviewSnapshot(application.current)
    const terms = reviewRecord(proposal.terms ?? proposal)
    const currentTerms = reviewRecord(current.terms ?? current)
    const currentSku = reviewRecord(current.sku)
    const skus = Array.isArray(proposal.skus)
        ? proposal.skus.map(reviewRecord)
        : []
    const newProduct = application.kind === "new_product"
    const priced = application.kind === "quote" || application.kind === "terms"
    const dropship = price(terms.dropship_supply_price_gross)
    const bulk = price(terms.bulk_supply_price_gross)
    const quotedPrice = dropship ?? bulk
    const previousPrice = price(
        dropship === null
            ? currentTerms.bulk_supply_price_gross
            : currentTerms.dropship_supply_price_gross,
    )
    const priceLabel = dropship === null ? "集采含税单价" : "代发含税单价"
    const latest = application.submissions?.at(-1)
    const title =
        reviewText(application.title) ||
        reviewText(proposal.name) ||
        reviewText(current.name) ||
        reviewText(current.sku_name) ||
        reviewText(currentSku.name)
    const currentRequired =
        application.kind === "terms" ||
        application.kind === "stop" ||
        application.kind === "cooperation"
    const missingCurrent = currentRequired && !Object.keys(current).length
    const qualificationChecks = newProduct
        ? [
              { label: "品牌", value: reviewText(proposal.brand) },
              { label: "商品分类", value: reviewText(proposal.category) },
              {
                  label: "规格单位",
                  value:
                      skus.length && skus.every((sku) => reviewText(sku.unit))
                          ? "已提报"
                          : "",
              },
          ]
        : [
              {
                  label: currentRequired ? "当前生效资料" : "供货报价",
                  value: currentRequired
                      ? missingCurrent
                          ? ""
                          : "已提供"
                      : (quotedPrice ?? ""),
              },
              ...(priced && currentRequired
                  ? [{ label: "供货报价", value: quotedPrice ?? "" }]
                  : []),
          ]
    const outstanding = qualificationChecks.filter((check) => !check.value)
    const priceDelta = (() => {
        if (!priced || quotedPrice === null || previousPrice === null) {
            return null
        }
        const delta = subtractFixed(quotedPrice, previousPrice, {
            maxScale: 4,
            outputScale: 4,
        })
        const cmp = compareDecimal(delta, "0", 4)
        if (cmp === 0) {
            return {
                amount: delta,
                direction: "flat" as const,
                label: `持平 ${currency("0")}`,
            }
        }
        if (cmp > 0) {
            return {
                amount: delta,
                direction: "up" as const,
                label: `涨 ${currency(delta)}`,
            }
        }
        return {
            amount: delta,
            direction: "down" as const,
            label: `降 ${currency(delta.replace(/^-/, ""))}`,
        }
    })()
    const impact = priced
        ? priceDelta
            ? `单价${priceDelta.label}`
            : bulk !== null && dropship !== null
              ? `集采含税单价 ${currency(bulk)}`
              : "适用于后续供货"
        : application.kind === "stop"
          ? "停止后续选源"
          : application.kind === "cooperation"
            ? "更新后续业务付款条件"
            : "新增规格进入未上架商品库"
    return {
        kind: kindLabels[application.kind] ?? "供应商申请",
        title,
        supplier: reviewText(application.supplier_name),
        reason: reviewText(application.reason),
        submittedAt: latest?.submitted_at,
        skuCount: skus.length,
        qualifiedCount: qualificationChecks.length - outstanding.length,
        qualificationChecks,
        outstanding,
        price: priced ? quotedPrice : null,
        previousPrice: priced ? previousPrice : null,
        priceDelta,
        priceLabel,
        impact,
        brand: newProduct ? reviewText(proposal.brand) : "",
        category: newProduct ? reviewText(proposal.category) : "",
        validFrom: reviewText(terms.valid_from),
        validTo: reviewText(terms.valid_to),
        orderingCode: reviewText(
            proposal.supplier_sku_code ?? current.supplier_sku_code,
        ),
        offeringId: reviewText(
            proposal.offering_id ?? application.input.offering_id,
        ),
    }
}

export function portalReviewMaterialRefs(application?: PortalApplication) {
    if (application?.kind !== "new_product") return []
    const product = reviewSnapshot(
        application.submitted_snapshot ?? application.input,
    )
    const ids = (values: unknown) =>
        Array.isArray(values)
            ? values.filter(
                  (id): id is string => typeof id === "string" && Boolean(id),
              )
            : []
    const skus = Array.isArray(product.skus)
        ? product.skus.map(reviewRecord)
        : []
    const files = new Map<
        string,
        { id: string; label: string; mimeType?: string }
    >()
    for (const id of ids(product.image_asset_ids)) {
        files.set(id, { id, label: "商品图片", mimeType: "image/*" })
    }
    for (const id of ids(product.file_asset_ids)) {
        if (!files.has(id)) files.set(id, { id, label: "商品资料" })
    }
    for (const sku of skus) {
        const id = reviewText(sku.image_asset_id)
        if (id && !files.has(id)) {
            const name = reviewText(sku.name)
            files.set(id, {
                id,
                label: name ? `${name} · 规格图片` : "规格图片",
                mimeType: "image/*",
            })
        }
    }
    return [...files.values()]
}
