"use client"

import * as React from "react"
import { CalendarDaysIcon } from "lucide-react"
import { detailWorkspaceStyles } from "@/components/business"
import { useAppForm } from "@/components/form"
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
    onApply,
}: {
    count: number
    total: number
    disabled: boolean
    onApply: (date: string) => void
}) {
    const [open, setOpen] = React.useState(false)
    const targetCount = count > 0 ? count : total
    const scopedToSelection = count > 0
    const form = useAppForm({
        defaultValues: { date: "" },
        onSubmit: ({ value }) => {
            if (disabled || !targetCount || !value.date) return
            onApply(value.date)
            setOpen(false)
        },
    })
    const prefix = "sales-inquiry-batch-due-date"

    return (
        <Popover
            open={open}
            onOpenChange={(next) => {
                if (next) form.reset()
                setOpen(next)
            }}
        >
            <PopoverTrigger
                id={`${prefix}-open`}
                render={
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                    />
                }
                disabled={disabled || targetCount === 0}
            >
                <CalendarDaysIcon
                    aria-hidden="true"
                    data-icon="inline-start"
                    className="size-3.5"
                />
                {scopedToSelection
                    ? `批量设交付日（${count}）`
                    : "统一设交付日"}
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="w-80 max-w-[calc(100vw-2rem)]"
            >
                <PopoverTitle>
                    {scopedToSelection ? "批量设置交付日" : "统一设置交付日"}
                </PopoverTitle>
                <PopoverDescription>
                    将覆盖{scopedToSelection ? "已选" : "全部"}{" "}
                    <span className="num">{targetCount}</span>{" "}
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
                            size="sm"
                            className="w-full"
                            disabled={disabled || !targetCount || !date}
                            onClick={() => void form.handleSubmit()}
                        >
                            应用到{scopedToSelection ? "已选" : "全部"}{" "}
                            {targetCount} 项商品
                        </Button>
                    )}
                </form.Subscribe>
            </PopoverContent>
        </Popover>
    )
}
