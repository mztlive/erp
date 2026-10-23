import { listSellableItemsPage } from "@/features/master-data/api"
import { mapWithConcurrency } from "@/features/master-data/lib/sellable-excel-rows"
import { fetchSupplierOfferings } from "@/features/supplier-offerings/api/offerings"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"
import { fetchInquiry } from "../api"
import {
    groupSupplierQuotes,
    supplierQuoteLine,
    type SupplierQuoteSource,
} from "../lib/supplier-quote-export"

export type SupplierQuoteRequest = {
    inquiryId: string
    taskId: string
    version: number
    taskVersion: number
}

/** 导出不能因空页、重复页或范围变化而悄悄漏掉供应商。 */
export async function allOfferings(
    skuId: string,
): Promise<SupplierOfferingView[]> {
    const items: SupplierOfferingView[] = []
    const seen = new Set<string>()
    let scopeVersion: string | undefined
    let expectedTotal: number | undefined
    for (let page = 1; page <= 100; page += 1) {
        const result = await fetchSupplierOfferings({
            skuId,
            page,
            pageSize: 100,
            scopeVersion,
        })
        expectedTotal ??= result.total
        if (
            result.total !== expectedTotal ||
            (page > 1 && result.scope_version !== scopeVersion)
        )
            throw new Error("供给关系已变化，请刷新供给资料后重试")
        scopeVersion = result.scope_version
        for (const item of result.items) {
            if (item.sku_id !== skuId || seen.has(item.id))
                throw new Error("供给关系读取结果不完整，请刷新供给资料后重试")
            seen.add(item.id)
            items.push(item)
        }
        if (items.length === expectedTotal) return items
        if (items.length > expectedTotal || result.items.length === 0) break
    }
    throw new Error("供给关系未能完整读取，请缩小商品范围后重试")
}

export async function fetchSupplierQuoteInquiry(request: SupplierQuoteRequest) {
    const inquiry = await fetchInquiry(request.inquiryId, true, request.taskId)
    if (!("work_items" in inquiry))
        throw new Error("请从采购询价页面生成报价表")
    const task = inquiry.work_items.find((item) => item.id === request.taskId)
    if (
        !task ||
        task.version !== request.taskVersion ||
        inquiry.version !== request.version ||
        !["awaiting_procurement", "procurement_processing"].includes(
            inquiry.status,
        )
    )
        throw new Error("询价需求或采购任务已变化，请刷新询价页面后重新生成")
    return { inquiry, task }
}

/** 先由采购详情接口重新证明当前任务，再读取已有权限覆盖的供给与商品资料。 */
export async function fetchSupplierQuoteSource(
    request: SupplierQuoteRequest,
): Promise<SupplierQuoteSource> {
    const { inquiry, task } = await fetchSupplierQuoteInquiry(request)
    const demandLines = inquiry.draft.lines.filter((line) =>
        task.line_ids.includes(line.line_id),
    )
    if (demandLines.length === 0)
        throw new Error("当前采购任务没有可导出的商品")
    if (demandLines.length > 200)
        throw new Error("一次最多生成 200 条需求商品，请缩小范围")
    const skuIds = [...new Set(demandLines.map((line) => line.sku_id))]
    const batches: string[][] = []
    for (let offset = 0; offset < skuIds.length; offset += 50)
        batches.push(skuIds.slice(offset, offset + 50))
    const [offerings, products] = await Promise.all([
        mapWithConcurrency(skuIds, 4, allOfferings),
        mapWithConcurrency(batches, 4, (batch) =>
            listSellableItemsPage({
                resource: "sellable-items",
                skuIds: batch,
                page: 1,
                pageSize: 50,
            }),
        ),
    ])
    const details = new Map(
        products.flatMap((page) =>
            page.rows.map((row) => [row.stableId, row.sellableItem] as const),
        ),
    )
    const lines = demandLines.map((line) => {
        const product = details.get(line.sku_id)
        const unit =
            product?.baseUnitCode === line.unit_code && product.baseUnit !== "—"
                ? product.baseUnit
                : ""
        return supplierQuoteLine(line, unit ?? "", product?.mainImageAssetId)
    })
    return {
        inquiryId: inquiry.id,
        inquiryNo: inquiry.inquiry_no,
        feedbackRound: inquiry.feedback_round + 1,
        version: inquiry.version,
        requirementVersion: inquiry.requirement_version,
        deadlineAt: inquiry.deadline_at,
        taskId: task.id,
        taskVersion: task.version,
        lines,
        groups: groupSupplierQuotes(lines, offerings.flat()),
    }
}
