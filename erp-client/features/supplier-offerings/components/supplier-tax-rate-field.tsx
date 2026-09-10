"use client"
import { useEffect, useRef } from "react"
import { useQuery } from "@tanstack/react-query"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { apiGet } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { supplierTaxPercentages } from "@/lib/supplier-tax-rates"

/** 常用税率只作录入候选；每条供货关系明确保存一个税率。 */
export const SupplierTaxRateField = ({
    id,
    supplierId,
    value,
    onChange,
    onBlur,
    errors,
    prefill = false,
}: {
    id: string
    supplierId?: string
    value: string
    onChange: (value: string) => void
    onBlur: () => void
    errors?: readonly unknown[]
    prefill?: boolean
}) => {
    const query = useQuery({
        queryKey: ["master-data", "supplier-tax-candidates", supplierId],
        enabled: Boolean(supplierId),
        queryFn: () =>
            apiGet<{
                current_profile?: {
                    invoice_tax_rates?: string[] | null
                    invoice_tax_rate?: string | null
                } | null
            }>(`/admin/suppliers/${encodeURIComponent(supplierId!)}`),
    })
    const profile = query.data?.current_profile
    const percentages = supplierTaxPercentages(
        profile?.invoice_tax_rates,
        profile?.invoice_tax_rate,
    )
        .split("、")
        .filter(Boolean)
    const initialized = useRef<string | undefined>(undefined)
    useEffect(() => {
        if (!query.data) {
            initialized.current = undefined
            return
        }
        if (!prefill || initialized.current === supplierId) return
        initialized.current = supplierId
        if (!value && percentages.length === 1) onChange(percentages[0])
    }, [prefill, query.data, supplierId, value, percentages, onChange])
    return (
        <div className="space-y-2">
            <Label htmlFor={id}>进项税率（%）</Label>
            <Input
                id={id}
                value={value}
                onChange={(event) => onChange(event.target.value)}
                onBlur={onBlur}
                list={`${id}-options`}
                inputMode="decimal"
                required
                aria-invalid={Boolean(errors?.length)}
                placeholder="选择或填写该商品税率"
            />
            <datalist id={`${id}-options`}>
                {percentages.map((rate) => (
                    <option
                        id={`${id}-option-${toAutomationIdSegment(rate)}`}
                        key={rate}
                        value={rate}
                    >
                        {rate}%
                    </option>
                ))}
            </datalist>
            <p className="text-xs text-muted-foreground">
                {query.isFetching
                    ? "正在读取常用税率…"
                    : query.isError
                      ? "常用税率加载失败，可按商品实际税率填写。"
                      : percentages.length
                        ? `供应商常用：${percentages.map((rate) => `${rate}%`).join("、")}。请按本商品选择。`
                        : "供应商未登记常用税率，请按本商品填写。"}
            </p>
            {Boolean(errors?.length) && (
                <p role="alert" className="text-xs text-destructive">
                    请输入 0–100 的有效税率。
                </p>
            )}
        </div>
    )
}
