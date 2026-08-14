"use client"

import { CircleAlertIcon, CircleCheckIcon } from "lucide-react"

import { getErrorMessage } from "@/lib/api/errors"
import { errorMessage } from "@/features/sales-orders/lib/sales-order-create-model"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

export type SalesOrderCreateAlertsProps = {
    /** 当前登录用户信息查询错误；为 null 表示未出错。 */
    profileError: Error | null
    /** 提交命令失败或结果待确认时的提示。 */
    formalFailure: { unknown: boolean; description: string } | null
    /** 新建销售单 mutation 错误；为 null 表示未出错。 */
    createError: Error | null
    draftSaved: { documentNumber: string; savedAt: Date } | null
}

export function SalesOrderCreateAlerts({
    profileError,
    formalFailure,
    createError,
    draftSaved,
}: SalesOrderCreateAlertsProps) {
    return (
        <>
            {profileError ? (
                <Alert variant="destructive">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>当前用户信息加载失败</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(
                            profileError,
                            "无法获取当前登录用户，请刷新后重试。",
                        )}
                    </AlertDescription>
                </Alert>
            ) : null}

            {formalFailure ? (
                <Alert
                    variant={formalFailure.unknown ? "warning" : "destructive"}
                >
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>
                        {formalFailure.unknown
                            ? "处理结果待确认"
                            : "操作未完成"}
                    </AlertTitle>
                    <AlertDescription>
                        {formalFailure.description}
                    </AlertDescription>
                </Alert>
            ) : createError ? (
                <Alert variant="destructive">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>销售单未创建</AlertTitle>
                    <AlertDescription>
                        {errorMessage(createError)}
                    </AlertDescription>
                </Alert>
            ) : null}

            {draftSaved ? (
                <Alert variant="success">
                    <CircleCheckIcon aria-hidden="true" />
                    <AlertTitle>草稿已保存</AlertTitle>
                    <AlertDescription>
                        销售单 {draftSaved.documentNumber} 已保存为草稿（
                        {draftSaved.savedAt.toLocaleTimeString("zh-CN")}
                        ）。当前内容仍保留在本页，可继续完善后提交；草稿也会出现在销售单列表中。
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}
