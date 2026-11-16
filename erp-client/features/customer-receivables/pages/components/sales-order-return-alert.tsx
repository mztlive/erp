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
                {salesOrderId && salesOrderNo
                    ? `正在处理销售单 ${salesOrderNo} 的往来账款，办理完成后可直接返回该销售单。`
                    : "正在处理该销售单的往来账款，办理完成后可直接返回该销售单。"}
                <Button
                    id="customer-receivables-return-alert-back"
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
