import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const descriptionListVariants = cva("grid gap-x-6 gap-y-4", {
    variants: {
        columns: {
            one: "grid-cols-1",
            two: "grid-cols-1 @[440px]/description-list:grid-cols-2",
            three: "grid-cols-1 @[440px]/description-list:grid-cols-2 @[740px]/description-list:grid-cols-3",
            four: "grid-cols-1 @[440px]/description-list:grid-cols-2 @[740px]/description-list:grid-cols-3 @[960px]/description-list:grid-cols-4",
        },
    },
    defaultVariants: {
        columns: "two",
    },
})

function DescriptionList({
    className,
    columns,
    ...props
}: React.ComponentProps<"dl"> & VariantProps<typeof descriptionListVariants>) {
    return (
        <div
            data-slot="description-list-container"
            className="@container/description-list min-w-0"
        >
            <dl
                data-slot="description-list"
                className={cn(descriptionListVariants({ columns }), className)}
                {...props}
            />
        </div>
    )
}

function DescriptionItem({ className, ...props }: React.ComponentProps<"div">) {
    return (
        <div
            data-slot="description-item"
            className={cn("min-w-0 space-y-1", className)}
            {...props}
        />
    )
}

function DescriptionTerm({ className, ...props }: React.ComponentProps<"dt">) {
    return (
        <dt
            data-slot="description-term"
            className={cn(
                "text-xs font-medium text-muted-foreground",
                className,
            )}
            {...props}
        />
    )
}

function DescriptionDetails({
    className,
    ...props
}: React.ComponentProps<"dd">) {
    return (
        <dd
            data-slot="description-details"
            className={cn(
                "min-w-0 break-words text-body-compact text-foreground",
                className,
            )}
            {...props}
        />
    )
}

export {
    DescriptionList,
    DescriptionItem,
    DescriptionTerm,
    DescriptionDetails,
    descriptionListVariants,
}
