"use client"

import { Collapsible as CollapsiblePrimitive } from "@base-ui/react/collapsible"

function Collapsible({ ...props }: CollapsiblePrimitive.Root.Props) {
    return <CollapsiblePrimitive.Root data-slot="collapsible" {...props} />
}

function CollapsibleTrigger({
    id,
    ...props
}: CollapsiblePrimitive.Trigger.Props & { id?: string }) {
    return (
        <CollapsiblePrimitive.Trigger
            id={id}
            data-slot="collapsible-trigger"
            {...props}
        />
    )
}

function CollapsibleContent({ ...props }: CollapsiblePrimitive.Panel.Props) {
    return (
        <CollapsiblePrimitive.Panel
            data-slot="collapsible-content"
            {...props}
        />
    )
}

export { Collapsible, CollapsibleTrigger, CollapsibleContent }
