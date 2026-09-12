"use client"

import * as React from "react"
import { CalendarDaysIcon } from "lucide-react"
import { QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import { Field, FieldLabel } from "@/components/ui/field"
import {
    Popover,
    PopoverTrigger,
    PopoverContent,
    PopoverTitle,
    PopoverDescription,
} from "@/components/ui/popover"

export type SalesOrderCreateDueDateBatchBarProps = {
    lineCount: number
    onApply: (dueDate: string) => void
}

export function SalesOrderCreateDueDateBatchBar({
    lineCount,
    onApply,
}: SalesOrderCreateDueDateBatchBarProps) {
    const [dueDate, setDueDate] = React.useState("")
    const [open, setOpen] = React.useState(false)
    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger
                id="sales-orders-create-batch-due-date-open"
                render={<Button type="button" variant="outline" />}
                disabled={lineCount === 0}
            >
                <CalendarDaysIcon aria-hidden="true" />
                设置统一交付日
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="w-80 max-w-[calc(100vw-2rem)]"
                data-testid="sales-create-batch-due-date-bar"
            >
                <PopoverTitle>设置统一交付日</PopoverTitle>
                <PopoverDescription>
                    将覆盖全部{" "}
                    <QuantityValue unit="" value={String(lineCount)} />{" "}
                    条明细的承诺交付日，应用后仍可逐行调整。
                </PopoverDescription>
                <Field>
                    <FieldLabel htmlFor="sales-orders-create-batch-due-date">
                        承诺交付日
                    </FieldLabel>
                    <DatePicker
                        id="sales-orders-create-batch-due-date"
                        value={dueDate || undefined}
                        onValueChange={(next) => setDueDate(next ?? "")}
                        placeholder="选择日期"
                        clearable={false}
                        className="w-full"
                    />
                </Field>
                <Button
                    id="sales-orders-create-batch-due-date-apply"
                    type="button"
                    disabled={!dueDate || lineCount === 0}
                    onClick={() => {
                        onApply(dueDate)
                        setOpen(false)
                    }}
                    data-testid="sales-create-batch-due-date-apply"
                >
                    应用到全部明细
                </Button>
            </PopoverContent>
        </Popover>
    )
}
