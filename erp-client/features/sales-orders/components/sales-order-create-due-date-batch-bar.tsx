"use client"

import * as React from "react"

import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import { Field, FieldLabel } from "@/components/ui/field"

export type SalesOrderCreateDueDateBatchBarProps = {
    lineCount: number
    onApply: (dueDate: string) => void
}

/**
 * 实物/服务明细的批量交期：选一个日期后写到全部行，仍可逐行改。
 */
export function SalesOrderCreateDueDateBatchBar({
    lineCount,
    onApply,
}: SalesOrderCreateDueDateBatchBarProps) {
    const [dueDate, setDueDate] = React.useState("")

    return (
        <div
            className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-3 py-2"
            data-testid="sales-create-batch-due-date-bar"
        >
            <Field orientation="horizontal" className="w-auto gap-2">
                <FieldLabel
                    htmlFor="sales-line-batch-due-date"
                    className="text-sm text-muted-foreground"
                >
                    批量交期
                </FieldLabel>
                <DatePicker
                    id="sales-line-batch-due-date"
                    value={dueDate || undefined}
                    onValueChange={(next) => setDueDate(next ?? "")}
                    placeholder="选择承诺交付日"
                    className="w-44"
                />
            </Field>
            <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={!dueDate || lineCount === 0}
                onClick={() => onApply(dueDate)}
                data-testid="sales-create-batch-due-date-apply"
            >
                应用到全部明细
            </Button>
        </div>
    )
}
