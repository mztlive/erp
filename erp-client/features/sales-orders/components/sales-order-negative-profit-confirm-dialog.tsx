"use client"

import { TriangleAlertIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogMedia,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"

const ID_PREFIX = "sales-orders-negative-profit-confirm"

/** 负毛利提交前的二次确认；关闭或返回修改由表单恢复。 */
export function SalesOrderNegativeProfitConfirmDialog({
    grossProfit,
    open,
    onOpenChange,
    onConfirm,
}: {
    grossProfit: string
    open: boolean
    onOpenChange: (open: boolean) => void
    onConfirm: () => void
}) {
    return (
        <AlertDialog open={open} onOpenChange={onOpenChange}>
            <AlertDialogContent>
                <AlertDialogHeader>
                    <AlertDialogMedia className="bg-destructive-soft text-destructive-soft-foreground">
                        <TriangleAlertIcon aria-hidden="true" />
                    </AlertDialogMedia>
                    <AlertDialogTitle>预估毛利为负</AlertDialogTitle>
                    <AlertDialogDescription>
                        该销售单的预估毛利（含税）为{" "}
                        <MoneyValue
                            value={grossProfit}
                            className="font-semibold text-destructive"
                        />
                        。提交后仍会进入审批，请确认成交价与采购供给价填写无误。
                    </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                    <AlertDialogCancel id={`${ID_PREFIX}-cancel`}>
                        返回修改
                    </AlertDialogCancel>
                    <AlertDialogAction
                        id={`${ID_PREFIX}-submit`}
                        variant="destructive"
                        onClick={onConfirm}
                    >
                        仍然提交
                    </AlertDialogAction>
                </AlertDialogFooter>
            </AlertDialogContent>
        </AlertDialog>
    )
}
