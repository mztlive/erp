"use client"

import { DatePicker } from "@/components/ui/date-picker"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { cn } from "@/lib/utils"

type ProductEffectiveSectionProps = {
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
    isCreate,
    canRevise,
    effectiveFrom,
    effectiveTo,
    changeReason,
    setEffectiveFrom,
    setEffectiveTo,
    setChangeReason,
}: ProductEffectiveSectionProps) {
    return (
        <fieldset
            id="product-section-effective"
            className={cn(
                "scroll-mt-[var(--product-section-scroll-margin)] space-y-3 border-b border-grid p-5 last:border-b-0",
            )}
            disabled={!canRevise}
        >
            <legend className="sr-only">生效与原因</legend>
            <div className="text-base font-semibold">生效与原因</div>
            <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1.5">
                    <Label htmlFor="ef-from">
                        {masterDataCopy.fieldEffectiveFrom}
                    </Label>
                    <DatePicker
                        value={effectiveFrom || undefined}
                        onValueChange={(next) => setEffectiveFrom(next ?? "")}
                        className="w-full"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="ef-to">
                        {masterDataCopy.fieldEffectiveTo}
                    </Label>
                    <DatePicker
                        value={effectiveTo || undefined}
                        onValueChange={(next) => setEffectiveTo(next ?? "")}
                        className="w-full"
                    />
                </div>
                <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor="reason">
                        {masterDataCopy.fieldChangeReason}
                    </Label>
                    <Textarea
                        id="reason"
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
        </fieldset>
    )
}

export { ProductEffectiveSection }
