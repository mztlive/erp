import {
    BULK_TERM_LABEL,
    DROPSHIP_TERM_LABEL,
    type DeliveryRuleSetView,
    type DeliveryTerm,
} from "../types"

function termLabel(mode: "dropship" | "bulk", term: DeliveryTerm) {
    return mode === "dropship" && term !== "FREIGHT_EXTRA"
        ? DROPSHIP_TERM_LABEL[term]
        : BULK_TERM_LABEL[term]
}

/** 规则摘要，例如「其他地区包邮；运费另计：上海市；不配送：新疆维吾尔自治区」。 */
export function ruleSetSummary(
    mode: "dropship" | "bulk",
    set: DeliveryRuleSetView,
): string {
    const groups = new Map<DeliveryTerm, string[]>()
    for (const rule of set.rules) {
        const names = groups.get(rule.term) ?? []
        names.push(rule.region_name)
        groups.set(rule.term, names)
    }
    return [
        `其他地区${termLabel(mode, set.default_term)}`,
        ...[...groups.entries()].map(
            ([term, names]) => `${termLabel(mode, term)}：${names.join("、")}`,
        ),
    ].join("；")
}
