"use client"

import { DatePicker } from "@/components/ui/date-picker"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { ProductSectionFrame } from "@/features/master-data/components/product/product-section-frame"
import { masterDataCopy } from "@/features/master-data/lib/copy"

type ProductEffectiveSectionProps = {
    idPrefix?: string
    isCreate: boolean
    canRevise: boolean
    effectiveFrom: string
    effectiveTo: string
    changeReason: string
    setEffectiveFrom: (next: string) => void
    setEffectiveTo: (next: string) => void
    setChangeReason: (next: string) => void
}

function ProductEffectiveSection({
    idPrefix,
    isCreate,
    canRevise,
    effectiveFrom,
    effectiveTo,
    changeReason,
    setEffectiveFrom,
    setEffectiveTo,
    setChangeReason,
}: ProductEffectiveSectionProps) {
    const prefix = idPrefix ?? "master-data-product-effective"
    return (
        <ProductSectionFrame
            id="product-section-effective"
            title="生效与原因"
            disabled={!canRevise}
        >
            <div className="grid gap-4 sm:grid-cols-2">
                <div className="space-y-1.5">
                    <Label htmlFor={`${prefix}-from`}>
                        {masterDataCopy.fieldEffectiveFrom}
                    </Label>
                    <DatePicker
                        id={`${prefix}-from`}
                        value={effectiveFrom || undefined}
                        onValueChange={(next) => setEffectiveFrom(next ?? "")}
                        className="w-full"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor={`${prefix}-to`}>
                        {masterDataCopy.fieldEffectiveTo}
                    </Label>
                    <DatePicker
                        id={`${prefix}-to`}
                        value={effectiveTo || undefined}
                        onValueChange={(next) => setEffectiveTo(next ?? "")}
                        className="w-full"
                    />
                </div>
                <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor={`${prefix}-reason`}>
                        {masterDataCopy.fieldChangeReason}
                    </Label>
                    <Textarea
                        id={`${prefix}-reason`}
                        value={changeReason}
                        onChange={(e) => setChangeReason(e.target.value)}
                        rows={2}
                        placeholder={
                            isCreate
                                ? "新建原因"
                                : "说明本次修改内容，保存后形成新版本"
                        }
                    />
                </div>
            </div>
        </ProductSectionFrame>
    )
}

export { ProductEffectiveSection }
