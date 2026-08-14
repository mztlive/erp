"use client"

import Link from "next/link"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"

type SalesOrderReturnAlertProps = {
    salesOrderId: string | undefined
    salesOrderNo: string | undefined
    returnTo: string
}

/** W05 销售单票款入口返回上下文提示条。 */
export function SalesOrderReturnAlert({
    salesOrderId,
    salesOrderNo,
    returnTo,
}: SalesOrderReturnAlertProps) {
    return (
        <Alert variant="info">
            <AlertTitle>销售单票款入口</AlertTitle>
            <AlertDescription className="flex flex-wrap items-center gap-2">
                已携带来源页签返回上下文
                {salesOrderId
                    ? ` · 销售单 ${salesOrderNo ?? ""}`
                    : ""}
                。核销完成后可回到销售单。
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    render={<Link href={returnTo} />}
                >
                    返回销售单
                </Button>
            </AlertDescription>
        </Alert>
    )
}
