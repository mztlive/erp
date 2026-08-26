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
            className="flex items-center gap-1.5"
            data-testid="sales-create-batch-due-date-bar"
        >
            <Field orientation="horizontal" className="w-auto gap-1.5">
                <FieldLabel
                    htmlFor="sales-line-batch-due-date"
                    className="shrink-0 text-xs text-muted-foreground"
                >
                    批量交期
                </FieldLabel>
                <DatePicker
                    id="sales-line-batch-due-date"
                    size="sm"
                    value={dueDate || undefined}
                    onValueChange={(next) => setDueDate(next ?? "")}
                    placeholder="选择日期"
                    clearable={false}
                    className="w-40"
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
                应用到全部
            </Button>
        </div>
    )
}
