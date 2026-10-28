"use client"

import { EraserIcon, RotateCcwIcon, Undo2Icon } from "lucide-react"
import { Button } from "@/components/ui/button"
import type {
    BatchClearField,
    useBatchWorkspace,
} from "../../hooks/use-batch-workspace"

export function BatchClearButton({
    workspace,
    field,
    label = "清除",
}: {
    workspace: ReturnType<typeof useBatchWorkspace>
    field: BatchClearField
    label?: string
}) {
    const active = workspace.values.clearFields.includes(field)
    const reset = field === "delivery_template_id"
    const Icon = active ? Undo2Icon : reset ? RotateCcwIcon : EraserIcon
    return (
        <Button
            id={`supplier-catalog-batch-clear-${field.replaceAll("_", "-")}`}
            type="button"
            variant="ghost"
            size="xs"
            className="shrink-0 text-muted-foreground"
            aria-pressed={active}
            onClick={() => workspace.setDefaultClear(field, !active)}
        >
            <Icon aria-hidden />
            {active ? (reset ? "取消恢复" : "取消清除") : label}
        </Button>
    )
}
