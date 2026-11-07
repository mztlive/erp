import { Clock } from "lucide-react"

/** 结束后移除所有可写表单。 */
export const PublicSelectionEnded = () => {
    return (
        <main className="mx-auto flex min-h-screen max-w-md flex-col items-center justify-center p-6 text-center bg-background">
            <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-3xl bg-muted/70 text-muted-foreground">
                <Clock className="h-8 w-8" />
            </div>
            <h1 className="text-xl font-bold text-foreground">选品已结束</h1>
            <p className="mt-2 text-sm text-muted-foreground">
                链接已失效，请联系销售核对。
            </p>
            <p className="mt-4 text-xs text-muted-foreground">
                如需重新选品或确认报价，请联系您的专属销售经理获取新链接。
            </p>
        </main>
    )
}
