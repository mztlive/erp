/** 按销售 SKU 批量读取当前有效供给及供应商能力修订。 */

import { apiGet } from "@/lib/api"

import type {
    BackendPage,
    BackendSupplierDetail,
    BackendSupplierOffering,
} from "./backend-types"

export type ProcurementSupplyOption = {
    skuId: string
    supplierId: string
    offeringRevisionId: string
    offeringRevisionNo: number
    costGross: string
    bulkCostGross: string
    dropshipCostGross: string
    bulkMinimumOrderQuantity: string
    inputTaxRate: string
    freightAmount: string
    serviceFeeAmount: string
    capabilities: Array<{
        revisionId: string
        label: string
        capabilityCode: string
    }>
}

const CAPABILITY_LABEL: Record<string, string> = {
    physical: "实物商品",
    virtual: "虚拟商品",
    offline_service: "线下服务",
    api: "API",
    printing: "印刷",
}

/**
 * 按销售 SKU 批量读取当前有效供给及供应商能力修订。
 *
 * @param skuIds 销售提交行中的公司 SKU 集合。
 * @returns 可用于采购确认分行选择的不可变版本选项。
 */
export const fetchProcurementSupplyOptions = async (
    skuIds: readonly string[],
): Promise<ProcurementSupplyOption[]> => {
    const today = new Date().toISOString().slice(0, 10)
    const uniqueSkuIds = [...new Set(skuIds.filter(Boolean))]
    const offeringPages = await Promise.all(
        uniqueSkuIds.map((skuId) =>
            apiGet<BackendPage<BackendSupplierOffering>>(
                "/admin/supplier-offerings",
                { sku_id: skuId, page: 1, page_size: 100 },
            ),
        ),
    )
    const offerings = offeringPages
        .flatMap((page) => page.items)
        .filter(
            (offering) =>
                offering.status === "ACTIVE" &&
                Boolean(offering.current_revision_id) &&
                offering.availability_status === "AVAILABLE" &&
                (!offering.valid_from || offering.valid_from <= today) &&
                (!offering.valid_to || today <= offering.valid_to) &&
                (offering.available_quantity == null ||
                    Number(offering.available_quantity) > 0),
        )
    const supplierIds = [...new Set(offerings.map((row) => row.supplier_id))]
    const supplierDetails = await Promise.all(
        supplierIds.map(async (supplierId) => ({
            supplierId,
            detail: await apiGet<BackendSupplierDetail>(
                `/admin/suppliers/${encodeURIComponent(supplierId)}`,
            ),
        })),
    )
    const capabilitiesBySupplier = new Map(
        supplierDetails.map(({ supplierId, detail }) => [
            supplierId,
            detail.capabilities
                .filter(
                    (capability) =>
                        capability.status === "active" &&
                        Boolean(capability.current_revision_id) &&
                        capability.valid_from <= today &&
                        (!capability.valid_to || today <= capability.valid_to),
                )
                .map((capability) => ({
                    revisionId: capability.current_revision_id!,
                    label:
                        CAPABILITY_LABEL[capability.capability_code] ??
                        "供应商能力",
                    capabilityCode: capability.capability_code,
                })),
        ]),
    )
    return offerings.map((offering) => ({
        skuId: offering.sku_id,
        supplierId: offering.supplier_id,
        offeringRevisionId: offering.current_revision_id!,
        offeringRevisionNo: offering.current_revision_no ?? 1,
        costGross:
            offering.bulk_supply_price_gross ??
            offering.dropship_supply_price_gross ??
            "",
        bulkCostGross: offering.bulk_supply_price_gross ?? "",
        dropshipCostGross: offering.dropship_supply_price_gross ?? "",
        bulkMinimumOrderQuantity: offering.bulk_minimum_order_quantity ?? "1",
        inputTaxRate: offering.input_tax_rate ?? "",
        freightAmount: offering.freight_amount ?? "0",
        serviceFeeAmount: offering.service_fee_amount ?? "0",
        capabilities: capabilitiesBySupplier.get(offering.supplier_id) ?? [],
    }))
}
