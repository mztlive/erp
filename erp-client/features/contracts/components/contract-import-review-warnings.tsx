import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

const FIELDS = {
    contract_no: { label: "合同编号", scope: "terms" },
    customer_name: { label: "对方签约名称", scope: "customer" },
    customer_credit_code: { label: "对方统一社会信用代码", scope: "customer" },
    company_name: { label: "我方签约名称", scope: "company" },
    company_credit_code: { label: "我方统一社会信用代码", scope: "company" },
    settlement_name: { label: "结算主体", scope: "company" },
    settlement_credit_code: {
        label: "结算主体统一社会信用代码",
        scope: "company",
    },
    payment_terms: { label: "付款条件", scope: "terms" },
    invoice_type: { label: "开票要求", scope: "terms" },
    tax_point: { label: "税率", scope: "terms" },
    signed_at: { label: "签订日期", scope: "dates" },
    valid_from: { label: "生效日期", scope: "dates" },
    valid_to: { label: "有效期止", scope: "dates" },
    business_scope: { label: "业务范围", scope: "dates" },
} as const

type Scope = (typeof FIELDS)[keyof typeof FIELDS]["scope"] | "other"

export function ContractImportReviewWarnings({
    warnings,
    scope,
}: {
    warnings: string[]
    scope: Scope
}) {
    const entries = Object.entries(FIELDS)
    const relevant = [...new Set(warnings)].filter((warning) => {
        // 服务端按字段英文名标注冲突；跨字段提示在每个相关区域保留。
        const fields = entries.filter(
            ([key, { label }]) =>
                warning.includes(key) ||
                warning.startsWith(`${label}：`) ||
                warning.startsWith(`${label}:`),
        )
        return fields.length
            ? fields.some(([, field]) => field.scope === scope)
            : scope === "other"
    })
    if (!relevant.length) return null
    return (
        <Alert variant="warning">
            <AlertTitle>
                {scope === "other" ? "归档前还需核对" : "请对照原文核对"}
            </AlertTitle>
            <AlertDescription>
                <ul className="list-disc space-y-1 pl-4 text-xs leading-relaxed">
                    {relevant.map((warning) => (
                        <li key={warning}>
                            {entries.reduce(
                                (text, [key, { label }]) =>
                                    text.replaceAll(key, label),
                                warning,
                            )}
                        </li>
                    ))}
                </ul>
            </AlertDescription>
        </Alert>
    )
}
