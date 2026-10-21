"use client"

import * as React from "react"
import { CalendarDaysIcon } from "lucide-react"
import { useAppForm } from "@/components/form"
import { TableToolbar } from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"

export function InquiryDueDateBatchBar({
    count,
    total,
    disabled,
    onSelectAll,
    onClear,
    onApply,
}: {
    count: number
    total: number
    disabled: boolean
    onSelectAll: () => void
    onClear: () => void
    onApply: (date: string) => void
}) {
    const [open, setOpen] = React.useState(false)
    const form = useAppForm({
        defaultValues: { date: "" },
        onSubmit: ({ value }) => {
            if (disabled || !count || !value.date) return
            onApply(value.date)
            setOpen(false)
        },
    })
    const prefix = "sales-inquiry-batch-due-date"
    return (
        <TableToolbar>
            <div className="flex flex-wrap items-center gap-2">
                <span className="text-muted-foreground">
                    {count
                        ? `已选 ${count} 项商品`
                        : "勾选商品后可批量设置交付日"}
                </span>
                <Button
                    id={`${prefix}-select-all`}
                    type="button"
                    variant="ghost"
                    disabled={disabled || count === total}
                    onClick={onSelectAll}
                >
                    全选 {total} 项
                </Button>
                {count > 0 && (
                    <Button
                        id={`${prefix}-clear-selection`}
                        type="button"
                        variant="ghost"
                        disabled={disabled}
                        onClick={onClear}
                    >
                        清空选择
                    </Button>
                )}
                <Popover
                    open={open}
                    onOpenChange={(next) => {
                        if (next) form.reset()
                        setOpen(next)
                    }}
                >
                    <PopoverTrigger
                        id={`${prefix}-open`}
                        render={<Button type="button" variant="outline" />}
                        disabled={disabled || !count}
                    >
                        <CalendarDaysIcon aria-hidden="true" />
                        批量设置交付日
                    </PopoverTrigger>
                    <PopoverContent
                        align="start"
                        className="w-80 max-w-[calc(100vw-2rem)]"
                    >
                        <PopoverTitle>批量设置交付日</PopoverTitle>
                        <PopoverDescription>
                            将覆盖已选 {count}{" "}
                            项商品的要求交付日，应用后仍可逐行调整。
                        </PopoverDescription>
                        <form.AppField name="date">
                            {(field) => (
                                <field.DateField
                                    id={`${prefix}-date`}
                                    label="要求交付日"
                                    clearable={false}
                                    disabled={disabled}
                                    required
                                />
                            )}
                        </form.AppField>
                        <form.Subscribe selector={(state) => state.values.date}>
                            {(date) => (
                                <Button
                                    id={`${prefix}-apply`}
                                    type="button"
                                    disabled={disabled || !count || !date}
                                    onClick={() => void form.handleSubmit()}
                                >
                                    应用到已选 {count} 项商品
                                </Button>
                            )}
                        </form.Subscribe>
                    </PopoverContent>
                </Popover>
            </div>
        </TableToolbar>
    )
}
