import {
    BanIcon,
    CircleAlertIcon,
    CircleCheckIcon,
    CircleDashedIcon,
    CircleDotIcon,
    type LucideIcon,
    TriangleAlertIcon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

/**
 * 单据状态徽章。
 *
 * 交互规范 §4.5 / §19：状态必须同时使用文字、图标和颜色，禁止只用红黄绿圆点。
 * 因此 `label` 为必填，且始终渲染图标 —— 从类型上杜绝「仅靠颜色表意」的用法。
 */
type StatusTone =
    | "neutral" // 草稿、未开始、不适用
    | "info" // 处理中、待接收、待复核
    | "success" // 已生效、已完成、已结清
    | "warning" // 待审批、部分履约、版本差异
    | "destructive" // 接收失败、待处理异常
    | "void" // 已作废、已关闭

const toneIcon: Record<StatusTone, LucideIcon> = {
    neutral: CircleDashedIcon,
    info: CircleDotIcon,
    success: CircleCheckIcon,
    warning: CircleAlertIcon,
    destructive: TriangleAlertIcon,
    void: BanIcon,
}

const toneVariant: Record<
    StatusTone,
    "neutral" | "info" | "success" | "warning" | "destructive"
> = {
    neutral: "neutral",
    info: "info",
    success: "success",
    warning: "warning",
    destructive: "destructive",
    void: "neutral",
}

function StatusBadge({
    tone = "neutral",
    icon,
    label,
    className,
    ...props
}: Omit<React.ComponentProps<typeof Badge>, "variant" | "children"> & {
    tone?: StatusTone
    /** 状态文字。必填：状态不得仅靠颜色表达。 */
    label: string
    /** 覆盖默认图标，例如用 ClockIcon 表示「等待中」。 */
    icon?: LucideIcon
}) {
    const Icon = icon ?? toneIcon[tone]

    return (
        <Badge
            variant={toneVariant[tone]}
            className={cn(
                tone === "void" && "line-through opacity-80",
                className,
            )}
            {...props}
        >
            <Icon data-icon="inline-start" aria-hidden="true" />
            {label}
        </Badge>
    )
}

export { StatusBadge, type StatusTone }
