"use client"

import { LoginMark, LoginStageArt } from "./login-stage-art"

const TRACKS = ["单据流转", "库存履约", "往来结算"] as const

/**
 * 登录品牌舞台：深色画布 + SVG 动效。
 * 桌面为整列侧栏，窄屏为顶部短栏。
 */
export function LoginStage() {
    return (
        <aside className="relative isolate flex min-h-40 flex-col overflow-hidden bg-foreground text-background lg:min-h-svh">
            <LoginStageArt className="pointer-events-none absolute inset-0 size-full" />
            <div className="pointer-events-none absolute inset-x-0 top-0 h-28 bg-linear-to-b from-foreground via-foreground/80 to-transparent" />
            <div className="pointer-events-none absolute inset-x-0 bottom-0 h-24 bg-linear-to-t from-foreground to-transparent lg:h-64" />

            <div className="relative z-10 flex flex-1 flex-col justify-between gap-8 p-6 lg:p-12">
                <div className="flex items-center gap-3">
                    <LoginMark inverted />
                    <div className="flex flex-col">
                        <span className="text-sm font-semibold tracking-wide">
                            福尚云 ERP
                        </span>
                        <span className="text-xs text-background/65">
                            员工福利经营协同
                        </span>
                    </div>
                </div>

                <div className="hidden max-w-lg flex-col gap-5 lg:flex">
                    <h1 className="font-heading text-3xl font-medium tracking-tight text-balance">
                        把单据、库存和往来，放进同一本账
                    </h1>
                    <p className="text-sm leading-6 text-background/70">
                        后台账号进入工作台后，待办、审批与履约进度都在同一工作面完成，不必在菜单之间来回跳。
                    </p>
                    <ul className="flex flex-wrap gap-2">
                        {TRACKS.map((track) => (
                            <li
                                key={track}
                                className="rounded-full border border-background/15 bg-background/8 px-3 py-1 text-xs text-background/80"
                            >
                                {track}
                            </li>
                        ))}
                    </ul>
                </div>
            </div>
        </aside>
    )
}
