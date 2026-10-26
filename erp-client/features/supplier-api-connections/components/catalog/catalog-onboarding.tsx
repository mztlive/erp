import type { ReactNode } from "react"
import { PackageSearchIcon, PlugZapIcon } from "lucide-react"
import { BusinessEmptyState } from "@/components/business"
import { cn } from "@/lib/utils"

const steps = [
    { title: "连接供应商", description: "配置接口并完成连接测试" },
    { title: "同步商品", description: "读取供应商授权的商品目录" },
    { title: "选品入库", description: "选择需要经营的商品纳入公司商品库" },
]

export function CatalogOnboarding({
    title,
    description,
    action,
    step,
}: {
    title: string
    description: ReactNode
    action: ReactNode
    step: 0 | 1
}) {
    return (
        <section
            aria-label="供应商品库使用引导"
            className="overflow-hidden rounded-xl border bg-card"
        >
            <BusinessEmptyState
                kind="no-data"
                icon={step === 0 ? PlugZapIcon : PackageSearchIcon}
                title={title}
                description={description}
                action={action}
                className="px-6 py-12 sm:py-16"
            />
            <ol className="grid gap-5 border-t bg-muted/30 px-6 py-6 sm:grid-cols-3 sm:gap-8">
                {steps.map((item, index) => (
                    <li
                        key={item.title}
                        aria-current={index === step ? "step" : undefined}
                        className="flex min-w-0 items-start gap-3"
                    >
                        <span
                            aria-hidden="true"
                            className={cn(
                                "flex size-7 shrink-0 items-center justify-center rounded-full border text-xs font-medium",
                                index === step
                                    ? "border-primary bg-primary text-primary-foreground"
                                    : "bg-background text-muted-foreground",
                            )}
                        >
                            {index + 1}
                        </span>
                        <div className="space-y-1">
                            <p className="text-sm font-medium">{item.title}</p>
                            <p className="text-xs leading-relaxed text-muted-foreground">
                                {item.description}
                            </p>
                        </div>
                    </li>
                ))}
            </ol>
        </section>
    )
}
