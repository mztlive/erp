"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

/**
 * 毛利风险只读提示。不提供决定、提交或独立工作面入口。
 */
export function SalesOrderMarginRiskHint({ hint }: { hint: string }) {
    return (
        <Alert variant="warning">
            <AlertTitle>毛利风险</AlertTitle>
            <AlertDescription>{hint}</AlertDescription>
        </Alert>
    )
}
