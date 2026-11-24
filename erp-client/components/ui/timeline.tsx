import * as React from "react"

import { cn } from "@/lib/utils"

function Timeline({ className, ...props }: React.ComponentProps<"ol">) {
    return (
        <ol data-slot="timeline" className={cn("ml-3", className)} {...props} />
    )
}

function TimelineItem({ className, ...props }: React.ComponentProps<"li">) {
    return (
        <li
            data-slot="timeline-item"
            className={cn(
                "relative pb-5 pl-6 last:pb-0 before:pointer-events-none before:absolute before:top-3 before:-bottom-1 before:left-0 before:w-px before:bg-border last:before:hidden",
                className,
            )}
            {...props}
        />
    )
}

function TimelineMarker({ className, ...props }: React.ComponentProps<"span">) {
    return (
        <span
            data-slot="timeline-marker"
            className={cn(
                "absolute -left-3 top-0 z-10 flex size-6 items-center justify-center rounded-full border bg-card text-muted-foreground [&_svg:not([class*='size-'])]:size-3",
                className,
            )}
            {...props}
        />
    )
}

function TimelineHeader({ className, ...props }: React.ComponentProps<"div">) {
    return (
        <div
            data-slot="timeline-header"
            className={cn(
                "flex flex-wrap items-baseline gap-x-2 gap-y-1",
                className,
            )}
            {...props}
        />
    )
}

function TimelineTitle({ className, ...props }: React.ComponentProps<"div">) {
    return (
        <div
            data-slot="timeline-title"
            className={cn("text-body-compact font-medium", className)}
            {...props}
        />
    )
}

function TimelineTime({ className, ...props }: React.ComponentProps<"time">) {
    return (
        <time
            data-slot="timeline-time"
            className={cn("num text-xs text-muted-foreground", className)}
            {...props}
        />
    )
}

function TimelineDescription({
    className,
    ...props
}: React.ComponentProps<"div">) {
    return (
        <div
            data-slot="timeline-description"
            className={cn(
                "mt-1 text-body-compact text-muted-foreground",
                className,
            )}
            {...props}
        />
    )
}

export {
    Timeline,
    TimelineItem,
    TimelineMarker,
    TimelineHeader,
    TimelineTitle,
    TimelineTime,
    TimelineDescription,
}
