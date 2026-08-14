/**
 * W22 商品发布 · 变更命令（发布修订 / 手动暂停 / 重试发送）。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"

import {
    mapDeliveryStatus,
    secsToIso,
    toBackendMediaRole,
    toBackendSaleStatus,
} from "@/features/product-publications/api/mappers"
import type {
    BackendDeliveryResult,
    BackendPublication,
    BackendRevision,
} from "@/features/product-publications/api/wire-types"
import type {
    ManualPauseCommand,
    ManualPauseResult,
    PublishRevisionCommand,
    PublishRevisionResult,
    RetryDeliveryCommand,
    RetryDeliveryResult,
} from "@/features/product-publications/types"

export async function publishRevision(
    command: PublishRevisionCommand,
): Promise<PublishRevisionResult> {
    const content = command.content
    const revision = await apiPost<BackendRevision>(
        `/admin/product-publications/${encodeURIComponent(command.publicationId)}/revisions`,
        {
            sku_revision_id: content.skuRevisionId,
            supplier_offering_revision_id: content.supplierOfferingRevisionId,
            category_id: content.categoryId,
            name: content.name,
            specification: content.specification || null,
            sales_description: content.salesDescription,
            minimum_purchase_quantity: content.minimumPurchaseQuantity,
            sales_price_gross: content.salesPriceGross,
            sales_tax_rate: content.salesTaxRate,
            base_unit_code: content.baseUnitCode,
            sales_region: content.salesRegion?.join(",") || null,
            sale_status: toBackendSaleStatus(content.saleStatus),
            product_capabilities: content.productCapabilities.map((c) =>
                c.toLowerCase(),
            ),
            valid_from:
                Math.floor(new Date(content.validFrom).getTime() / 1000) || 1,
            valid_to: content.validTo
                ? Math.floor(new Date(content.validTo).getTime() / 1000)
                : null,
            media: content.media.map((m) => ({
                file_asset_id: m.fileAssetId,
                media_role: toBackendMediaRole(m.mediaRole),
                sort_no: m.sortNo,
                alt_text: m.altText || null,
            })),
        },
    )

    const delivery = await apiPost<BackendDeliveryResult>(
        `/admin/product-publications/${encodeURIComponent(command.publicationId)}/revisions/${revision.revision_no}/deliver`,
        { idempotency_key: command.requestId },
    )

    return {
        status: "succeeded",
        operationId: delivery.inbox_message_id,
        publicationId: command.publicationId,
        revisionId: revision.id,
        revisionNo: revision.revision_no,
        deliveryId: delivery.delivery_id,
        deliveryStatus: "PENDING_SEND",
        committedAt: secsToIso(revision.created_at),
    }
}

export async function manualPausePublication(
    command: ManualPauseCommand,
): Promise<ManualPauseResult> {
    await apiPut<BackendPublication>(
        `/admin/product-publications/${encodeURIComponent(command.publicationId)}`,
        {
            version: Number(command.expectedObjectVersion) || 1,
            status: "paused",
        },
    )

    return {
        status: "succeeded",
        revisionId: "",
        revisionNo: 0,
        deliveryId: "",
        committedAt: new Date().toISOString(),
    }
}

export async function retryDelivery(
    command: RetryDeliveryCommand,
): Promise<RetryDeliveryResult> {
    // 重试：对当前发布的最新修订再次 deliver（幂等键 = requestId）
    const revisions = await apiGet<BackendRevision[]>(
        `/admin/product-publications/${encodeURIComponent(command.publicationId)}/revisions`,
    ).catch(() => [] as BackendRevision[])
    const latest = revisions[0]
    if (!latest) {
        return {
            status: "blocked",
            code: "NO_REVISION",
            message: "无可重试的发布修订",
        }
    }

    const result = await apiPost<BackendDeliveryResult>(
        `/admin/product-publications/${encodeURIComponent(command.publicationId)}/revisions/${latest.revision_no}/deliver`,
        { idempotency_key: command.requestId },
    )

    const st = mapDeliveryStatus(result.delivery_status)
    return {
        status: "succeeded",
        deliveryId: result.delivery_id,
        attemptCount: 1,
        deliveryStatus: st,
    }
}
