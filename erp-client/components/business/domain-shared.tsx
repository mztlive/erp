import * as React from "react"

import { Badge } from "@/components/ui/badge"
import { Card } from "@/components/ui/card"

export type CardProps = React.ComponentProps<typeof Card>

export type DomainDateTime = Readonly<{
    dateTime: string
    label: React.ReactNode
}>

export type DomainPanelProps = Omit<CardProps, "children">

export function DomainTime({ value }: { value: DomainDateTime }) {
    return (
        <time className="num" dateTime={value.dateTime}>
            {value.label}
        </time>
    )
}

export function ShanghaiTime({ value }: { value: DomainDateTime }) {
    return (
        <span className="flex flex-wrap items-center gap-2">
            <DomainTime value={value} />
            <Badge variant="neutral">Asia/Shanghai</Badge>
        </span>
    )
}

export function NumericValue({ children }: { children: React.ReactNode }) {
    return <span className="num font-medium">{children}</span>
}
