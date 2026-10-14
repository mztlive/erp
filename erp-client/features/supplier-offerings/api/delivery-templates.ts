import { apiDelete, apiGet, apiPost, apiPut } from "@/lib/api"

import type { DeliveryTemplateInput, DeliveryTemplateView } from "../types"

function toWire(input: DeliveryTemplateInput) {
    const ruleSet = (set: DeliveryTemplateInput["dropship"]) => ({
        default_term: set.defaultTerm,
        rules: set.rules.map((rule) => ({
            region_code: rule.regionCode,
            term: rule.term,
        })),
    })
    return {
        name: input.name.trim(),
        is_default: input.isDefault,
        dropship: ruleSet(input.dropship),
        bulk: ruleSet(input.bulk),
        expected_version: input.expectedVersion,
    }
}

/** 查询供应商全部配送模板。 */
export function fetchSupplierDeliveryTemplates(
    supplierId: string,
): Promise<DeliveryTemplateView[]> {
    return apiGet<DeliveryTemplateView[]>(
        `/admin/suppliers/${encodeURIComponent(supplierId)}/delivery-templates`,
    )
}

/** 为供应商新建配送模板。 */
export function createDeliveryTemplate(
    supplierId: string,
    input: DeliveryTemplateInput,
): Promise<DeliveryTemplateView> {
    return apiPost<DeliveryTemplateView>(
        `/admin/suppliers/${encodeURIComponent(supplierId)}/delivery-templates`,
        toWire(input),
    )
}

/** 整体保存配送模板。 */
export function updateDeliveryTemplate(
    id: string,
    input: DeliveryTemplateInput,
): Promise<DeliveryTemplateView> {
    return apiPut<DeliveryTemplateView>(
        `/admin/supplier-delivery-templates/${encodeURIComponent(id)}`,
        toWire(input),
    )
}

/** 删除未被供给使用的配送模板。 */
export function deleteDeliveryTemplate(
    id: string,
    expectedVersion: number,
): Promise<void> {
    return apiDelete<void>(
        `/admin/supplier-delivery-templates/${encodeURIComponent(id)}`,
        { body: { expected_version: expectedVersion } },
    )
}

/** 为同一供应商的供给指定配送模板；模板为空表示跟随供应商默认。 */
export function assignDeliveryTemplate(
    offeringIds: readonly string[],
    templateId: string | null,
): Promise<{ updated_count: number }> {
    return apiPost<{ updated_count: number }>(
        "/admin/supplier-offerings/delivery-template",
        { offering_ids: offeringIds, template_id: templateId },
    )
}
