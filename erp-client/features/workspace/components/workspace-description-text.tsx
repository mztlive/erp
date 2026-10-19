import { cn } from "@/lib/utils"

/** 系统说明按段落和分号排版；人工填写的意见只保留原有换行。 */
export function WorkspaceDescriptionText({
    text,
    className,
    splitClauses = true,
}: {
    text: string
    className?: string
    splitClauses?: boolean
}) {
    const paragraphs = text
        .trim()
        .split(/\r?\n\s*\r?\n/)
        .filter(Boolean)
    if (paragraphs.length === 0) return null

    return (
        <div
            className={cn(
                "min-w-0 space-y-2 text-sm leading-6 wrap-anywhere",
                className,
            )}
        >
            {paragraphs.map((paragraph, paragraphIndex) => {
                // 保留标点与原文，不把逗号、金额小数或斜杠拆成独立条目。
                const clauses = splitClauses
                    ? paragraph
                          .split(/(?<=[；])\s*|\r?\n/)
                          .map((line) => line.trim())
                          .filter(Boolean)
                    : [paragraph]
                if (clauses.length === 1) {
                    return (
                        <p key={paragraphIndex} className="whitespace-pre-wrap">
                            {paragraph}
                        </p>
                    )
                }
                // 逐行说明的总述独占一行，普通“字段：值”仍随原条目展示。
                const introduction = clauses[0].match(
                    /^([^：\n]+：)\s*(第\s*\d+\s*行.+)$/,
                )
                const entries = introduction
                    ? [introduction[2], ...clauses.slice(1)]
                    : clauses
                return (
                    <div key={paragraphIndex} className="space-y-2">
                        {introduction && (
                            <p className="font-medium">{introduction[1]}</p>
                        )}
                        <ul className="list-disc space-y-2 pl-4 marker:text-muted-foreground">
                            {entries.map((entry, index) => (
                                <li
                                    key={index}
                                    className="pl-1 whitespace-pre-wrap"
                                >
                                    {entry}
                                </li>
                            ))}
                        </ul>
                    </div>
                )
            })}
        </div>
    )
}
