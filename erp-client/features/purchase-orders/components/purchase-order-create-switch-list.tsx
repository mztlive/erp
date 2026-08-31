"use client"

import {
    Item,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemTitle,
} from "@/components/ui/item"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type PurchaseOrderCreateSwitchItem = {
    id: string
    title: string
    description: string
}

export type PurchaseOrderCreateSwitchListProps = {
    items: readonly PurchaseOrderCreateSwitchItem[]
    activeId: string
    label: string
    onSelect: (id: string) => void
}

/**
 * 纸质预览弹窗左侧的单据列表：固定宽度、条目留白，避免挤成一叠按钮。
 */
export function PurchaseOrderCreateSwitchList({
    items,
    activeId,
    label,
    onSelect,
}: PurchaseOrderCreateSwitchListProps) {
    return (
        <nav
            className="flex w-full shrink-0 flex-col self-stretch overflow-y-auto border-b border-border bg-muted/40 md:w-72 md:border-r md:border-b-0"
            aria-label={label}
        >
            <p className="px-4 py-3 text-xs font-medium text-muted-foreground">
                {label}
                <span className="num ml-1">{items.length}</span>
            </p>
            <ItemGroup className="gap-2 px-3 pb-4" data-size="sm">
                {items.map((item) => {
                    const active = item.id === activeId
                    return (
                        <Item
                            key={item.id}
                            variant={active ? "outline" : "default"}
                            size="sm"
                            render={
                                <button
                                    id={`procurement-orders-create-switch-${toAutomationIdSegment(item.id)}`}
                                    type="button"
                                    aria-label={`${item.title}，${item.description}`}
                                    data-testid={`purchase-create-switch-${item.id}`}
                                />
                            }
                            className={cn(
                                "w-full cursor-pointer",
                                active
                                    ? "bg-background shadow-xs"
                                    : "hover:bg-background/70",
                            )}
                            aria-current={active ? "true" : undefined}
                            onClick={() => onSelect(item.id)}
                        >
                            <ItemContent className="min-w-0">
                                <ItemTitle className="num w-full max-w-full">
                                    {item.title}
                                </ItemTitle>
                                <ItemDescription className="text-xs">
                                    {item.description}
                                </ItemDescription>
                            </ItemContent>
                        </Item>
                    )
                })}
            </ItemGroup>
        </nav>
    )
}
