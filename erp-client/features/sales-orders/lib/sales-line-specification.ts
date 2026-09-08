/** 旧销售草稿曾把 SKU 身份写进规格快照；身份相同时不作为规格展示或重新保存。 */
export function salesLineSpecification(
    specification?: string | null,
    skuId?: string | null,
    skuRevisionId?: string | null,
): string {
    const text = specification?.trim() ?? ""
    if (!text || text === skuId?.trim() || text === skuRevisionId?.trim())
        return ""
    return text
}
