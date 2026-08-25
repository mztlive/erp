/**
 * W22 商品发布 · 变更命令（发布修订 / 手动暂停 / 重试发送）。
 */

import { apiPost, apiPut } from "@/lib/api"

import {
    mapDeliveryStatus,
    secsToIso,
    toBackendMediaRole,
    toBackendSaleStatus,
} from "@/features/product-publications/api/mappers"
import type {
    BackendPublication,
    BackendRevisionCommit,
    BackendRetryDeliveryResult,
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
    const committed = await apiPost<BackendRevisionCommit>(
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

    const revision = committed.revision

    return {
        status: "succeeded",
        operationId: committed.operation_id,
        publicationId: command.publicationId,
        revisionId: revision.id,
        revisionNo: revision.revision_no,
        deliveryId: committed.delivery_id,
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
    const result = await apiPost<BackendRetryDeliveryResult>(
        `/admin/product-publication-deliveries/${encodeURIComponent(command.deliveryId)}/retry`,
        { request_id: command.requestId },
    )

    const st = mapDeliveryStatus(result.delivery_status)
    return {
        status: "succeeded",
        deliveryId: result.delivery_id,
        attemptCount: result.attempt_count,
        deliveryStatus: st,
    }
}
