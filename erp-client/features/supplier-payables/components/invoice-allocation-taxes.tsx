"use client"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { InvoiceFormApi } from "@/features/supplier-payables/lib/allocation-form-types"
import type { AllocationSessionView } from "@/features/supplier-payables/types"

export const InvoiceAllocationTaxes = ({
    form,
    pool,
    selected,
    disabled,
}: {
    form: Pick<InvoiceFormApi, "AppField">
    pool: AllocationSessionView["pool"]
    selected: ReadonlySet<string>
    disabled: boolean
}) => {
    if (selected.size <= 1) return null
    return (
        <section className="space-y-3 rounded-lg border p-4">
            <h3 className="text-sm font-semibold">分配税额</h3>
            <p className="text-xs text-muted-foreground">
                按发票实际内容填写每笔应付的税额；合计须与票面税额一致，不含税金额由分配金额减去税额计算。
            </p>
            <form.AppField name="allocationTaxes">
                {(field) => (
                    <div className="space-y-3">
                        {pool
                            .filter((row) => selected.has(row.payableAccountId))
                            .map((row) => {
                                const id = `supplier-invoice-tax-${toAutomationIdSegment(row.payableAccountId)}`
                                return (
                                    <div
                                        key={row.payableAccountId}
                                        className="grid items-center gap-2 sm:grid-cols-2"
                                    >
                                        <Label htmlFor={id}>
                                            {row.sourceDocumentNo} · 分配税额
                                        </Label>
                                        <Input
                                            id={id}
                                            inputMode="decimal"
                                            value={
                                                field.state.value?.[
                                                    row.payableAccountId
                                                ] ?? ""
                                            }
                                            disabled={disabled}
                                            onChange={(event) =>
                                                field.handleChange({
                                                    ...field.state.value,
                                                    [row.payableAccountId]:
                                                        event.target.value,
                                                })
                                            }
                                        />
                                    </div>
                                )
                            })}
                    </div>
                )}
            </form.AppField>
        </section>
    )
}
